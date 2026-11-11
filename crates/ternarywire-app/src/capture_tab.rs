//! Traffic capture through a local proxy (PRD section 4, "Penangkapan lalu
//! lintas lewat proxy"): point a browser, app or device at the proxy, watch
//! requests live, then save them to a collection or export HAR.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use egui::{Align, Layout, RichText, Ui};
use parking_lot::Mutex;

use ternarywire_core::model::Id;
use ternarywire_core::secrets::{FileBackend, KeychainBackend, SecretStore};
use ternarywire_proxy::{
    CaPem, CaStore, CaptureEntry, CaptureFilter, CaptureProxy, CertificateAuthority, EntryKind, EntryState,
    ImportOptions, ProxyConfig,
};

use crate::app::{Msg, TernaryWireApp};
use crate::i18n::{tr, trf};
use crate::kit::{self, Tone};
use crate::server_ui::{self as sv, Cell, Col, Stat, State};
use crate::{icons, theme, widgets};

const COLS: [Col; 6] = [Col::Px(18.0), Col::Px(62.0), Col::Px(66.0), Col::Px(50.0), Col::Fr(1.0), Col::Px(58.0)];

pub enum CaptureMsg {
    Started(Result<CaptureProxy, String>),
    Trusted(bool),
}

/// The capture CA lives in the Keychain (one for the app, not per workspace),
/// so trusting it once covers every workspace.
struct KeychainCa(SecretStore);

impl KeychainCa {
    fn open() -> KeychainCa {
        let file =
            || FileBackend { path: ternarywire_core::settings::AppSettings::config_dir().join("capture-ca.json") };
        let store = match std::env::var("TERNARYWIRE_SECRETS").as_deref() {
            Ok("file") => SecretStore::new(Box::new(file())),
            Ok("memory") => SecretStore::memory(),
            _ if KeychainBackend::available() => {
                SecretStore::new(Box::new(KeychainBackend::for_workspace("capture-proxy")))
            }
            _ => SecretStore::new(Box::new(file())),
        };
        KeychainCa(store)
    }
}

impl CaStore for KeychainCa {
    fn load(&self) -> Result<Option<CaPem>, String> {
        Ok(match (self.0.get("ca_cert"), self.0.get("ca_key")) {
            (Some(cert_pem), Some(key_pem)) => Some(CaPem { cert_pem, key_pem }),
            _ => None,
        })
    }

    fn save(&self, pem: &CaPem) -> Result<(), String> {
        self.0.set("ca_cert", &pem.cert_pem);
        self.0.set("ca_key", &pem.key_pem);
        Ok(())
    }
}

pub struct CaptureTab {
    proxy: Option<CaptureProxy>,
    starting: bool,
    port: u16,
    intercept: bool,
    verify_upstream: bool,
    allow_lan: bool,
    passthrough: String,
    include_hosts: String,
    exclude_hosts: String,
    url_contains: String,
    search: String,
    selected: Option<u64>,
    checked: BTreeSet<u64>,
    target: Option<Id>,
    ca: Option<Arc<CertificateAuthority>>,
    trusted: Arc<Mutex<Option<bool>>>,
    error: Option<String>,
    show_ca: bool,
    /// Options open while stopped (`[0]`) and while running (`[1]`): open
    /// while stopped, folded into a summary once the proxy runs.
    options_open: [bool; 2],
    /// Automation: start on the next frame.
    pub auto_start: bool,
    /// Automation: `start`, `stop`, `settings`, `cert`, `select[:n]` (the
    /// newest request, or the n-th row from the top), `tick:<n>,<n>…` (tick
    /// those rows for Save to collection).
    pub auto: Option<String>,
}

impl Default for CaptureTab {
    fn default() -> Self {
        CaptureTab {
            proxy: None,
            starting: false,
            port: ternarywire_proxy::DEFAULT_PORT,
            intercept: true,
            verify_upstream: true,
            allow_lan: false,
            passthrough: String::new(),
            include_hosts: String::new(),
            exclude_hosts: String::new(),
            url_contains: String::new(),
            search: String::new(),
            selected: None,
            checked: BTreeSet::new(),
            target: None,
            ca: None,
            trusted: Arc::new(Mutex::new(None)),
            error: None,
            show_ca: false,
            options_open: [true, false],
            auto_start: false,
            auto: None,
        }
    }
}

fn list(text: &str) -> Vec<String> {
    text.split([',', ' ', '\n']).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect()
}

impl CaptureTab {
    pub fn with_port(port: u16) -> Self {
        CaptureTab { port, ..Default::default() }
    }

    /// Automation: select the newest entry.
    pub fn select_latest(&mut self) {
        self.selected = self
            .proxy
            .as_ref()
            .and_then(|p| p.entries().iter().rev().find(|e| e.kind != EntryKind::Tunnel).map(|e| e.id));
    }

