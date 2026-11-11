# TernaryWire UI design guide

The app is drawn with egui. Every screen uses the same tokens, grid and
components so it looks like one product. Read this before changing UI code.

The visual source of truth is the prototype in `design/redesign/` (open
`index.html`; `README.md` there explains it). Every screen of the app has a
screen in the prototype, and `design/redesign/fixture/compare.sh <screen-id>`
renders both at the same size for a side-by-side check.

## Layout

```
title bar 44 · rail 52 | sidebar 224–276 | workbench card (radius 10, 8 px from the window's right edge)
                                          ├ tab strip 40
                                          ├ view title 46
                                          ├ panes: left (request, compose, list) | 9 px gutter | right (response, traffic, detail)
                                          │   each pane: action/status row 56 → tabs 36 → toolbar 40 → content (padding 0 12 16)
                                          └ console drawer (optional)
status bar 28
```

- Rows that sit side by side in the two panes have the same height, so their
  hairlines line up across the window (`theme::size`).
- Views get the whole area under the tab strip with no margin
  (`app.rs` `bench()`), and draw their own rows: `kit::bar` for fixed rows,
  `kit::panes` for the two panes, `kit::tabs` for pane tabs, `kit::toolbar`,
  `kit::pane_body` for scrolling content.
- Side by side when the view is at least `size::SIDE_BY_SIDE_MIN` (880) wide
  (HTTP also follows the Side/Stacked setting); narrower views stack the panes.
- Request on the left, response on the right; the URL bar and Send belong to
  the left pane, the response status (code, time, size, tests) to the right.

## Tokens (`crates/ternarywire-app/src/theme.rs`)

- Colors: `theme::pal(ui)` returns the `Palette` of the current theme:
  `canvas` (the workbench card), `chrome` (title bar, rail, sidebar, status
  bar), `surface` (inputs, cards, menus, dialogs), `well` (code wells, table
  headers, the tab strip, side panels), `hover`, `subtle` (pressed, segmented
  track, count badges), `border`, `border_strong` (input borders), `text`,
  `text_muted`, `text_faint`, `accent` (coral red of the icon), `accent_hover`,
  `accent_soft`, `accent_text`, `ok`/`warn`/`err`/`info` and their `*_soft`
  fills, `var_ok`/`var_ok_bg`/`var_bad_bg` for `{{variable}}` chips.
  `Palette::tone(Tone)` gives a (text, fill) pair; `theme::status_tone(code)`.
  Never hard-code RGB values in screens.
- Methods and statuses: `theme::method_color`, `theme::status_color`.
- Type: Inter (body 13, small 12) and JetBrains Mono (12.5). Weights:
  `FontId::proportional` (400), `theme::medium` (500: tabs, buttons, labels),
  `theme::semibold` (600: titles, section titles 12.5). `RichText::strong()`
  only changes color; use the font helpers for real weight.
- Radius: `RADIUS` 7 (controls), `RADIUS_SM` 6 (small controls), `RADIUS_CARD`
  9, `RADIUS_LG` 10 (workbench card, toasts, menus), `RADIUS_DIALOG` 14.
- Grid: `theme::size` (`TITLE_BAR`, `TAB_STRIP`, `VIEW_TITLE`, `ACTION_ROW`,
  `TABS`, `TOOLBAR`, `PAD`, `ROW`, `TABLE_HEAD`, `TREE_ROW`, `GUTTER`, …).
  Spacing: 8 px between controls in a row, 12–14 between groups.

## Icons (`crates/ternarywire-app/src/icons.rs`)

Phosphor icons, usable inside any text: `format!("{} {}", icons::PLUS, tr("New"))`
or as the `icon` of a kit component. Do not use emoji or symbol characters as
icons. To add an icon, add its Phosphor name to `ICONS` in
`scripts/build-fonts.py`, rerun the script (see its header), then
`python3 design/redesign/build-assets.py` so the prototype has it too.

## Components (`crates/ternarywire-app/src/kit.rs`)

