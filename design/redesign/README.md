# TernaryWire — Redesign UI/UX

Prototipe desain lokal untuk merombak seluruh tampilan TernaryWire. Semua layar, dialog, dan state ada
di sini sebagai halaman HTML interaktif. Tidak ada yang dipublikasikan ke mana pun.

**Status: sudah diimplementasikan di app egui** (`crates/ternarywire-app`). Prototipe ini tetap menjadi
acuan visual; `fixture/compare.sh <screen-id> [light|dark] [WxH]` merender layar app dan layar desain
berdampingan dengan data yang sama (lihat `fixture/README.md`). Panduan komponen Rust ada di
`docs/design.md`.

## Cara membuka

Buka `design/redesign/index.html` langsung di Chrome, Safari, atau Firefox. Tidak perlu server atau
internet: font (Inter, JetBrains Mono) dan ikon (subset Phosphor) diambil dari aset app sendiri lewat
`fonts.css` dan `icons.js`.

| Kontrol | Fungsi |
| --- | --- |
| Dropdown layar, `‹` `›`, atau tombol `[` `]` | Pindah layar (semua dikelompokkan per area) |
| Light / Dark | Ganti tema |
| Ukuran | 1440×900, 1280×820 (default app), 1100×720, 900×640 (sempit) |
| Catatan desain | Alasan desain + petunjuk implementasi egui untuk layar yang sedang dibuka |
| `⌘K` `⌘E` `⌘,` `⌘B` `Esc` | Palette, ganti environment, Settings, sidebar, tutup |

Tab, segmented control, menu (`⋯`, `▾`), dan item sidebar bisa diklik. Tautan langsung ke satu
layar: `index.html#http-auth`, `index.html?theme=dark#flow`.

Kalau font atau ikon app berubah, jalankan `python3 design/redesign/build-assets.py`.
`index.html?audit=1` merender semua layar di kedua tema dan mendaftar error JS serta elemen yang keluar
dari jendela (dipakai untuk memeriksa prototipe secara headless).

## Daftar layar (64)

| Area | Layar |
| --- | --- |
| Start | `home` start page tanpa tab |
| HTTP request (11) | `http` Params + body JSON · `http-body` body raw + headers response · `http-auth` OAuth 2.0 + Tests · `http-scripts` Scripts + Details/timing · `http-graphql` GraphQL + schema explorer · `http-filter` filter JSONPath · `http-compare` Compare · `http-empty` belum dikirim · `http-loading` sedang mengirim · `http-error` gagal kirim · `http-stacked` jendela sempit |
| Realtime & RPC (8) | `ws` · `sio` (ack tertunda) · `mqtt` · `grpc` · `grpc-picker` · `mcp` · `mcp-elicit` · `ai` |
| Workspace (9) | `container` pengaturan collection · `env` variabel environment · `history` · `spec` · `spec-sync` · `flow` · `flow-settings` · `runner` · `runner-perf` |
| Local servers (6) | `mock` · `mock-routes` · `webhook` · `webhook-empty` · `capture` · `capture-cert` |
| Dialog & panel (29) | palette (2), env switcher, quick look, Settings (7 halaman), Vault (3), cookies, import (2), code snippet, save request, generate tests, set as variable, fork pull/merge, konfirmasi hapus, tutup tab belum disimpan, name prompt, new workspace, update (2), console |

| File | Isi |
| --- | --- |
| `styles.css` | Token warna (terang/gelap) dan semua komponen |
| `kit.js` | Komponen sebagai helper HTML; tiap helper punya padanan di `kit.rs`/`widgets.rs` |
| `app.js` | Runtime prototipe: state, registri layar, harness, event |
| `shell.js` | Title bar, rail, sidebar, tab strip, status bar, console, menu |
| `screens-http.js` | Request HTTP/GraphQL dan response, semua sub-tab dan state |
| `screens-proto.js` | WebSocket, Socket.IO, MQTT, gRPC, MCP, AI |
| `screens-work.js` | Start page, pengaturan collection, environment, history, Spec Hub |
| `screens-flow.js` | Flows dan Collection Runner (functional + performance) |
| `screens-tools.js` | Mock server, webhook listener, capture proxy |
| `dialogs.js` | Semua dialog dan panel console |
| `fonts.css`, `icons.js` | Dihasilkan `build-assets.py` dari `crates/ternarywire-app/assets/fonts` dan `src/icons.rs` |