    pub fn on_msg(&mut self, m: CaptureMsg) {
        match m {
            CaptureMsg::Started(r) => {
                self.starting = false;
                match r {
                    Ok(p) => {
                        self.error = None;
                        self.proxy = Some(p);
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            CaptureMsg::Trusted(t) => *self.trusted.lock() = Some(t),
        }
    }

    pub fn is_running(&self) -> bool {
        self.proxy.is_some()
    }

    /// The proxy's address while it runs.
    pub fn address(&self) -> Option<std::net::SocketAddr> {
        self.proxy.as_ref().map(|p| p.addr())
    }

    /// Running but not recording (traffic still flows).
    pub fn is_paused(&self) -> bool {
        self.proxy.as_ref().is_some_and(|p| !p.is_recording())
    }

    pub fn stop(&mut self, rt: &tokio::runtime::Runtime) {
        if let Some(p) = self.proxy.take() {
            rt.spawn(p.stop());
        }
    }

    fn filter(&self) -> CaptureFilter {
        CaptureFilter {
            include_hosts: list(&self.include_hosts),
            exclude_hosts: list(&self.exclude_hosts),
            url_contains: self.url_contains.trim().to_string(),
            ..Default::default()
        }
    }

    fn ensure_ca(&mut self) -> Result<Arc<CertificateAuthority>, String> {
        if let Some(ca) = &self.ca {
            return Ok(ca.clone());
        }
        let (ca, _created) = CertificateAuthority::load_or_generate(&KeychainCa::open()).map_err(|e| e.to_string())?;
        let ca = Arc::new(ca);
        self.ca = Some(ca.clone());
        Ok(ca)
    }

    /// `Port 8888 · Decrypt HTTPS · 2 filters`.
    fn summary(&self) -> String {
        let filters = [&self.include_hosts, &self.exclude_hosts, &self.url_contains]
            .iter()
            .filter(|f| !f.trim().is_empty())
            .count();
        let port = self.address().map(|a| a.port()).unwrap_or(self.port);
        format!(
            "{} {} · {} · {}",
            tr("Port"),
            port,
            if self.intercept { tr("Decrypt HTTPS") } else { tr("HTTPS passed through") },
            if filters == 0 { tr("no filters").to_string() } else { trf("{} filters", &[&filters]) }
        )
    }
}

fn check_trust(app: &TernaryWireApp, tab_id: u64, ca: Arc<CertificateAuthority>) {
    let tx = app.tx.clone();
    let ctx = app.ctx.clone();
    app.rt.spawn_blocking(move || {
        let _ = tx.send(Msg::Capture { tab: tab_id, event: CaptureMsg::Trusted(ca.is_trusted_by_system()) });
        ctx.request_repaint();
    });
}

/// What the panes asked for this frame.
#[derive(Default)]
struct Acts {
    close: bool,
    copy: Option<String>,
}

pub fn show(app: &mut TernaryWireApp, ui: &mut Ui, tab_id: u64, tab: &mut CaptureTab) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let id = ui.make_persistent_id("capture-view");
    let mut do_start = std::mem::take(&mut tab.auto_start);
    let mut do_stop = false;
    match tab.auto.take().as_deref() {
        Some("start") => do_start = true,
        Some("stop") => do_stop = true,
        Some("settings") => {
            let i = tab.proxy.is_some() as usize;
            tab.options_open[i] = !tab.options_open[i];
        }
        Some("cert") => tab.show_ca = !tab.show_ca,
        Some(a) if a.starts_with("tick:") => {
            let entries = tab.proxy.as_ref().map(|p| p.entries()).unwrap_or_default();
            for n in a[5..].split(',').filter_map(|n| n.trim().parse::<usize>().ok()) {
                if let Some(e) = entries.get(n) {
                    tab.checked.insert(e.id);
                }
            }
        }
        Some(a) if a.starts_with("select") => {
            let entries = tab.proxy.as_ref().map(|p| p.entries()).unwrap_or_default();
            match a.strip_prefix("select:").and_then(|n| n.parse::<usize>().ok()) {
                Some(n) => tab.selected = entries.get(n).map(|e| e.id),
                None => tab.select_latest(),
            }
        }
        _ => {}
    }
    if do_start && tab.proxy.is_none() && !tab.starting {
        start(app, tab_id, tab);
    }
    let (state, label) = match tab.proxy.is_some() {
        true if tab.is_paused() => (State::Idle, tr("Paused")),
        true => (State::Live, tr("Recording")),
        false if tab.starting => (State::Busy, tr("Starting…")),
        false if tab.error.is_some() => (State::Failed, tr("Not started")),
        false => (State::Idle, tr("Stopped")),
    };
    let url = tab.proxy.as_ref().map(|p| p.url());
    let addr = url.as_deref().map(|u| u.trim_start_matches("http://").to_string());
    let entries: Vec<Arc<CaptureEntry>> = tab.proxy.as_ref().map(|p| p.entries()).unwrap_or_default();
    if tab.proxy.is_some() {
        ui.ctx().request_repaint_after(Duration::from_millis(400));
    }

    // Title and address bar.
    let mut copy_addr = false;
    let mut export = false;
    let mut clear = false;
    sv::tool_title(
        ui,
        icons::BROADCAST,
        tr("Capture traffic"),
        tr(
            "Set this proxy in a browser, app or device to record its requests. HTTPS needs the TernaryWire certificate to be trusted.",
        ),
        |ui| {
            if tab.proxy.is_some() {
                copy_addr = kit::menu_item(ui, Some(icons::COPY), tr("Copy address"), None).clicked();
                ui.add_enabled_ui(!entries.is_empty(), |ui| {
                    export = kit::menu_item(ui, Some(icons::EXPORT), tr("Export HAR"), None).clicked();
                    clear = kit::menu_item(ui, Some(icons::TRASH), tr("Clear"), None).clicked();
                });
                do_stop |= kit::menu_item(ui, Some(icons::STOP), tr("Stop"), None).clicked();
            } else {
                do_start |= ui
                    .add_enabled_ui(!tab.starting, |ui| kit::menu_item(ui, Some(icons::PLAY), tr("Start proxy"), None))
                    .inner
                    .clicked();
            }
        },
    );
    let placeholder = trf("Starts on port {}", &[&tab.port]);
    let mut toggle_ca = false;
    let copied = sv::server_bar(
        ui,
        id.with("bar"),
        sv::Bar { state, label, addr: addr.as_deref(), placeholder: &placeholder, max_w: 400.0 },
        |ui| {
            let Some(proxy) = &tab.proxy else { return };
            let rec = proxy.is_recording();
            let (icon, text) = if rec { (icons::PAUSE, tr("Pause")) } else { (icons::PLAY, tr("Resume")) };
            if kit::button_ex(ui, kit::Kind::Secondary, Some(icon), text, true)
                .on_hover_text(if rec { tr("Stop recording; traffic still flows") } else { tr("Record traffic again") })
                .clicked()
            {
                proxy.set_recording(!rec);
            }
            if proxy.is_intercepting() {
                let kind = if tab.show_ca { kit::Kind::Ghost } else { kit::Kind::Secondary };
                let o = kit::ButtonOpts { icon: Some(icons::SHIELD_CHECK), on: tab.show_ca, ..Default::default() };
                toggle_ca = kit::button(ui, kind, kit::Size::Sm, tr("Certificate"), o)
                    .on_hover_text(tr("Show the HTTPS certificate"))
                    .clicked();
            }
            let rejected = proxy.tls_failed_hosts();
            if !rejected.is_empty() {
                let o = kit::PillOpts { icon: Some(icons::WARNING), ..Default::default() };
                kit::pill_ex(ui, tr("Certificate rejected"), Tone::Warn, o).on_hover_text(format!(
                    "{}: {}",
                    tr("Certificate rejected by"),
                    rejected.join(", ")
                ));
            }
        },
        |ui| {
            if tab.proxy.is_some() {
                do_stop |= kit::secondary(ui, Some(icons::STOP), tr("Stop")).clicked();
            } else {
                do_start |= ui
                    .add_enabled_ui(!tab.starting, |ui| kit::primary_icon(ui, icons::PLAY, tr("Start proxy")))
                    .inner
                    .clicked();
            }
        },
    );
    if toggle_ca {
        tab.show_ca = !tab.show_ca;
    }
    if copied || copy_addr {
        if let Some(u) = &url {
            ui.ctx().copy_text(u.clone());
        }
        app.toast(tr("Copied"), false);
    }
    if let Some(e) = tab.error.clone() {
        let mut dismiss = false;
        sv::error_row(ui, &format!("{}: {e}", tr("Could not start the proxy")), |ui| {
            dismiss = kit::icon_button_sm(ui, icons::X, tr("Dismiss")).clicked();
        });
        if dismiss {
            tab.error = None;
        }
    }
    if tab.show_ca && tab.proxy.is_some() {
        ca_panel(app, ui, tab_id, tab);
    }

    // Options.
    let running = tab.proxy.is_some();
    let summary = vec![(tab.summary(), false)];
    let mut open = tab.options_open[running as usize];
    let tip = if open { tr("Hide options") } else { tr("Show options") };
    sv::strip(ui, id.with("strip"), &mut open, tr("Options"), &summary, None, tip);
    tab.options_open[running as usize] = open;
    if open {
        sv::settings_body(ui, |ui| options(ui, id, tab));
    }

    // Traffic | detail.
    let dialog_open = app.dialog.is_some();
    let rejected: Vec<String> = tab.proxy.as_ref().map(|p| p.tls_failed_hosts()).unwrap_or_default();
    let mut acts = Acts::default();
    let mut save: Option<Vec<Arc<CaptureEntry>>> = None;
    let collections: Vec<(Id, String)> =
        app.ws.data.collections.iter().map(|c| (c.id.clone(), c.name.clone())).collect();
    // Save to the first collection unless another one is chosen.
    if !tab.target.as_ref().is_some_and(|t| collections.iter().any(|(c, _)| c == t)) {
        tab.target = collections.first().map(|(c, _)| c.clone());
    }
    let cell = RefCell::new(&mut *tab);
    sv::panes(
        ui,
        id.with("panes"),
        0.5,
        |ui| {
            let mut t = cell.borrow_mut();
            traffic(ui, &mut t, &entries, &collections, &mut export, &mut clear, &mut save);
        },
        |ui| {
            let t = cell.borrow();
            match t.selected.and_then(|sel| entries.iter().find(|e| e.id == sel)) {
                Some(e) => {
                    detail(ui, e, rejected.contains(&e.host), &mut acts);
                    acts.close |= !dialog_open && sv::esc_closes(ui);
                }
                None => sv::select_hint(ui, tr("Its headers, bodies and timings appear here.")),
            }
        },
    );
    if acts.close {
        tab.selected = None;
    }
    if let Some(text) = acts.copy {
        ui.ctx().copy_text(text);
        app.toast(tr("Copied"), false);
    }
    if export && let Some(path) = rfd::FileDialog::new().set_file_name("capture.har").save_file() {
        let har = ternarywire_proxy::to_har(entries.iter().map(|e| e.as_ref()));
        match serde_json::to_vec_pretty(&har)
            .map_err(|e| e.to_string())
            .and_then(|b| std::fs::write(&path, b).map_err(|e| e.to_string()))
        {
            Ok(()) => app.toast(tr("HAR exported"), false),
            Err(e) => app.toast(e, true),
        }
    }
    if clear && let Some(proxy) = &tab.proxy {
        proxy.clear();
        tab.checked.clear();
        tab.selected = None;
    }
    if let Some(chosen) = save
        && let Some(cid) = tab.target.clone()
    {
        let items = ternarywire_proxy::to_request_items(chosen.iter().map(|e| e.as_ref()), &ImportOptions::default());
        let count = items.len();
        if let Some(c) = app.ws.data.collection_mut(&cid) {
            c.items.extend(items.into_iter().map(Into::into));
            app.ws.mark_collection(&cid);
            tab.checked.clear();
            app.toast(format!("{count} {}", tr("requests saved")), false);
        }
    }
    if do_stop {
        tab.stop(&app.rt);
    } else if do_start && tab.proxy.is_none() && !tab.starting {
        start(app, tab_id, tab);
    }
}

/// Proxy (fixed while it runs), Record filters and Never decrypt.
fn options(ui: &mut Ui, id: egui::Id, tab: &mut CaptureTab) {
    let running = tab.proxy.is_some();
    let mut apply = false;
    let mut passthrough_changed = false;
    let CaptureTab {
        port,
        intercept,
        verify_upstream,
        allow_lan,
        include_hosts,
        exclude_hosts,
        url_contains,
        passthrough,
        ..
    } = tab;
    let lock_hint = running.then(|| tr("Stop the proxy to change its port and HTTPS options."));
    sv::settings_grid(
        ui,
        vec![
            Box::new(|ui: &mut Ui| {
                sv::group(ui, tr("Proxy"), running, lock_hint, |ui| {
                    sv::field(ui, tr("Port"), |ui| {
                        let mut v = *port as u64;
                        sv::num_input(ui, id.with("port"), &mut v, 1..=65535, 70.0, tr("Port"));
                        *port = v as u16;
                    });
                    sv::switch_line(ui, intercept, tr("Decrypt HTTPS"));
                    sv::switch_line(ui, verify_upstream, tr("Verify server certificates"));
                    sv::switch_line(ui, allow_lan, tr("Allow other devices on the network"))
                        .on_hover_text(tr("Listens on all interfaces; anyone on the network can use the proxy"));
                });
            }),
            Box::new(|ui: &mut Ui| {
                sv::group(ui, tr("Record"), false, None, |ui| {
                    for (label, text, hint, key) in [
                        (tr("Only hosts"), include_hosts, "*.example.com", "include"),
                        (tr("Skip hosts"), exclude_hosts, "*.apple.com", "exclude"),
                        (tr("URL contains"), url_contains, "/api/", "contains"),
                    ] {
                        sv::field(ui, label, |ui| {
                            let w = ui.available_width();
                            let r = sv::input(ui, id.with(key), text, hint, w, true);
                            widgets::a11y_name(&r, label);
                        });
                    }
                    if running {
                        sv::hrow(ui, 24.0, |ui| {
                            apply = kit::ghost(ui, Some(icons::CHECK), tr("Apply filters")).clicked();
                        });
                    }
                });
            }),
            Box::new(|ui: &mut Ui| {
                sv::group(ui, tr("Never decrypt"), false, None, |ui| {
                    let w = ui.available_width();
                    let r =
                        sv::input(ui, id.with("passthrough"), passthrough, "bank.example.com, *.icloud.com", w, true);
                    widgets::a11y_name(&r, tr("Never decrypt"));
                    // While running, the list applies to new connections.
                    passthrough_changed = running && r.lost_focus();
                    ui.label(kit::hint(ui, &format!("{}.", tr("Hosts whose HTTPS is passed through untouched"))));
                });
            }),
        ],
    );
    if let Some(proxy) = &tab.proxy {
        if apply {
            proxy.set_filter(tab.filter());
        }
        if passthrough_changed {
            proxy.set_passthrough_hosts(list(&tab.passthrough));
        }
    }
}

fn start(app: &mut TernaryWireApp, tab_id: u64, tab: &mut CaptureTab) {
    let ca = if tab.intercept {
        match tab.ensure_ca() {
            Ok(ca) => Some(ca),
            Err(e) => {
                tab.error = Some(format!("{}: {e}", tr("Certificate")));
                return;
            }
        }
    } else {
        None
    };
    let host = if tab.allow_lan { std::net::Ipv4Addr::UNSPECIFIED } else { std::net::Ipv4Addr::LOCALHOST };
    let config = ProxyConfig {
        bind: std::net::SocketAddr::from((host, tab.port)),
        allow_lan: tab.allow_lan,
        intercept_tls: ca.is_some(),
        ca: ca.clone(),
        verify_upstream_tls: tab.verify_upstream,
        passthrough_hosts: list(&tab.passthrough),
        filter: tab.filter(),
        ..Default::default()
    };
    tab.starting = true;
    let tx = app.tx.clone();
    let ctx = app.ctx.clone();
    app.rt.spawn(async move {
        let r = CaptureProxy::start(config).await.map_err(|e| e.to_string());
        let _ = tx.send(Msg::Capture { tab: tab_id, event: CaptureMsg::Started(r) });
        ctx.request_repaint();
    });
    if let Some(ca) = ca {
        check_trust(app, tab_id, ca);
    }
}

/// The certificate card under the address bar: trust state, fingerprint,
/// expiry, Save and Check again.
fn ca_panel(app: &mut TernaryWireApp, ui: &mut Ui, tab_id: u64, tab: &mut CaptureTab) {
    let p = theme::pal(ui);
    let Ok(ca) = tab.ensure_ca() else { return };
    let trusted = *tab.trusted.lock();
    let mut close = false;
    let mut save = false;
    let mut check = false;
    let r = egui::Frame::new()
        .fill(p.well)
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            kit::card(ui).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    let (fg, bg) = match trusted {
                        Some(true) => (p.ok, p.ok_soft),
                        Some(false) => (p.warn, p.warn_soft),
                        None => (p.info, p.info_soft),
                    };
                    let (tile, _) = ui.allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::hover());
                    ui.painter().rect_filled(tile, egui::CornerRadius::same(7), bg);
                    let font = theme::icon_font(18.0);
                    ui.painter().text(tile.center(), egui::Align2::CENTER_CENTER, icons::SHIELD_CHECK, font, fg);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 10.0;
                        close = cert_title(ui, trusted);
                        cert_facts(ui, &ca);
                        (save, check) = cert_actions(ui);
                    });
                });
            });
        })
        .response
        .rect;
    // The panel's bottom border, under its padding.
    ui.painter().hline(r.x_range(), r.bottom() + 0.5, egui::Stroke::new(1.0, p.border));
    ui.add_space(1.0);
    if close {
        tab.show_ca = false;
    }
    if save && let Some(path) = rfd::FileDialog::new().set_file_name("ternarywire-capture-ca.pem").save_file() {
        match std::fs::write(&path, ca.cert_pem()) {
            Ok(()) => {
                ui.ctx().copy_text(ca.trust_instructions(&path.display().to_string()));
                app.toast(tr("Certificate saved; trust instructions copied to the clipboard"), false);
            }
            Err(e) => app.toast(e.to_string(), true),
        }
    }
    if check {
        *tab.trusted.lock() = None;
        check_trust(app, tab_id, ca);
    }
}

