/* Page behaviour: icons, the latest release, the visitor's system, install tabs, copy buttons,
   the live window, the shots with tabs, the shortcut table, the lattice and reveal on scroll. */
(() => {
  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => [...r.querySelectorAll(s)];
  const REPO = 'ilhamnadhif/ternarywire';
  const GH = `https://github.com/${REPO}`;
  const PH = window.PH || {};
  const store = {
    get(k) { try { return JSON.parse(sessionStorage.getItem(k)); } catch { return null; } },
    set(k, v) { try { sessionStorage.setItem(k, JSON.stringify(v)); } catch { /* private mode */ } },
  };

  /* ---------------------------------------------------------- icons (Phosphor, by name) */
  const fillIcons = (root = document) => $$('[data-i]', root).forEach(el => {
    const cp = PH[el.dataset.i];
    if (cp) el.textContent = String.fromCodePoint(cp);
  });
  fillIcons();

  /* ---------------------------------------------------------- nav */
  const nav = $('#nav');
  const onScroll = () => nav.classList.toggle('solid', scrollY > 8);
  addEventListener('scroll', onScroll, { passive: true });
  onScroll();

  /* ---------------------------------------------------------- the visitor's system */
  const ua = navigator.userAgent;
  const plat = (navigator.userAgentData && navigator.userAgentData.platform) || navigator.platform || '';
  const OS = /Win/i.test(plat) || /Windows/.test(ua) ? 'win'
    : /Mac|iPhone|iPad|iPod/i.test(plat) || /Mac OS X|iPhone|iPad/.test(ua) ? 'mac'
    : /Linux|X11|CrOS/i.test(plat) || /Linux|X11/.test(ua) ? (/Android/.test(ua) ? 'mac' : 'linux') : 'mac';
  const MAC = OS === 'mac';
  const NAME = { mac: 'macOS', win: 'Windows', linux: 'Linux' };

  /* ---------------------------------------------------------- release: version, files, sizes */
  const FALLBACK = '0.3.2';
  const FILE = {
    'dmg': v => `TernaryWire-${v}-universal.dmg`,
    'cask': () => 'ternarywire.rb',
    'cli-mac': v => `ternarywire-cli-${v}-macos-universal.tar.gz`,
    'win-setup': v => `TernaryWire-Setup-${v}-x64.exe`,
    'win-zip': v => `ternarywire-${v}-windows-x86_64.zip`,
    'deb-x64': v => `ternarywire_${v}_amd64.deb`,
    'deb-arm': v => `ternarywire_${v}_arm64.deb`,
    'rpm-x64': v => `ternarywire-${v}-1.x86_64.rpm`,
    'rpm-arm': v => `ternarywire-${v}-1.aarch64.rpm`,
    'appimage-x64': v => `TernaryWire-${v}-x86_64.AppImage`,
    'appimage-arm': v => `TernaryWire-${v}-aarch64.AppImage`,
    'tar-x64': v => `ternarywire-${v}-linux-x86_64.tar.gz`,
    'tar-arm': v => `ternarywire-${v}-linux-aarch64.tar.gz`,
  };
  const mb = n => `${Math.max(1, Math.round(n / 1048576))} MB`;
  $$('[data-size]').forEach(el => { el.dataset.base = el.textContent; });
  let winDirect = '';
  function applyRelease(v, sizes) {
    $$('[data-version]').forEach(el => { el.textContent = v; });
    $$('[data-release-link]').forEach(a => { a.href = a.closest('.inst-f') ? `${GH}/releases` : `${GH}/releases/tag/v${v}`; });
    $$('[data-asset]').forEach(a => {
      const name = FILE[a.dataset.asset](v);
      const known = !sizes || name in sizes;
      a.href = known ? `${GH}/releases/download/v${v}/${name}` : `${GH}/releases/latest`;
      const s = $('[data-size]', a);
      if (s && sizes && sizes[name] && a.dataset.asset !== 'cask') s.textContent = `${s.dataset.base} · ${mb(sizes[name])}`;
    });
    winDirect = $('[data-asset="win-setup"]').href;
    setCta();
  }
  const cached = store.get('tw-release');
  applyRelease(cached ? cached.v : FALLBACK, cached ? cached.sizes : null);
  if (!cached || Date.now() - cached.at > 30 * 60 * 1000) {
    fetch(`https://api.github.com/repos/${REPO}/releases/latest`, { headers: { Accept: 'application/vnd.github+json' } })
      .then(r => (r.ok ? r.json() : null))
      .then(j => {
        if (!j || !j.tag_name) return;
        const v = String(j.tag_name).replace(/^v/, '');
        const sizes = {};
        (j.assets || []).forEach(a => { sizes[a.name] = a.size; });
        store.set('tw-release', { v, sizes, at: Date.now() });
        applyRelease(v, sizes);
      })
      .catch(() => { /* offline or rate-limited: the links above still work */ });
  }

  /* ---------------------------------------------------------- calls to action */
  function setCta() {
    $$('#cta-main, [data-cta-main]').forEach(a => {
      $('span', a).textContent = `Download for ${NAME[OS]}`;
      a.href = OS === 'win' && winDirect ? winDirect : '#install';
      a.dataset.os = OS;
    });
  }
  const note = $('#cta-note');
  if (note && OS !== 'mac') {
    const others = ['mac', 'win', 'linux'].filter(o => o !== OS);
    note.innerHTML = `Free and open source · also for <a href="#install" data-os-link="${others[0]}">${NAME[others[0]]}</a> and <a href="#install" data-os-link="${others[1]}">${NAME[others[1]]}</a>`;
  }

  /* ---------------------------------------------------------- install tabs */
  const tabs = $$('.tabs-os [role="tab"]');
  function selectTab(os) {
    tabs.forEach(t => {
      const on = t.id === `tab-${os}`;
      t.setAttribute('aria-selected', String(on));
      t.tabIndex = on ? 0 : -1;
      $(`#${t.getAttribute('aria-controls')}`).hidden = !on;
    });
  }
  tabs.forEach((t, i) => {
    t.addEventListener('click', () => selectTab(t.id.slice(4)));
    t.addEventListener('keydown', e => {
      if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft') return;
      const n = tabs[(i + (e.key === 'ArrowRight' ? 1 : tabs.length - 1)) % tabs.length];
      n.focus(); n.click();
    });
  });
  selectTab(OS);
  document.addEventListener('click', e => {
    const a = e.target.closest('[data-os-link], #cta-main, [data-cta-main], [data-dl-nav]');
    if (!a) return;
    const os = a.dataset.osLink || a.dataset.os || OS;
    if (a.getAttribute('href') === '#install') selectTab(os);
  });

  /* ---------------------------------------------------------- copy */
  $$('[data-copy]').forEach(b => b.addEventListener('click', async () => {
    const src = $(`#${b.dataset.copy}`);
    const text = src.textContent.trim();
    let ok = false;
    try { await navigator.clipboard.writeText(text); ok = true; } catch {
      const r = document.createRange(); r.selectNodeContents(src);
      const s = getSelection(); s.removeAllRanges(); s.addRange(r);
      try { ok = document.execCommand('copy'); } catch { ok = false; }
    }
    const label = $('span', b), icon = $('.ic', b);
    label.textContent = ok ? 'Copied' : 'Select it';
    icon.dataset.i = ok ? 'check' : 'copy'; fillIcons(b);
    b.classList.toggle('ok', ok);
    clearTimeout(b._t);
    b._t = setTimeout(() => { label.textContent = 'Copy'; icon.dataset.i = 'copy'; fillIcons(b); b.classList.remove('ok'); }, 1800);
  }));

  /* ---------------------------------------------------------- shortcuts */
  // [id, action, macOS, Windows & Linux]
  const KEYS = [
    ['send', '<b>Send the request</b>', ['⌘↵'], ['Ctrl+Enter']],
    ['search', 'Search requests', ['⌘K'], ['Ctrl+K']],
    ['palette', 'Command palette', ['⇧⌘P'], ['Ctrl+Shift+P']],
    ['env', 'Switch environment', ['⌘E'], ['Ctrl+E']],
    ['new', 'New request · new tab', ['⌘N', '⌘T'], ['Ctrl+N', 'Ctrl+T']],
    ['close', 'Close tab', ['⌘W'], ['Ctrl+W']],
    ['tabs', 'Next · previous tab', ['⇧⌘]', '⇧⌘['], ['Ctrl+Shift+]', 'Ctrl+Shift+[']],
    ['save', 'Save', ['⌘S'], ['Ctrl+S']],
    ['sidebar', 'Toggle sidebar', ['⌘B'], ['Ctrl+B']],
    ['console', 'Console', ['⌥⌘C'], ['Ctrl+Alt+C']],
    ['settings', 'Settings', ['⌘,'], ['Ctrl+,']],
    ['accept', 'Accept a variable suggestion', ['Tab'], ['Tab']],
  ];
  const caps = combo => (combo.includes('+') || combo === 'Tab' ? combo.split('+') : [...combo]).map(k => `<kbd>${k}</kbd>`).join('');
  const combos = list => list.map(caps).join('<span style="color:var(--fg-4);margin:0 6px">·</span>');
  let keysOs = MAC ? 'mac' : 'pc';
  function drawKeys() {
    const col = keysOs === 'mac' ? 2 : 3;
    $('#keygrid').innerHTML = KEYS.map(k => `<div class="krow"><span>${k[1]}</span><span class="kk">${combos(k[col])}</span></div>`).join('');
    $('#keys-note').innerHTML = keysOs === 'mac'
      ? 'The command palette, <kbd>⇧</kbd><kbd>⌘</kbd><kbd>P</kbd>, lists all of them.'
      : 'On Windows and Linux, <kbd>⌘</kbd> is <kbd>Ctrl</kbd>. The command palette, <kbd>Ctrl</kbd><kbd>Shift</kbd><kbd>P</kbd>, lists all of them.';
    $$('.seg-os button').forEach(b => b.setAttribute('aria-pressed', String(b.dataset.keys === keysOs)));
    // the shortcuts quoted in the text follow the same choice
    $$('[data-k]').forEach(el => { const k = KEYS.find(r => r[0] === el.dataset.k); if (k) el.innerHTML = combos(k[col]); });
    const tk = $('#try-k');
    if (tk) tk.innerHTML = combos(KEYS[0][col]);
  }
  $$('.seg-os button').forEach(b => b.addEventListener('click', () => { keysOs = b.dataset.keys; drawKeys(); }));
  drawKeys();

  /* ---------------------------------------------------------- the live window */
  const frame = $('#demo-if'), scaler = $('#demo-scaler');
  if (frame && scaler) {
    const fit = () => { frame.style.transform = `scale(${scaler.clientWidth / 1280})`; };
    new ResizeObserver(fit).observe(scaler);
    fit();
    const post = msg => { try { frame.contentWindow.postMessage(msg, '*'); } catch { /* not loaded yet */ } };
    const chips = $$('#try [data-go]');
    // the chip of a screen: every HTTP state is HTTP, the local servers are "Mock server", ...
    const chipOf = id => id === 'http-graphql' ? id
      : /^(http|console)/.test(id) ? 'http'
      : /^(mock|webhook|capture)/.test(id) ? 'mock'
      : id.replace(/-.*/, '');
    const mark = id => chips.forEach(b => b.setAttribute('aria-pressed', String(b.dataset.go === chipOf(id))));
    chips.forEach(b => b.addEventListener('click', () => post({ go: b.dataset.go })));
    $$('.seg-mini [data-theme]').forEach(b => b.addEventListener('click', () => {
      post({ theme: b.dataset.theme });
      $$('.seg-mini [data-theme]').forEach(x => x.setAttribute('aria-pressed', String(x === b)));
    }));
    $('#demo-reset').addEventListener('click', () => post({ go: 'http' }));
    let sentOnce = false, ready = false, seen = false;
    // The first time the window is in view, it sends the request once, to show that it's live.
    const firstSend = () => {
      if (sentOnce || !ready || !seen || matchMedia('(prefers-reduced-motion: reduce)').matches) return;
      sentOnce = true;
      setTimeout(() => post({ send: true }), 900);
    };
    addEventListener('message', e => {
      if (e.source !== frame.contentWindow || !e.data || !e.data.tw) return;
      if (e.data.screen) mark(e.data.screen);
      if (e.data.ready) { ready = true; firstSend(); }
    });
    if ('IntersectionObserver' in window) {
      const io = new IntersectionObserver(es => { if (es.some(x => x.isIntersecting)) { seen = true; firstSend(); io.disconnect(); } }, { threshold: .45 });
      io.observe(scaler);
    }
    // If the window doesn't come up (a script failed to load), show the app's screenshot instead.
    frame.addEventListener('load', () => setTimeout(() => {
      if (ready) return;
      scaler.innerHTML = '<img src="assets/img/shots/http.webp" alt="TernaryWire: the List books request on the left and its JSON response on the right" width="1800" height="1125" style="width:100%;height:100%;object-fit:cover">';
      $('#demo').classList.add('static');
    }, 2500));
    mark('http');
  }

  /* ---------------------------------------------------------- shots with tabs */
  $$('[data-show]').forEach(show => {
    const btns = $$('[role="tab"]', show), img = $('.shot img', show), cap = $('.show-cap', show);
    const pick = b => {
      btns.forEach(x => { const on = x === b; x.setAttribute('aria-selected', String(on)); x.tabIndex = on ? 0 : -1; });
      cap.innerHTML = b.dataset.cap;
      const src = `assets/img/shots/${b.dataset.shot}.webp`;
      if (img.getAttribute('src') === src) return;
      img.classList.add('swap');
      const pre = new Image();
      pre.onload = pre.onerror = () => { img.src = src; img.alt = b.dataset.alt; img.classList.remove('swap'); };
      pre.src = src;
    };
    btns.forEach((b, i) => {
      b.addEventListener('click', () => pick(b));
      b.addEventListener('keydown', e => {
        if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft') return;
        const n = btns[(i + (e.key === 'ArrowRight' ? 1 : btns.length - 1)) % btns.length];
        n.focus(); pick(n);
      });
    });
    pick(btns[0]);
    // the other shots load once this one is near
    if ('IntersectionObserver' in window) {
      const io = new IntersectionObserver(es => {
        if (!es.some(x => x.isIntersecting)) return;
        io.disconnect();
        btns.slice(1).forEach(b => { new Image().src = `assets/img/shots/${b.dataset.shot}.webp`; });
      }, { rootMargin: '400px 0px' });
      io.observe(show);
    }
  });

  /* ---------------------------------------------------------- the lattice behind the hero */
  // Lit points run along the icon's wire, in the colours of the methods.
  const WIRE = { 7: 0, 4: .25, 5: .5, 6: .75, 3: 1 };
  function lattice(field, n, avoid) {
    if (!field) return;
    const W = field.clientWidth, H = field.clientHeight, step = 24;
    const cols = Math.floor(W / 2 / step);
    const used = [];
    const kinds = ['--accent', '--m-get', '--accent', '--m-put', '--m-post', '--accent', '--m-get', '--m-patch', '--accent'];
    let tries = 0;
    field.querySelectorAll('.glyph').forEach(g => g.remove());
    for (let i = 0; i < n && tries < 400; tries++) {
      const cx = (Math.floor(Math.random() * (cols * 2 - 2)) - cols + 1);
      const cy = 1 + Math.floor(Math.random() * Math.max(1, Math.floor(H / step) - 4));
      const x = W / 2 + cx * step, y = 12 + cy * step;
      if (x < 16 || x + 64 > W || y + 64 > H) continue;
      if (avoid && avoid(x, y, W, H)) continue;
      if (used.some(([ux, uy]) => Math.abs(ux - x) < 150 && Math.abs(uy - y) < 110)) continue;
      used.push([x, y]);
      const g = document.createElement('span');
      g.className = i % 4 === 3 ? 'glyph still' : 'glyph';
      g.style.cssText = `left:${x - 1.5}px;top:${y - 1.5}px;--c:var(${kinds[i % kinds.length]})`;
      g.innerHTML = '<b></b>'.repeat(9);
      // the wire: points 7, 4, 5, 6, 3 light one after the other, each glyph at its own moment
      const off = Math.random() * 2.6;
      [...g.children].forEach((d, j) => { if (WIRE[j + 1] !== undefined) d.style.animationDelay = `${WIRE[j + 1] - off}s`; });
      field.appendChild(g);
      i++;
    }
  }
  const heroAvoid = (x, y, W) => Math.abs(x + 25 - W / 2) < Math.min(470, W * .36) || y > 600;
  const drawFields = () => {
    const small = innerWidth < 760;
    lattice($('#field-hero'), small ? 3 : 9, small ? (x, y) => y > 150 : heroAvoid);
    lattice($('#field-final'), small ? 0 : 7, (x, y, W, H) => Math.abs(x + 25 - W / 2) < Math.min(380, W * .3) && Math.abs(y - H / 2) < 260);
  };
  drawFields();
  let lastW = innerWidth;
  addEventListener('resize', () => { if (Math.abs(innerWidth - lastW) > 80) { lastW = innerWidth; drawFields(); } });

  /* ---------------------------------------------------------- reveal on scroll */
  if ('IntersectionObserver' in window) {
    const io = new IntersectionObserver(es => es.forEach(e => { if (e.isIntersecting) { e.target.classList.add('in'); io.unobserve(e.target); } }), { rootMargin: '0px 0px -8% 0px' });
    $$('.rv').forEach(el => io.observe(el));
  } else $$('.rv').forEach(el => el.classList.add('in'));
})();