## Prinsip

1. **Request di kiri, response di kanan.** Setiap panel berdiri sendiri. Kiri berisi method, URL, Send,
   lalu tab request. Kanan berisi baris status (kode, waktu, ukuran, ringkasan test), lalu tab response.
2. **Baris yang sejajar.** Baris URL dan baris status sama tinggi, tab strip kiri dan kanan sejajar,
   sehingga garis horizontal menyambung di seluruh lebar. Inilah yang membuat layar terasa rapi.
3. **Satu grammar untuk semua protokol.** WebSocket, Socket.IO, MQTT, gRPC, MCP, dan AI memakai pola
   yang sama: compose/koneksi di kiri, trafik/hasil di kanan. Mock, webhook, dan capture memakai satu
   pola "server": header server, daftar trafik di kiri, detail di kanan.
4. **Satu kartu kerja.** Tab strip dan view berada dalam satu kartu membulat di atas chrome. Rail dan
   sidebar tidak berebut perhatian dengan konten.
5. **Satu aksi primer per view** (Send, Connect, Run, Invoke, Start). Aksi lain ghost atau secondary.
6. **Status selalu terlihat.** Status response, koneksi, server, dan penyimpanan tidak pernah
   tersembunyi di balik tab.
7. **Detail bertahap.** Opsi lanjutan tertutup (`Advanced (n)`), detail hanya muncul saat dipilih.
8. **Setiap state didesain.** Kosong, memuat, gagal, dan tidak cocok filter punya ikon, judul, penjelasan,
   dan aksi berikutnya.

## Shell

```
┌ title bar 44 ─ ● ● ●  [▣ Bookstore ▾] │ + New ▾  ⤓ Import   [⌕ Search requests, run a command… ⌘K]   [● Local ▾] 👁 │ 🍪 🔒 ┐
├ rail 52 ┬ sidebar 224–276 ┬ kartu kerja ──────────────────────────────────────────────────────────────────────┤
│ Collections │ judul + aksi   │ tab strip 40                                                                  │
│ Environments│ filter         │ judul view 46                                                                 │
│ History     │ tree           │ ┌ panel kiri ─────────────────┬ panel kanan ───────────────────────────────┐ │
│ APIs        │                │ │ baris aksi 56               │ baris status 56                             │ │
│ Flows       │                │ │ tab 36                      │ tab 36                                      │ │
│ ─────       │                │ │ toolbar 40                  │ toolbar 40                                  │ │
│ Servers     │                │ │ konten                      │ konten                                      │ │
│  …          │                │ └─────────────────────────────┴─────────────────────────────────────────────┘ │
│ Console     │                │ (console drawer, opsional)                                                    │
│ Settings    │                │                                                                               │
├ status bar 28 ─ sidebar · Console (1) · ✓ Saved            Update · workspace · Side by side · ⌘ ──────────┘
```

| Bagian | Sekarang | Redesign |
| --- | --- | --- |
| Title bar | Logo, workspace, New, Import, search 420 px, env, cookies, vault | Sama isinya. Search jadi "Search requests, run a command…" (palette), env selector + quick look dikelompokkan, cookies/vault dipisah garis |
| Rail | 5 bagian + Console + Settings | 5 bagian + **Local servers** (mock, webhook, capture yang sedang jalan, dengan titik status) + Console (titik merah bila ada error) + Settings |
| Area kerja | Panel kanvas penuh | Kartu radius 10, border 1 px, margin 8 px dari chrome |
| Tab strip | 38 px, tab aktif bergaris aksen atas | 40 px di dalam kartu, tab aktif "terangkat" ke kanvas, titik untuk unsaved, menu "All open tabs" di ujung |
| Status bar | Sidebar, console, layout, palette, update, saved, workspace | Urutan baru: kiri = keadaan kerja (sidebar, console + jumlah error, saved), kanan = app (update, workspace, layout Side by side/Stacked, palette) |

## Ukuran dan grid