/// Title, trust state and Close of the certificate card. Returns true on Close.
fn cert_title(ui: &mut Ui, trusted: Option<bool>) -> bool {
    let p = theme::pal(ui);
    sv::hrow(ui, 22.0, |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let title = RichText::new(tr("TernaryWire capture certificate")).font(theme::semibold(13.5)).color(p.text);
        ui.label(title);
        let pill = |icon| kit::PillOpts { icon: Some(icon), ..Default::default() };
        match trusted {
            Some(true) => {
                kit::pill_ex(ui, tr("Trusted by this Mac"), Tone::Ok, pill(icons::CHECK));
            }
            Some(false) => {
                kit::pill_ex(ui, tr("Not trusted yet"), Tone::Warn, pill(icons::WARNING))
                    .on_hover_text(tr("Not trusted yet: HTTPS clients will reject the proxy"));
            }
            None => {
                kit::pill_ex(ui, tr("Checking trust…"), Tone::Info, pill(icons::CIRCLE_NOTCH));
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| kit::icon_button_sm(ui, icons::X, tr("Close")))
            .inner
            .clicked()
    })
}

/// SHA-256 (with Copy) and Valid until, in a two-column grid 6 apart.
fn cert_facts(ui: &mut Ui, ca: &CertificateAuthority) {
    let p = theme::pal(ui);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        let fp = ca.fingerprint_sha256();
        sv::hrow(ui, 22.0, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            cert_label(ui, "SHA-256", 22.0);
            ui.spacing_mut().item_spacing.x = 4.0;
            let g = kit::elide(ui, &fp, theme::mono(11.5), p.text, ui.available_width() - 26.0);
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(g.size().x, 22.0), egui::Sense::hover());
            ui.painter().galley(egui::pos2(rect.left(), rect.center().y - g.size().y / 2.0), g, p.text);
            resp.on_hover_text(&fp);
            if kit::icon_button_sm(ui, icons::COPY, tr("Copy fingerprint")).clicked() {
                ui.ctx().copy_text(fp.clone());
            }
        });
        // 12.5 px text on the body's 1.45 line height.
        sv::hrow(ui, 12.5 * 1.45, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            cert_label(ui, tr("Valid until"), 12.5 * 1.45);
            let date = ca.not_after().format("%-d %b %Y").to_string();
            ui.label(RichText::new(date).size(12.5).color(p.text));
        });
    });
}