| Need | Use |
| --- | --- |
| Main action of a pane (Send, Connect, Invoke, Run) | `kit::action_button` (38) |
| Buttons | `kit::button(ui, Kind, Size, text, ButtonOpts{icon, caret, kbd, on, min_width})`, `primary`, `secondary`, `ghost` (24), `button_ex` |
| Icon-only button (always with a tooltip = accessible name) | `kit::icon_button` (28), `icon_button_sm` (22), `icon_button_ex` (on/off state, any size) |
| Tabs of a pane | `kit::tabs(ui, &mut cur, &[Tab::new(v, label).count(n).badge(text, Tone).dot(b)])`, `tabs_with` (controls on the right) |
| Choice between 2–5 views | `kit::segmented_sm` (26), `segmented_ex(.., small)` (28 or 26, optional icons) |
| Status / method / count label | `kit::pill_ex(ui, text, Tone, PillOpts{dot, live, large, outline, icon})`, `tone_pill`, `status_pill`, `widgets::method_badge` |
| Metrics in a status row | `kit::metric`, `metric_colored` |
| Keyboard hint | `kit::kbd(ui, "⌘↩")` |
| Checkbox, switch | `kit::check`, `check_ex` (mixed), `checkbox` (with label), `switch` |
| Drop-down button | `kit::select_box`, `select_box_ex` (open a `Popup::menu` from it) |
| Search box | `kit::search_field_sm` (26) |
| Section title, eyebrow, help text | `kit::section_title`, `eyebrow`, `hint` |
| Bordered group, recessed panel | `kit::card`, `kit::well` |
| Settings rows | `kit::group(ui, title, help, \|ui, rows\| rows.row(ui, label, help, control))` |
| Label above a field | `kit::field` |
| Messages | `kit::banner(ui, Tone, icon, text)`, `banner_with` |
| Icon on a soft square | `kit::icon_tile` |
| Nothing to show yet | `kit::empty_state(ui, icon, title, body, actions)` |
| Collapsible section | `kit::disclosure` |
| Menus | `kit::menu_item`, `menu_item_ex(MenuItem{icon, method, dot, checked, label, shortcut, detail, danger, submenu})`, `menu_header`, `menu_sep` |
| Sidebar rows | `kit::tree_row(ui, TreeRow{…}, trailing)` → (response, hover-actions rect), `kit::sidebar_group`; footer: `sidebar::sidebar_footer` |
| Key/value editing | `widgets::kv_table` (set `KvOptions::title`) |
| Code editing | `widgets::code_editor_gutter` inside `kit::well` (line numbers, 20 px lines) |
| Masked secret | `widgets::secret_field` / `auth_ui::secret_input` |
| Dialog | `dialogs::Modal` with `dialogs::footer` (Cancel secondary, main action primary on the right) |
| Focus of a hand-drawn input | `kit::focus_ring(ui, rect, radius)` |

egui notes:

- To draw inside an area that is already allocated (cells, rows, a bar), use
  `ui.new_child(UiBuilder…)`; `scope_builder` moves the parent's cursor.
- A `TextEdit` ignores `.margin()` once `.frame(...)` is set: put the padding
  in the frame (`Frame::NONE.inner_margin(…)`).
- Long lists lay out only the rows in view with `kit::show_rows` (rows of one
  fixed height, edge to edge).

## App shell on macOS

- **Title bar**: the window content runs under a transparent native title bar
  (`main.rs`); `app.rs` `top_bar` draws the 44 px bar and `macos_titlebar.rs`
  centers the traffic lights in it. Its empty space moves the window.
- **Menu bar** (`menu.rs`, muda): an item with a shortcut hands egui the same
  key press the shortcut would have, so `TernaryWireApp::shortcuts` and text
  editing stay the one place that handles keys on every platform.
- **Scrolling**: trackpad gestures stick to one axis (`scroll_lock.rs`). A
  canvas that pans in 2D calls `scroll_lock::allow_free_scroll(ctx, rect)`.
- **Fonts**: the bundled fonts cover Latin, Greek and Cyrillic; other scripts
  come from the system fonts through eframe's `system_fonts` feature.

## Rules

- One primary (accent) button per view.
- Every icon-only control has a tooltip and an accessible name.
- Empty, loading and error states are designed (empty state with an action,
  spinner with text, error with icon, cause and next step).
- Text that users read goes through `tr()`/`trf()` with an Indonesian entry in
  `crates/ternarywire-app/i18n/id.tsv`.
- Check both themes and a narrow window: `design/redesign/fixture/compare.sh
  <screen-id> light|dark [WxH]` (e.g. `900x640`) writes the app and the
  prototype side by side.
- Screenshots without a window: the ignored test `automation::tests::offscreen`
  runs a `TERNARYWIRE_AUTOMATION` script on the real app drawn on the GPU, with
  settings from a scratch `TERNARYWIRE_CONFIG_DIR` (`TERNARYWIRE_SIZE=1280x820`,
  `TERNARYWIRE_PPP=2`), and lists widgets that have no accessible name:

  ```sh
  TERNARYWIRE_CONFIG_DIR=/tmp/m/config TERNARYWIRE_SECRETS=memory TERNARYWIRE_AUTOMATION=steps.json \
    cargo test -p ternarywire-app offscreen -- --ignored --nocapture
  ```