| Elemen | Tinggi | Catatan |
| --- | --- | --- |
| Title bar | 44 | `egui::Panel::top("top").exact_size(44)` |
| Tab strip | 40 | tab 32 + garis bawah |
| Baris judul view | 46 | breadcrumb + nama + aksi sekunder |
| Baris aksi / status panel | 56 | URL bar 38, tombol primer 38 |
| Tab strip dalam panel | 36 | garis bawah aksen 2 px |
| Toolbar konten | 40 | segmented 26, tombol kecil 24 |
| Baris tabel / kv | 32–34 | header 28–30 |
| Baris tree sidebar | 28 | indent 14 per level |
| Status bar | 28 | |

Jarak: 8 px antar kontrol sebaris, 12–14 px antar grup, 12 px padding panel. Radius: 6 kontrol kecil,
7–9 kontrol dan kartu, 10 kartu kerja, 14 dialog.

Side by side dipakai bila lebar area kerja ≥ 880 px (dua panel ±440 px); di bawah itu response turun ke
bawah dengan struktur panel yang sama. Pembagi bisa digeser, klik dua kali = reset 50/50. Toggle manual
ada di status bar.

## Token warna

Nama token di `styles.css` → field `theme::Palette`. Yang bertanda **baru** perlu ditambahkan.

| Token | Terang | Gelap | Palette |
| --- | --- | --- | --- |
| `--chrome` | `#f4f4f6` | `#121316` | `chrome` (sedikit lebih gelap dari sekarang agar kartu kerja terangkat) |
| `--canvas` | `#ffffff` | `#18191c` | `canvas` |
| `--surface` | `#ffffff` | `#202125` | `surface` |
| `--well` | `#f8f8fa` | `#1c1d21` | **baru**: latar editor, header tabel, tab strip, panel samping (sekarang `faint_bg_color`) |
| `--hover` / `--subtle` | `#efeff2` / `#e8e8ec` | `#26272c` / `#2d2e34` | `hover` / `subtle` |
| `--border` / `--border-strong` | `#e4e4e9` / `#d0d0d7` | `#2c2d33` / `#3e4047` | `border` / `border_strong` |
| `--text` / `--text-muted` / `--text-faint` | `#17171c` / `#62626d` / `#9a9aa5` | `#e8e8ec` / `#9e9fa8` / `#6e6f78` | `text` / `text_muted` / `text_faint` |
| `--accent` (+ hover, soft, text) | `#e14d2a` | `#f26748` | sama seperti sekarang |
| `--ok-soft`, `--warn-soft`, `--err-soft`, `--info-soft` | | | **baru**: latar pill dan banner (sekarang `theme::soft(c, α)` per tempat) |
| `--var-ok-bg`, `--var-bad-bg` | `#e3f4ea` / `#fce8e8` | `#1b3326` / `#3d1f1f` | **baru**: chip `{{variabel}}` (sekarang hanya warna teks) |
| `--syn-*` | | | sama dengan `highlight.rs` |
| `--m-*` | | | sama dengan `theme::method_color` |

## Komponen → Rust

| Prototipe (`kit.js`) | Rust | Perubahan |
| --- | --- | --- |
| `btn`, `primary`, `ghost`, `ibtn` | `kit::button_ex`, `primary`, `ghost`, `icon_button` | tinggi 30 / 24 (sm) / 38 (aksi panel) |
| `tabs` | `kit::tab_strip_ex` | tinggi 36, badge hitungan berwarna (ok/err) untuk hasil test |
| `seg` | `kit::segmented` | varian kecil 22 px untuk toolbar |
| `pill`, `statusPill` | `kit::pill`, `kit::status_pill` | memakai token `*-soft`, varian `live` (titik dengan halo) |
| `kv` | `widgets::kv_table` | kolom aksi kanan tetap, baris tambah selalu ada, ikon gembok untuk header otomatis |
| `code`, `jsonLines` | `body_view::doc_editor`, `widgets::code_editor` | gutter dengan latar `well`, chip lipatan "… 6 keys" |
| `empty` | `kit::empty_state` | ikon dalam kotak 52 px radius 14 |
| `banner`, `card`, `stat` | `request_tab::banner`, `kit::card`, `request_tab::stat_card` | dipindah ke `kit.rs` (dipakai >3 layar) |
| `ptitle` | `proto_common::view_title`, `request_tab::view_header` | disatukan |
| `modal` | `dialogs::Modal` + `footer` | radius 14, footer berlatar `well` |
| `menu` | `egui::Popup::menu` | header grup huruf kapital kecil, kbd di kanan |

## Perubahan per area