/// Save certificate…, Check again and the hints. Returns (save, check again).
fn cert_actions(ui: &mut Ui) -> (bool, bool) {
    let p = theme::pal(ui);
    let hint = tr("Saving also copies the steps to trust it to the clipboard.");
    let hint_w = ui.painter().layout_no_wrap(hint.into(), egui::FontId::proportional(12.0), p.text_muted).size().x;
    let mut wrap = false;
    let clicked = sv::hrow(ui, 24.0, |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let label = format!("{}…", tr("Save certificate"));
        let save = kit::button_ex(ui, kit::Kind::Secondary, Some(icons::DOWNLOAD_SIMPLE), &label, true).clicked();
        let check = kit::ghost(ui, Some(icons::ARROWS_CLOCKWISE), tr("Check again")).clicked();
        // Like the prototype's wrapping row: the hint moves under the buttons.
        wrap = hint_w > ui.available_width();
        if !wrap {
            ui.label(kit::hint(ui, hint));
        }
        (save, check)
    });
    if wrap {
        ui.add_space(-2.0);
        sv::hrow(ui, 12.0 * 1.45, |ui| {
            ui.add(egui::Label::new(kit::hint(ui, hint)).truncate());
        });
    }
    let text = tr("Devices on the network can also download it from http://ternarywire.proxy/ through the proxy.");
    sv::hrow(ui, 12.0 * 1.45, |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(RichText::new(icons::INFO).font(theme::icon_font(12.0)).color(p.text_faint));
        ui.add(egui::Label::new(code_in_text(text, "http://ternarywire.proxy/", &p)).wrap());
    });
    clicked
}

/// A muted label in the 96 px first column of the certificate card (and
/// the 12 px gap after it), `height` high.
fn cert_label(ui: &mut Ui, text: &str, height: f32) {
    let p = theme::pal(ui);
    ui.allocate_ui_with_layout(egui::vec2(108.0, height), Layout::left_to_right(Align::Center), |ui| {
        ui.set_width(108.0);
        ui.label(RichText::new(text).size(12.5).color(p.text_muted));
    });
}

/// `text` (hint style) with `code` in monospace.
fn code_in_text(text: &str, code: &str, p: &theme::Palette) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let plain = egui::TextFormat::simple(egui::FontId::proportional(12.0), p.text_muted);
    let mono = egui::TextFormat::simple(theme::mono(12.0), p.text_muted);
    match text.find(code) {
        Some(i) => {
            job.append(&text[..i], 0.0, plain.clone());
            job.append(code, 0.0, mono);
            job.append(&text[i + code.len()..], 0.0, plain);
        }
        None => job.append(text, 0.0, plain),
    }
    job
}

/// The traffic list: title with Export HAR and Clear, search, target
/// collection, Save n to collection, then the requests with checkboxes.
fn traffic(
    ui: &mut Ui,
    tab: &mut CaptureTab,
    entries: &[Arc<CaptureEntry>],
    collections: &[(Id, String)],
    export: &mut bool,
    clear: &mut bool,
    save: &mut Option<Vec<Arc<CaptureEntry>>>,
) {
    let running = tab.proxy.is_some();
    let count = trf("{} requests", &[&entries.len()]);
    sv::pane_title(ui, tr("Traffic"), running.then_some(count.as_str()), |ui| {
        if running {
            ui.add_enabled_ui(!entries.is_empty(), |ui| {
                *clear |= kit::ghost(ui, Some(icons::TRASH), tr("Clear")).clicked();
                *export |= kit::ghost(ui, Some(icons::EXPORT), tr("Export HAR")).clicked();
            });
        }
    });
    let n = tab.checked.len();
    sv::row_ex(
        ui,
        kit::size::TOOLBAR,
        (12.0, 12.0),
        8.0,
        false,
        |ui| {
            let w = ui.available_width().max(110.0);
            let r = kit::search_field_sm(ui, "capture-search", &mut tab.search, tr("Search URLs"), w);
            widgets::a11y_name(&r, tr("Search captured requests"));
        },
        |ui| {
            if !running {
                return;
            }
            let can = n > 0 && tab.target.is_some();
            let label = if n == 0 { tr("Save to collection").to_string() } else { trf("Save {} to collection", &[&n]) };
            let hint = if n == 0 { tr("Tick requests in the list first") } else { tr("Choose a collection first") };
            let o = kit::ButtonOpts { icon: Some(icons::FOLDER_PLUS), ..Default::default() };
            let r = ui.add_enabled_ui(can, |ui| kit::button(ui, kit::Kind::Primary, kit::Size::Sm, &label, o)).inner;
            if r.on_hover_text(tr("Save the ticked requests")).on_disabled_hover_text(hint).clicked() {
                *save = Some(entries.iter().filter(|e| tab.checked.contains(&e.id)).cloned().collect());
            }
            let current = tab
                .target
                .as_ref()
                .and_then(|id| collections.iter().find(|(c, _)| c == id))
                .map(|(_, n)| n.clone())
                .unwrap_or_else(|| tr("choose collection").into());
            let text_w =
                ui.painter().layout_no_wrap(current.clone(), egui::FontId::proportional(12.0), p_text(ui)).size().x;
            let r = kit::select_box_ex(ui, Some(icons::STACK), None, &current, (text_w + 52.0).min(150.0), true, false);
            widgets::a11y_name(&r, tr("Save to collection"));
            egui::Popup::menu(&r).width(200.0).show(|ui| {
                for (cid, name) in collections {
                    let on = tab.target.as_ref() == Some(cid);
                    let item = kit::MenuItem { checked: Some(on), label: name, ..Default::default() };
                    if kit::menu_item_ex(ui, item).clicked() {
                        tab.target = Some(cid.clone());
                    }
                }
            });
        },
    );
    let q = tab.search.to_lowercase();
    let shown: Vec<&Arc<CaptureEntry>> = entries
        .iter()
        .filter(|e| q.is_empty() || e.url.to_lowercase().contains(&q) || e.method.to_lowercase() == q)
        .collect();
    let tickable: Vec<u64> = shown.iter().filter(|e| e.kind != EntryKind::Tunnel).map(|e| e.id).collect();
    let ticked = tickable.iter().filter(|i| tab.checked.contains(i)).count();
    let all = !tickable.is_empty() && ticked == tickable.len();
    let head = [tr("Select all"), tr("Time"), tr("Method"), tr("Status"), "URL", tr("Size")];
    if sv::list_head(ui, &COLS, &head, &[5], Some((all, ticked > 0 && !all))) {
        if all {
            for i in &tickable {
                tab.checked.remove(i);
            }
        } else {
            tab.checked.extend(tickable.iter().copied());
        }
    }
    if entries.is_empty() {
        match &tab.proxy {
            Some(proxy) => kit::empty_state(
                ui,
                icons::HOURGLASS,
                tr("Waiting for traffic"),
                &trf("Set {} as the HTTP and HTTPS proxy of a browser, app or device.", &[&proxy.url()]),
                |_| {},
            ),
            None => kit::empty_state(
                ui,
                icons::BROADCAST,
                tr("The proxy is stopped"),
                tr("Start it, then point a browser, app or device at it to record their requests."),
                |_| {},
            ),
        }
        return;
    }
    if shown.is_empty() {
        kit::empty_state(ui, icons::FUNNEL, tr("No requests match"), "", |_| {});
        return;
    }
    let (tunnel, failed) = (tr("tunnel"), tr("failed"));
    sv::list_scroll(ui, "capture-list", shown.len(), true, |ui, range| {
        for e in &shown[range] {
            let time = e.started_at.with_timezone(&chrono::Local).format("%H:%M:%S").to_string();
            let tunnelled = e.kind == EntryKind::Tunnel;
            let status = match (e.status, e.state) {
                _ if tunnelled => Stat::Faint(tunnel),
                (Some(s), _) => Stat::Code(s),
                (None, EntryState::Failed) => Stat::Error(failed),
                (None, _) => Stat::Pending,
            };
            let (path, query) = if tunnelled {
                (format!(":{}", e.port), String::new())
            } else {
                let p = e.path();
                match p.split_once('?') {
                    Some((a, b)) => (a.to_string(), b.to_string()),
                    None => (p.to_string(), String::new()),
                }
            };
            let size = size_text(e);
            let method = if e.method.is_empty() || tunnelled { "CONNECT" } else { e.method.as_str() };
            let cells = [
                if tunnelled { Cell::Empty } else { Cell::Check },
                Cell::Ts(&time),
                Cell::Method(method),
                Cell::Status(status),
                Cell::Path { host: &e.host, path: &path, query: &query, pill: None },
                Cell::Ts(&size),
            ];
            let mut on = tab.checked.contains(&e.id);
            let was = on;
            let name = format!("{method} {}", e.url);
            let check = (!tunnelled).then_some(&mut on);
            let r = sv::list_row(ui, &COLS, &cells, &[5], tab.selected == Some(e.id), &name, check);
            if r.on_hover_text(format!(
                "{}{}",
                e.host,
                if tunnelled { format!(":{}", e.port) } else { e.path().to_string() }
            ))
            .clicked()
            {
                tab.selected = Some(e.id);
            }
            if on != was {
                if on {
                    tab.checked.insert(e.id);
                } else {
                    tab.checked.remove(&e.id);
                }
            }
        }
    });
}