Semua fitur yang ada sekarang tetap ada. Yang dipindah atau digabung dijelaskan di sini dan di Catatan
desain layar masing-masing.

- **HTTP.** URL bar pindah ke panel kiri, baris status (kode, waktu, ukuran, "3/3 tests") ke panel
  kanan. Pemilih tipe auth jadi dropdown di toolbar, bukan kolom 250 px. Snippets jadi menu.
  "Authorization" disingkat "Auth" dan "Test Results" disingkat "Tests". Response kosong menampilkan
  request terakhir dari History. Error mendapat daftar periksa sesuai jenisnya.
- **Protokol.** Status koneksi pindah dari judul ke baris status panel kanan; pill protokol (transport,
  versi, server) tetap di judul. Kartu ack Socket.IO dan elicitation MCP ada di atas panel kanan. MQTT
  menampilkan langganan aktif sebagai chip di atas log; tab Subscriptions tetap ada. Traffic MCP dan Raw
  response AI jadi tab di panel kanan. "Saved messages" disingkat "Saved".
- **Workspace.** Overview collection mendapat kartu "Actions" (Export, Documentation, Fork), yang
  sekarang hanya ada di menu klik kanan. Kolom nilai variabel diberi label SHARED dan LOCAL. Tab dari
  History menampilkan status terakhir, karena entri history tidak menyimpan body response.
- **Flows dan Runner.** Zoom −/+ dan Fit jadi satu kapsul di kanan bawah kanvas. Menu Add block
  menampilkan deskripsi tiap blok sebagai baris kedua. Data file Runner jadi chip (nama, jumlah baris,
  Preview data, hapus). Latency percentiles dipasangkan dengan tabel Per request.
- **Server lokal.** Mock, webhook, dan capture memakai satu pola: judul 46 px, bar alamat 56 px (status,
  alamat + Copy, aksi utama), strip pengaturan yang bisa dilipat, daftar trafik di kiri, detail di
  kanan. Mock: Routes dan Logs jadi tab panel kiri. Webhook: "Respond with" masuk strip Settings, Replay
  jadi footer tetap di panel detail.
- **Dialog.** Anatomi seragam: header (ikon, judul, subjudul), isi, footer (petunjuk kiri, Cancel, aksi
  utama paling kanan). Di Vault, form Link menggantikan kartu "Add a secret". Providers jadi daftar di
  kiri dengan form edit di kanan. Daftar bahasa Code snippet mendapat filter.

### Usulan baru (belum ada di app)

| Usulan | Layar |
| --- | --- |
| Bagian rail **Local servers** dengan status server yang jalan | rail, `mock`, `webhook`, `capture` |
| Export hasil load test (sekarang belum ada export di mode performance) | `runner-perf` (ditandai pill "New") |
| Field "Filter requests" di webhook listener | `webhook` |
| Filter bahasa di Code snippet | `dlg-snippet` |
| Daftar request terakhir di response kosong | `http-empty` |
| Kolom "Duration" di log mock (string baru untuk `id.tsv`) | `mock` |

## Rencana implementasi

1. **Token dan kit.** Tambah token baru di `theme.rs`, pindahkan `banner`, `stat_card`, `view_header`,
   `disclosure`, dan `read_table` ke `kit.rs`, sesuaikan tinggi komponen.
2. **Shell.** `app.rs`: `top_bar`, `rail` (+ Local servers), `tab_bar`, `status_bar`, dan kartu kerja di
   `CentralPanel`. Update `docs/design.md`.
3. **Request HTTP.** `request_tab::show`: baris URL pindah ke kolom kiri, `response_panel` mendapat baris
   status 56 px; ambang side-by-side jadi lebar area kerja ≥ 880.
4. **Protokol.** `proto_common::split_view` mendapat mode horizontal (kiri/kanan) sebagai default;
   MCP dan AI dipindah ke pola yang sama.
5. **Workspace, Flows, Runner, Spec Hub, server lokal.**
6. **Dialog.**
7. **Teks dan dokumentasi.** Setiap string baru perlu baris di `crates/ternarywire-app/i18n/id.tsv`
   (tes i18n gagal tanpa itu). Perbarui skrip automation dan screenshot di `docs/screenshots`.

Setiap layar di prototipe punya bagian "Implementasi egui" di Catatan desain dengan nama fungsi Rust yang
perlu diubah.