fn p_text(ui: &Ui) -> egui::Color32 {
    theme::pal(ui).text
}

/// The size column: bytes of the response, of a tunnel, or WebSocket messages.
fn size_text(e: &CaptureEntry) -> String {
    let h = ternarywire_core::util::human_bytes;
    match (e.kind, e.state) {
        (EntryKind::Tunnel, _) => e.stream.as_ref().map(|s| h(s.bytes_up + s.bytes_down)).unwrap_or_else(|| "—".into()),
        (EntryKind::WebSocket, _) => trf("{} msgs", &[&(e.ws_messages.len() as u64 + e.ws_messages_dropped)]),
        (_, EntryState::Failed | EntryState::Pending) if e.status.is_none() => "—".into(),
        _ => h(e.response_body.size),
    }
}

fn ms(d: Duration) -> String {
    ternarywire_core::util::human_ms(d.as_secs_f64() * 1000.0)
}

/// The selected request: head, metrics, headers, bodies or messages.
fn detail(ui: &mut Ui, e: &CaptureEntry, rejected: bool, acts: &mut Acts) {
    let p = theme::pal(ui);
    let tunnelled = e.kind == EntryKind::Tunnel;
    let method = if e.method.is_empty() || tunnelled { "CONNECT" } else { e.method.as_str() };
    let pending = e.status.is_none() && !tunnelled && e.state != EntryState::Failed;
    sv::detail_head(
        ui,
        |ui| {
            sv::method_tag(ui, method);
            let (a, b) = e.url.split_once('?').unwrap_or((&e.url, ""));
            sv::path_label(ui, "", a, b);
        },
        |ui| {
            acts.close |= kit::icon_button_sm(ui, icons::X, tr("Close (Esc)")).clicked();
            let lg = kit::PillOpts { large: true, ..Default::default() };
            if tunnelled {
                kit::pill_ex(ui, tr("tunnel"), Tone::Neutral, lg);
            } else if let Some(s) = e.status {
                kit::status_pill(ui, s, e.status_text());
            } else if e.state == EntryState::Failed {
                kit::pill_ex(ui, tr("failed"), Tone::Err, lg);
            } else {
                sv::server_pill(ui, State::Busy, tr("Waiting…"));
            }
        },
    );
    let version = |ui: &mut Ui| {
        if !e.http_version.is_empty() {
            kit::pill_ex(ui, &e.http_version, Tone::Neutral, kit::PillOpts { outline: true, ..Default::default() });
        }
    };
    let total = e.timings.total.map(ms);
    let ws = e.kind == EntryKind::WebSocket;
    sv::detail_meta(
        ui,
        |ui| {
            if tunnelled {
                let o = kit::PillOpts { outline: true, icon: Some(icons::LOCK_SIMPLE), ..Default::default() };
                kit::pill_ex(ui, tr("passed through"), Tone::Neutral, o);
                return;
            }
            version(ui);
            if e.tls_intercepted && e.state != EntryState::Failed {
                let o = kit::PillOpts { icon: Some(icons::LOCK_SIMPLE), ..Default::default() };
                kit::pill_ex(ui, tr("decrypted"), Tone::Ok, o);
            }
        },
        |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let h = ternarywire_core::util::human_bytes;
            if tunnelled {
                if let Some(t) = &total {
                    sv::metric(ui, Some(icons::TIMER), t, None);
                }
                if let Some(s) = &e.stream {
                    sv::metric(ui, Some(icons::ARROW_DOWN), &h(s.bytes_down), Some(tr("Sent by the server")));
                    sv::metric(ui, Some(icons::ARROW_UP), &h(s.bytes_up), Some(tr("Sent by the client")));
                }
            } else if ws {
                let (up, down): (Vec<_>, Vec<_>) = e.ws_messages.iter().partition(|m| m.from_client);
                let sum = |v: &[&ternarywire_proxy::WsMessage]| h(v.iter().map(|m| m.size).sum());
                sv::metric(
                    ui,
                    Some(icons::ARROW_DOWN),
                    &format!("{} · {}", trf("{} msgs", &[&down.len()]), sum(&down)),
                    None,
                );
                sv::metric(
                    ui,
                    Some(icons::ARROW_UP),
                    &format!("{} · {}", trf("{} msgs", &[&up.len()]), sum(&up)),
                    None,
                );
            } else if pending {
                let since = (chrono::Utc::now() - e.started_at).to_std().unwrap_or_default();
                sv::metric(ui, Some(icons::TIMER), &ms(since), None);
            } else {
                if e.state != EntryState::Failed {
                    sv::metric(ui, Some(icons::ARROW_LINE_DOWN), &h(e.response_body.size), Some(tr("Response size")));
                    if let Some(t) = e.timings.ttfb {
                        sv::metric(ui, None, &format!("TTFB {}", ms(t)), Some(tr("Time to first byte")));
                    }
                }
                if let Some(t) = &total {
                    sv::metric(ui, Some(icons::TIMER), t, Some(tr("Total")));
                }
            }
        },
    );
    if pending {
        kit::empty_state(
            ui,
            icons::HOURGLASS,
            tr("No response yet"),
            tr("The server is still sending the response. It appears here when it is complete."),
            |_| {},
        );
        return;
    }
    sv::detail_scroll(ui, ("capture-detail", e.id), |ui| {
        if let Some(err) = &e.error {
            let text = format!("{} {err}", trf("Could not reach {}.", &[&e.host]));
            kit::banner(ui, Tone::Err, icons::WARNING_CIRCLE, &text);
        }
        if let Some(n) = &e.note {
            let tone = if rejected { Tone::Warn } else { Tone::Info };
            kit::banner(ui, tone, if rejected { icons::WARNING } else { icons::INFO }, n);
        }
        if tunnelled {
            let opened = e.started_at.with_timezone(&chrono::Local);
            let mut rows = vec![
                (tr("Host").to_string(), format!("{}:{}", e.host, e.port)),
                (tr("Client").to_string(), e.client_addr.to_string()),
                (tr("Opened").to_string(), opened.format("%H:%M:%S%.3f").to_string()),
            ];
            if let Some(t) = e.timings.total
                && e.is_finished()
            {
                let closed = opened + chrono::Duration::from_std(t).unwrap_or_default();
                rows.push((tr("Closed").to_string(), closed.format("%H:%M:%S%.3f").to_string()));
            }
            let rows: Vec<(&str, &str)> = rows.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            sv::section(
                ui,
                tr("Connection"),
                None,
                |_| {},
                |ui| sv::headers_table(ui, ui.make_persistent_id("conn"), &rows),
            );
            return;
        }
        let req_h = pairs(&e.request_headers);
        if e.state == EntryState::Failed && e.status.is_none() {
            sv::section(
                ui,
                tr("Request headers"),
                Some(req_h.len()),
                |_| {},
                |ui| sv::headers_table(ui, ui.make_persistent_id(("req-h", e.id)), &req_h),
            );
            return;
        }
        sv::disclosure(
            ui,
            ui.make_persistent_id("cap-req-h-open"),
            tr("Request headers"),
            Some(req_h.len()),
            true,
            |ui| {
                if req_h.is_empty() {
                    sv::no_body(ui, tr("No headers"));
                } else {
                    sv::headers_table(ui, ui.make_persistent_id(("req-h", e.id)), &req_h);
                }
            },
        );
        let req = e.decoded_request_body();
        sv::section(
            ui,
            tr("Request body"),
            None,
            |_| {},
            |ui| {
                if req.data.is_empty() {
                    sv::no_body(ui, tr("No body"));
                } else {
                    let text = req.text().map(|t| t.into_owned()).unwrap_or_else(|| tr("(binary)").into());
                    sv::body_box(
                        ui,
                        ui.make_persistent_id(("cap-req", e.id)),
                        &crate::proto_common::payload_text(text.as_bytes()),
                    );
                }
            },
        );
        let res_h = pairs(&e.response_headers);
        let open_id = ui.make_persistent_id(("cap-res-h-open", ws));
        sv::disclosure(ui, open_id, tr("Response headers"), Some(res_h.len()), !ws, |ui| {
            if res_h.is_empty() {
                sv::no_body(ui, tr("No response yet"));
            } else {
                sv::headers_table(ui, ui.make_persistent_id(("res-h", e.id)), &res_h);
            }
        });
        if ws {
            let n = e.ws_messages.len() + e.ws_messages_dropped as usize;
            sv::section(ui, tr("Messages"), Some(n), |_| {}, |ui| messages(ui, e));
            return;
        }
        let resp = e.decoded_response_body();
        let mut copy = false;
        sv::section(
            ui,
            tr("Response body"),
            None,
            |ui| {
                if !resp.data.is_empty() {
                    copy = kit::icon_button_sm(ui, icons::COPY, tr("Copy body")).clicked();
                }
                if resp.truncated {
                    kit::tone_pill(ui, tr("truncated"), Tone::Warn);
                }
            },
            |ui| {
                if resp.data.is_empty() {
                    let text = match e.status {
                        Some(s @ (204 | 304)) => format!("{} ({s} {})", tr("No body"), e.status_text()),
                        _ => tr("No body").to_string(),
                    };
                    sv::no_body(ui, &text);
                } else {
                    let text = resp
                        .text()
                        .map(|t| crate::dialogs::ellipsize(&t, 100_000))
                        .unwrap_or_else(|| tr("(binary)").into());
                    sv::body_box(
                        ui,
                        ui.make_persistent_id(("cap-resp", e.id)),
                        &crate::proto_common::payload_text(text.as_bytes()),
                    );
                }
            },
        );
        if copy && let Some(t) = resp.text() {
            acts.copy = Some(t.into_owned());
        }
    });
    let _ = p;
}

fn pairs(list: &[(String, String)]) -> Vec<(&str, &str)> {
    list.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect()
}

/// WebSocket messages in a bordered list: direction, time, payload, size.
fn messages(ui: &mut Ui, e: &CaptureEntry) {
    let p = theme::pal(ui);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(egui::Stroke::new(1.0, p.border))
        .corner_radius(egui::CornerRadius::same(8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            let start = e.started_at.with_timezone(&chrono::Local);
            let n = e.ws_messages.len();
            for (i, m) in e.ws_messages.iter().enumerate() {
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.0), egui::Sense::hover());
                if !ui.is_rect_visible(rect) {
                    continue;
                }
                let painter = ui.painter();
                if i + 1 < n {
                    painter.hline(rect.x_range(), rect.bottom() - 0.5, egui::Stroke::new(1.0, p.border));
                }
                let cy = rect.center().y;
                let mut x = rect.left() + 10.0;
                let (icon, color) = if m.from_client { (icons::ARROW_UP, p.info) } else { (icons::ARROW_DOWN, p.ok) };
                painter.text(egui::pos2(x + 6.0, cy), egui::Align2::CENTER_CENTER, icon, theme::icon_font(12.0), color);
                x += 22.0 + 8.0;
                let at = start + chrono::Duration::from_std(m.at).unwrap_or_default();
                painter.text(
                    egui::pos2(x, cy),
                    egui::Align2::LEFT_CENTER,
                    at.format("%H:%M:%S%.3f").to_string(),
                    theme::mono(11.5),
                    p.text_faint,
                );
                x += 92.0 + 8.0;
                let size = ternarywire_core::util::human_bytes(m.size);
                let right = rect.right() - 10.0;
                painter.text(egui::pos2(right, cy), egui::Align2::RIGHT_CENTER, &size, theme::mono(11.5), p.text_faint);
                let text = crate::dialogs::ellipsize(&m.text(), 400).replace('\n', " ");
                let g = kit::elide(ui, &text, theme::mono(12.0), p.text, right - 54.0 - 8.0 - x);
                ui.painter().galley(egui::pos2(x, cy - g.size().y / 2.0), g, p.text);
                resp.on_hover_text(crate::dialogs::ellipsize(&m.text(), 2000));
            }
        });
}
