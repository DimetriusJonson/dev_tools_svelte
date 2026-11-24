# AGENTS.md — WebDev Useful Tools

## 0. Documentation Contract

- `README.md` is the product source of truth: user-visible behavior, acceptance criteria, supported features, limitations.
- This file is the engineering contract: file map, commands, architecture, invariants, and implementation pitfalls.
- Current code and tests win over stale prose. If behavior changes, update `README.md` in the same change; if implementation details change, update this file.
- Do not duplicate the full product specification here. Link behavior to implementation instead.

## 1. Project Overview & Stack

- **Application:** WebDev Useful Tools / «Полезные инструменты веб-разработчика», version `<version>`.
- **Desktop runtime:** Tauri v2, Rust 2021, Windows/macOS/Linux.
- **Frontend:** SvelteKit 2 with `@sveltejs/adapter-static`, SPA mode, `ssr = false`, `prerender = false`, fallback `index.html`.
- **UI:** Svelte 5 runes, TypeScript, Tailwind CSS, CSS variables.
- **Editors:** CodeMirror 6 (`basicSetup`, JSON/XML/HTML language packages — `@codemirror/lang-xml`, `@codemirror/lang-json`, `@codemirror/lang-html` — one-dark highlight style).
- **Database:** SQLite via `rusqlite` with `bundled` feature. Not `tauri-plugin-sql`.
- **File processing:** `quick-xml 0.42`, `serde_json` with `preserve_order`. WSDL 1.1 and XSD parsing are `quick-xml`-only too, in `src-tauri/src/wsdl/`; no additional crate was added for them.
- **Tauri plugins:** `dialog`, `single-instance`, `deep-link`, `clipboard-manager`, `updater`.
- **Testing:** Vitest + jsdom for frontend; Rust unit tests for DB, processing, and tray mapping.
- **Current status:** XML, JSON, URL, Compare, Share (desktop only: retention select, the `Ссылка`/`Link` result tab, memory-only uploads, viewer delete), Rest client (including `multipart/form-data` bodies with fields and file parts, and the `Raw` dump tabs for the request and for the response), WSDL client (desktop only: WSDL 1.1 import, XSD request template, tree explorer, `Headers`/`Raw` request tabs with locked binding headers, SOAP 1.1/1.2 send, response tabs with `Save`/`Copy`/`Format`/download), themes, i18n, persistence, notifications, tray, single-instance, deep links, updates (desktop only: startup + navbar check, banner with install), and large-file processing are implemented. The updater's install path is unverified until a signed release with a `latest.json` asset is published.

## 2. Commands & Definition of Done

```bash
npm install
npm run dev
npm run check
npm run test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo fmt --check --manifest-path src-tauri/Cargo.toml
npm run tauri dev
npm run tauri build
```

Useful variants:

```bash
npm run test:watch
npm run tauri build -- --no-bundle
npx tauri signer generate -w ~/.tauri/webdev-useful-tools.key --ci
TAURI_SIGNING_PRIVATE_KEY=~/.tauri/webdev-useful-tools.key npm run tauri build
git tag v0.1.0 && git push origin v0.1.0   # cuts the GitHub Release via .github/workflows/release.yml
```

- `npm run dev` runs Vite on `http://localhost:1420` with `strictPort: true`.
- GitHub Actions pins `node-version: '24'` in both `preflight` and `build`. `lts/*` now resolves to Node 26, whose own `localStorage` accessor answers `undefined` unless the process got `--localstorage-file`; `src/lib/test-setup.ts` repairs that, so the suite also passes on Node 20/26 (`vitest run` under a Node 26 binary is the quickest way to prove it).
- If `npm run tauri dev` fails with "Port 1420 is already in use", a stale Vite is holding it. When Tauri runs on Windows while a WSL-side `npm run dev` is alive, WSL localhost forwarding reserves 1420 on the Windows side too; find it with `ss -ltnp | grep 1420` (WSL) and stop that `vite` process.
- `node_modules` installed from Windows only carries the win32 rollup/esbuild binaries, so Vitest, `npm run check` and `npm run build` fail under WSL with "Cannot find module @rollup/rollup-linux-x64-gnu". Restore them without touching `package-lock.json`: `npm pack @rollup/rollup-linux-x64-gnu@<rollup version> @esbuild/linux-x64@<esbuild version>`, then `tar -xzf <tgz> -C node_modules/@rollup/rollup-linux-x64-gnu --strip-components=1` (and `-C node_modules/@esbuild/linux-x64`), then `chmod +x node_modules/@esbuild/linux-x64/bin/esbuild`. A Windows-side `npm i` deletes these directories again; `npm ci` inside WSL is the persistent fix.
- `npm run check` runs `svelte-kit sync && svelte-check`.
- `npm run build` writes the static frontend to `build/`.
- `npm run tauri build` also produces updater artifacts because `bundle.createUpdaterArtifacts` is `true`; that requires `TAURI_SIGNING_PRIVATE_KEY` (and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` if the key has a password). Ship `latest.json` plus the installer and its `.sig` as release assets, or every check answers "not found".
- `cargo fmt --check` is not part of the release gate: the tree is not rustfmt-clean yet (`src-tauri/src/lib.rs` keeps the tray imports and the `TrayIconEvent::DoubleClick` pattern on one line), so `release.yml` runs `cargo test` only. Run `cargo fmt` and re-enable the check in `preflight` once the tree is formatted.
- `.github/workflows/release.yml` does that shipping: a `v<version>` tag push runs the checks (frontend tests, `svelte-check`, `cargo test`, one version across `package.json`/`tauri.conf.json`/`Cargo.toml`, tag equals the app version, `TAURI_SIGNING_PRIVATE_KEY` present) and then four `tauri-apps/tauri-action@v1` builds — Linux `deb`+`appimage`, Windows `msi`, macOS `app`+`dmg` for both Apple targets with ad-hoc `signingIdentity: "-"`. `workflow_dispatch` builds the same bundles but uploads them only as workflow artifacts (`tagName` is empty, so the action skips release uploads entirely). See the updater section for why the matrix is serialized and how the draft gets published.
- Rust tests live in `src-tauri/src/*.rs` and `src-tauri/src/wsdl/*.rs` under `#[cfg(test)]`.

Definition of Done for a code change:

1. The behavior matches the relevant `README.md` section and acceptance criteria.
2. `npm run test`, `npm run check`, and affected Rust tests pass.
3. Production frontend build succeeds when routes or frontend code change.
4. No new external icon/UI library and no empty SVG fragments.
5. New persistence keys, commands, or files are documented in this file.
6. No unrelated refactors or generated artifacts are committed.

## 3. Directory Map

```text
src/
├── app.css                              # theme variables, CodeMirror/diff/resizer styles
├── app.html                             # pre-hydration theme script, loading splash (markup + inline CSS + labels)
├── lib/
│   ├── test-setup.ts                     # vitest setup: browser storage shim for Node ≥ 25
│   ├── test-setup.test.ts                # the shim's round trip + "storage is usable" guard
│   ├── components/
│   │   ├── CodeMirrorEditor.svelte       # reusable CodeMirror wrapper + highlight API
│   │   ├── CodeMirrorEditor.test.ts      # HTML markup really highlights; plain text stays unstyled
│   │   ├── Icon.svelte                   # inline SVG path dictionary
│   │   ├── Navbar.svelte                 # nav pills, language, theme controls, desktop update check
│   │   ├── PropertyEditor.svelte         # shared name/value table for REST/WSDL properties + invalid-cell hints + locked rows
│   │   ├── PropertyFilesEditor.svelte    # Form tab's Files table: name cell + native file picker cell
│   │   ├── RestExplorer.svelte           # REST projects and request list UI
│   │   ├── WsdlExplorer.svelte           # WSDL projects, clipboard import + help popover, file→service→operation tree
│   │   ├── WsdlExplorer.test.ts          # file menu: the delete confirmation survives its own click
│   │   ├── Toaster.svelte                # global notification stack
│   │   ├── UpdateBanner.svelte           # update banner: version, notes, progress, install/restart
│   │   └── XmlJsonFormatter.svelte       # shared XML/JSON tool UI
│   ├── services/
│   │   ├── db.ts                         # Tauri IPC + localStorage fallback
│   │   ├── files.ts                      # native dialogs, path helpers, sharedFileName, pickPartFile for multipart rows
│   │   ├── files.test.ts                 # basename across separators + formatted output naming
│   │   ├── rest.ts                       # REST IPC wrappers + browser fallback + explorer selection persistence
│   │   ├── rest.test.ts                  # selection restore rules and browser-storage round trip
│   │   ├── share.ts                      # share IPC wrappers + viewer URL helpers + retention codes and persistence
│   │   ├── share.test.ts                 # byte formatting, token-free viewer URL, retention default/round trip, desktop-only delete
│   │   ├── updater.ts                    # plugin check/downloadAndInstall/close wrappers + restart_app
│   │   ├── wsdl.ts                       # WSDL IPC wrappers, desktop-only guard, explorer selection persistence
│   │   └── wsdl.test.ts                  # desktop-only rejection and selection restore rules
│   ├── stores/
│   │   ├── i18n.ts                       # en/ru dictionaries, locale detection
│   │   ├── i18n.test.ts                  # key + placeholder parity, dialog and nav keys in both
│   │   ├── share.ts                      # draftShare handoff from XML/JSON result
│   │   ├── theme.ts                      # system/dark/light mode
│   │   ├── toast.ts                      # transient success/error/info notifications
│   │   ├── toast.test.ts                 # stack cap, message precedence, localized fallback
│   │   └── update.ts                     # shared update flow: one check feeds banner and navbar
│   └── utils/
│       ├── clipboard.ts                  # clipboard read/write with notification fallback
│       ├── diff.ts                       # line LCS + inline token diff
│       ├── diff.test.ts
│       ├── formatters.ts                 # XML/JSON/URL transformations
│       ├── formatters.test.ts
│       ├── rest.ts                       # query/cookies/form + cURL helpers + raw request/response dumps + RFC validation + Content-Disposition filename
│       ├── rest.test.ts
│       ├── splash.ts                     # loading splash labels, version, dismissal
│       ├── splash.test.ts
│       ├── update.ts                     # update phases, percent, dismiss check, error grouping
│       ├── update.test.ts
│       ├── validation.ts                 # shared RFC validation keys, cell hints and blocked message for /rest and /wsdl
│       ├── validation.test.ts
│       ├── wsdl.ts                       # SOAP envelope + locked-header mirrors, endpoint choice, raw response dump formatting, import error/warning codes → i18n keys
│       └── wsdl.test.ts
└── routes/
    ├── +layout.svelte                    # navbar, banner, full-height shell, splash dismissal, deep-link navigation, startup update check
    ├── +layout.ts                        # ssr = false; prerender = false
    ├── +page.ts                          # 307 redirect to /xml
    ├── xml/+page.svelte                  # XmlJsonFormatter mode="xml"
    ├── json/+page.svelte                 # XmlJsonFormatter mode="json"
    ├── url/+page.svelte                  # URL encode/decode
    ├── compare/+page.svelte              # diff UI
    ├── share/+page.svelte                # desktop share UI: interfaces, File/Content/Link tabs, retention select, upload
    ├── share/+page.test.ts               # the `Ссылка` tab: appears on upload, hides with the result, browser stub
    ├── view/+page.svelte                 # read-only viewer opened by deep link, with the delete-upload control
    ├── rest/+page.svelte                 # REST client: explorer, request editor, response
    └── wsdl/+page.svelte                 # WSDL client: explorer, request template panel, SOAP response

src-tauri/
├── Cargo.toml
├── tauri.conf.json
├── capabilities/default.json
├── icon-source.png                       # source PNG for icon generation
├── icons/                                # generated bundle + app-icon.svg
└── src/
    ├── main.rs
    ├── lib.rs                            # tray, single-instance, DB init, commands, deep link, WSDL fetch state
    ├── db.rs                             # schema, editor/settings CRUD, shared_uploads CRUD, WSDL schema
    ├── processor.rs                      # large-file XML/JSON processing
    ├── rest.rs                           # REST projects/requests persistence, multipart encoding, execution, cancellation
    ├── share.rs                          # interfaces, deep link, upload/viewer commands, server state
    ├── update.rs                         # restart_app command used outside Windows after an update installs
    ├── wsdl/
    │   ├── mod.rs                        # WSDL models/constants + the 12 `cmd_*` Tauri command wrappers
    │   ├── parser.rs                     # WSDL 1.1 `XmlDom` arena, `parse_wsdl`, `fetch_wsdl`
    │   ├── xsd.rs                        # XSD index and SOAP request-template generator
    │   └── store.rs                      # WSDL projects/files/services/operations/endpoints/state CRUD
    └── http_server.rs                    # axum share server on port 3030: GET reads + DELETE by id

src-tauri/tests/http_client.rs            # reqwest/axum plain-HTTP round trip
screenshots/                              # design references, not automated tests
static/favicon.svg
.github/workflows/release.yml             # tag-triggered builds, signed updater manifest, release publish
```

## 4. Frontend Architecture & UI Rules

### Svelte and routes

- Use Svelte 5 runes: `$state`, `$derived`, `$effect`, `$props`.
- The app is a SPA. Do not add server-only APIs, SSR data loading, or `+page.server.ts`.
- `/` only redirects to `/xml`; feature pages are client components.
- `+layout.svelte` owns the full-height shell: `h-screen`, `overflow-hidden`, navbar, `min-h-0` main area.

### CodeMirror wrapper

`CodeMirrorEditor.svelte` props:

```ts
{
  value?: string;
  language?: 'xml' | 'json' | 'html' | 'text';
  readonly?: boolean;
  dark?: boolean;
  placeholder?: string;
  ariaLabel?: string;
  onchange?: (value: string) => void;
  onfocus?: () => void;
}
```

Exported methods via `bind:this`:

```ts
setHighlights(highlights: Highlight[]): void;
clearHighlights(): void;
focusEditor(): void;
getEditorValue(): string;
```

- `language` reaches `languageExtension()`: `json` and `xml` use their language packages, `html` is `htmlLang({ selfClosingTags: true, autoCloseTags: false })` — `selfClosingTags` is what makes an XHTML response (`application/xhtml+xml`) parse, `<script>`/`<style>` bodies already get CSS and JS highlighting from that same package, and `autoCloseTags` only *writes* text, which a read-only response editor must never do. `text` adds no extension.
- `Highlight` has `from`, `to`, `class`, and optional `line`.
- `line: true` creates a line decoration; otherwise a mark decoration.
- Reconfigure language, read-only mode, placeholder, and theme through compartments instead of recreating the view.
- Keep external value synchronization guarded by `nextValue !== view.state.doc.toString()`.
- `CodeMirrorEditor.test.ts` mounts the component to prove a language really reaches the view. jsdom lays out no text, so the file stubs `Range.prototype.getClientRects`/`getBoundingClientRect` in `beforeAll` and restores them in `afterAll`; without them CodeMirror's measure pass throws on every decorated line. Highlighted spans carry the style module's generated class names, not `cm-tag`/`cm-attributeName`, so the test asserts that a tag name, an attribute name and an attribute value get three **distinct** non-`cm-` classes and that plain text gets none.

### Icons

- **No external icon libraries.** All icons live in `ICON_PATHS` in `src/lib/components/Icon.svelte`.
- Store one or more SVG `d` strings per icon. Render only `<path d={...} />`.
- **Never** use empty fragments `<>...</>` inside `<svg>`.
- Every name in `ICON_PATHS` is rendered by at least one component. Current names: `copy`, `download`, `share`, `chevronDown`, `sun`, `moon`, `monitor`, `arrowRight`, `check`, `swap`, `file`, `server`, `sparkles`, `globe`, `more`, `plus`, `trash`, `clipboard`, `braces`, `alignLeft`, `eraser`, `eye`, `close`, `refresh`, `update`, `help`, `warning`. Do not park unused icons in the dictionary — remove the entry when the last usage goes away.

### Styling and themes

- Use CSS variables from `src/app.css`; do not hardcode palette colors in new components when an existing variable fits.
- List-row states read `--row-hover` and `--row-active` as neutral grays (`#f3f4f6` light, `#242424` dark). They currently hold the same values as `--panel-bg-alt`, but keep them as separate tokens so row states can be retinted without repainting every panel; do not read row states straight from `--panel-bg-alt` in components.
- Row selection is not a tinted background: `--row-hover` and `--row-active` are the same gray in both themes, and the selected REST request row is identified by the inset accent stripe (`shadow-[inset_3px_0_0_var(--accent)]`), which is what keeps the selection legible. Amber (`--accent`) is reserved for buttons, the accent stripe and the method colors, not for row backgrounds.
- The REST row `...` actions button is rendered only for the selected request (`{#if active}`), never revealed by hover; right-click still opens the menu on any row.
- Theme is applied by toggling `.dark` and `data-theme` on `document.documentElement`.
- Pre-hydration theme initialization is in `src/app.html`; keep it to avoid a flash of the wrong theme.
- Theme order: `system → dark → light`; default `system`; storage key `devtools.theme`.
- Invalid REST input uses the `--danger`, `--danger-bg` and `--danger-text` tokens (defined for both themes in `src/app.css`), never hardcoded red utilities: cell backgrounds, tab error dots, the URL border and the remove-button hover all read from them.
- Interactive controls use `hover:scale-105 transition-all duration-150 active:scale-95` (icon-only buttons may use a stronger scale).
- Navigation hover must **not** paint a background highlight in either theme; only scale/transform. Active nav pill remains visually distinct.
- Navbar uses `overflow-y-hidden` and `py-1` to prevent the WebKit/WSLg scrollbar artifact caused by scaling.

### Loading splash

- `src/app.html` owns the splash markup (`#app-splash`, `.app-splash-*`), its inline CSS, and its localized labels. It renders before the bundle exists, so colors use `var(--token, fallback)` literals and the labels mirror `app.title` and the loading text from `i18n.ts`.
- Keep the splash markup outside the `%sveltekit.body%` mount container; hydration must not manage it, and removal must never fight Svelte's DOM.
- `src/lib/utils/splash.ts` exposes `setSplashTitle`, `setSplashVersion`, `splashHoldMs`, and `dismissSplash({ minimumMs, fadeMs, elapsed })`. `+layout.svelte` calls them in `onMount` with `$t('app.title')` and `version`, so the dictionary value and `package.json` version replace the template guesses at hydration.
- Minimum visible time is 350 ms and the fade is 260 ms, then the element is removed. `dismissSplash` and the setters are idempotent no-ops once the splash is gone.
- The `app.html` watchdog reveals a restart hint after 12 s so a bundle that never hydrates does not leave a silent spinner.
- Tests pass `elapsed` explicitly instead of relying on `performance.now()` under fake timers.

### Update banner

- `UpdateBanner.svelte` sits between `Navbar.svelte` and `<main>` in `+layout.svelte` as a `shrink-0` row, so it never steals editor height with `flex-1`. It renders only for `phase !== 'idle' && phase !== 'checking'`: the navbar spinner carries the "checking" state.
- The banner reads the shared `updateState` store, not its own copy. Phases: `available` (Install + Not now), `downloading` (percent label + `role="progressbar"` bar), `installing`, `ready` (Restart + Later), `error` (`--danger-bg`/`--danger-text`, `role="alert"`, close button that calls `clearUpdate`). Non-error phases use `role="status"` and `aria-live="polite"`.
- Row background stays `--panel-bg`; only failures read `--danger-bg`. Row-state tokens are not used here because the banner is not a list row.
- The Windows installer hint (`update.windowsHint`) shows during `installing`, and the restart hint (`update.readyHint`) shows in `ready` only outside Windows — `isWindowsUserAgent(navigator.userAgent)` is the client-side discriminator, kept in `utils/update.ts` so it is testable.
- The navbar check button is rendered only when `isTauriRuntime()`, uses `update` while idle and `refresh` with `animate-spin` while checking, is `disabled` while `isUpdateBusy(phase)`, and follows the navbar pill look (`h-11 w-11 rounded-full bg-[var(--nav-active-bg)] text-white`, scale on hover, no background hover highlight). Localized text lives in `title`/`aria-label` (`update.checkAction` / `update.checking`).

### Localization

- `src/lib/stores/i18n.ts` contains `en` and `ru` dictionaries.
- Use `$t('key', { param: value })`; never hardcode user-facing strings in pages/components.
- Keys no page spells out live here too: `nav.main` (navbar `aria-label`), `common.resizePanes` (both resizer handles) and the native dialog keys `dialog.selectXml|selectJson|selectShare|saveShared|saveFormatted`, `fileFilter.xml|json|text|all`, `rest.multipart.pickTitle`, read by `services/files.ts` through `dialogTitle(key)`, `fileFilters(mode)` and `allFileFilters()`. They interpolate nothing. `en` and `ru` are exported so `src/lib/stores/i18n.test.ts` can pin key and placeholder parity across dictionaries.
- Locale storage key: `devtools.locale`; default is `ru` when `navigator.language` starts with `ru`, otherwise `en`.
- Keep `document.documentElement.lang` in sync with the selected locale. `src/app.html` sets `lang` and the localized splash/document title from the stored locale before the bundle loads; the layout then owns the real title (`app.title` + version).
- The window/tab title comes from the `app.title` key plus the app version (`$app/environment` `version`, set from `package.json` via `kit.version.name` in `svelte.config.js`). `+layout.svelte` updates `document.title` and the Tauri window title through `getCurrentWindow().setTitle()` on locale change.
- REST validation messages live in `rest.validation.headerName|headerValue|cookieName|cookieValue|queryParam|formParam|multipartFieldName|multipartFieldValue|multipartFileName|fileMissing|nameRequired|blocked`. They interpolate `{name}` and `{char}`; `src/lib/utils/validation.ts` maps each `ValidationKind` to its key through `VALIDATION_KEYS` and renders them — `validationMessage(issue, t)` for one issue, `blockedMessage(issues, t)` wraps the first as `{detail}`, and `propertyFieldHint(kind, field, row, index, t)` turns a `validatePropertyField` result into a cell hint. Both `/rest` and `/wsdl` read the same helpers, so a bad header reads identically on the two pages; `nameRequired` has no placeholders and renders with `name: '—'` and an empty `char`. The en strings use double quotes because the RFC token character list contains an apostrophe.

### Layout interactions

- Vertical resizer uses `.resizer-v`; horizontal resizer uses `.resizer-h`; active state adds `.active`.
- XML/JSON layout columns: source, 10px vertical resizer, 232px controls, 14px spacer, result. The grid has a single row.
- Both XML/JSON panels use an `h-11` header bar above the editor: the source panel holds the hidden file input, `Select file`, the current file name/status, and the `Format` file button; the output panel holds share/copy/download.
- Quiet icon-only buttons (`h-8 w-8 rounded-md`, `--text-muted`, `hover:bg-white/5`, `hover:scale-110`, `active:scale-95`) are the shared look for header-bar actions: the XML/JSON `Format` file button and the REST request-body format button. `Format` uses `braces` in JSON mode and `alignLeft` in XML mode, and paints `--accent` while a large-file run is busy, because the button carries no text label to show progress. Keep the localized `common.format` / `common.formatting` strings in `title` and `aria-label`.
- The XML/JSON controls column action buttons are `h-10` rounded accent pills (`Format` and `Escape`); keep both the same height so the column stays balanced.
- URL layout: source textarea, 12px horizontal resizer, centered buttons, result textarea.
- REST raw dumps: the request tab bar is `Headers`/`Query`/`Cookies`/`Raw` and the response tab bar is `Response`/`Headers`/`Raw`. Both `Raw` labels read `rest.rawRequest`/`rest.rawResponse`, which are the word "Raw" in `en` **and** `ru`; the descriptive wording lives in `rest.rawRequestHint`/`rest.rawResponseHint` and is used only in the tab `title` and the editor `aria-label`. The request tab holds the read-only `buildRawRequestPreview` mirror (`rawRequestText`) plus the two cURL buttons; the response tab renders `buildRawResponse(responseStatusLine, responseHeaders, response?.body ?? savedResponseBody)` — never the formatted `responseText` mirror — with status/size and its own copy control. The response toolbar (Save checkbox, format/preview/download) belongs to the `Response` tab only, so the raw tab carries the two counters it needs.
- Compare layout: two equal panes and a fixed 160px center column; it must fill the available height.
- Share layout: header card → server card → one card whose pill row is the tab bar (`Файл`, `Содержимое`, and `Ссылка` only while `result` exists) with the retention select (`#share-retention`) at the right; the tab body swaps in place, so the generated link never sits below the fold and needs no scrolling. The pills carry `aria-pressed` because the active state is otherwise colour-only. Only the active tab's body renders, so the `Select file` and `Upload file` buttons exist on the `Файл` tab alone. The retention select and its hint are hidden on the `Ссылка` tab — retention only applies to the next upload. A successful `shareFilePath`/`shareContent` switches to `Ссылка`; a `$effect` returns the tab to `Файл` when `result` becomes `null` (new file picked, failed upload), otherwise the pill would vanish and leave the card empty.
- Copy feedback lasts 1.5 s. Formatter errors clear after 4.5 s.
- `PropertyEditor.svelte` renders no column-title header row; `nameLabel`/`valueLabel` props are used only in `aria-label`, and placeholders convey the column meaning. Keep the add-row input directly under the container border.
- `PropertyEditor.svelte` takes an optional `fieldHint(field, row, index) => string | null`. A non-null hint paints the cell with `--danger-bg`/`--danger-text`, sets `aria-invalid` and puts the localized reason in `title`. `index` is the row index, or `-1` for the add-row draft; the draft is validated like a row, so the name cell is checked even while it is empty. `commitDraft()` refuses an invalid draft and refuses to create a row when only the value is filled, so a broken name or value never becomes a row; the text stays on screen highlighted for correction.
- `PropertyEditor.svelte` also takes `locked?: (row, index) => boolean` and `lockedTitle?: string`. A locked row renders both cells `readonly` with `tabindex={-1}`, no remove button (its value cell keeps the draft padding so the two columns stay aligned), and `title` reads `lockedTitle` instead of a validation hint. `/wsdl` passes it for the mandatory `Content-Type`/`SOAPAction` rows (`isLockedRow` reads `lockedHeaderNames` case- and space-insensitively, so `soapaction` and `Content - Type` count as locked too); `/rest` does not use it. Locked rows are never validated or persisted — `hintFor` returns `null` for them and `utils/wsdl.ts::editableForSave` drops them on the way to SQLite.
- `PropertyFilesEditor.svelte` is the Form tab's Files table and mirrors `PropertyEditor.svelte` — same `h-10` two-column grid, add-row draft under the container border, hover-revealed `trash` button, `fieldHint` cells. The difference is the value cell: it is a `<button>` that opens `pickPartFile`, shows `file` + the file name, and puts the full path (or the localized reason) in `title`. Buttons have no `aria-invalid`, so invalid picker cells are marked by the danger palette and `title` only. `commitDraft()` refuses to create a row when only a name is typed (`draftNeedsFile` makes the file cell explain itself) and refuses when the name is empty but a file was picked, so the pick is never thrown away silently. `picking` holds the row index while the dialog is open and renders a `…` marker.

### WSDL page and explorer

- `/wsdl` mirrors the REST shell: `flex h-full min-h-0 px-3 pb-3 pt-3`, explorer column (`shrink-0`, `explorerWidth` starts at 280 px) → `.resizer-v` → right column split by `.resizer-h` into request panel (`topHeight`, default 320 px, `grid-template-rows: {topHeight}px 10px minmax(180px, 1fr)`) and response panel. Both resizer `aria-label`s read `wsdl.resizeExplorer` / `wsdl.resizeBody`.
- The browser build renders only the desktop stub (`wsdl.desktopOnly` card with `server` icon) and must not import any Tauri module.
- `WsdlExplorer.svelte` props: `projects`, `files`, `activeProjectId` (`null` renders the `wsdl.noProject` option, which lists every file), `activeOperationId`, `loading`, `importing`, and callbacks `onSelectProject`, `onCreateProject`, `onRenameProject`, `onDeleteProject`, `onImport`, `onCancelImport`, `onSelectOperation`, `onExecuteFile`, `onDeleteFile`.
- Row states read `--row-hover`/`--row-active`; the selected file and operation rows are marked by the inset accent stripe (`shadow-[inset_2px_0_0_var(--accent)]`, narrower than the 3px REST stripe because these rows are indented), not by a tinted background, same as REST list rows.
- `expanded: Record<string, boolean>` holds manual toggles; `isOpen` falls back to the auto-open of the branch containing `activeOperationId`, so a manual collapse of that branch still wins. The chevron rows use the otherwise unused `wsdl.expand`/`wsdl.collapse`/`wsdl.expandOperations`/`wsdl.collapseOperations` keys as `aria-label`, built by `rowLabel(name, key)` in the script — see the pitfall below about `$t()` inside a template literal.
- Only one floating panel is on screen: `openProjectMenu`, `openFileMenu` and `openHelp` call `closeMenus()` first and close themselves when their trigger is clicked again, so the help popover is dismissable by its own `?` button as well as by a click elsewhere or `Escape`.
- The file `...` button renders only for the file that owns the selected operation (`{#if active}`), never on hover; right-click opens the same menu on any row. Deletion confirms in place: `confirmingFileId` turns the menu into the `wsdl.deleteConfirm` line plus `common.cancel` and the delete item — do not use `window.confirm`. Destructive menu items keep the `text-red-500` look already used by `RestExplorer.svelte`. Every item handler runs through `runMenuAction`/`withFileMenu`/`confirmDeleteFile`, which stop propagation, and the window click handler `onWindowClick` ignores a target that is detached or inside `[role="menu"], [role="dialog"]`: a menu item that only *sets* state (the confirm step) must not be undone by the dismiss handler that runs later in the same tick — see the pitfall below.
- Project create/rename replace the select with an input (`Enter` commits, `Escape` cancels, `onblur` commits); rename and delete menu items are `disabled={!activeProject}`.
- The import button swaps to "Cancel" while `importing`, and the `help` button next to it opens an in-app popover (`helpOpen`) holding the localized import explanation. `{@const menu/target}` must be the immediate child of the `{#if …}` that guards it, or Svelte refuses to compile the fixed-position menu.
- Warnings: a file row shows the `warning` icon with the list in `title`, and the page paints a banner over the request panel. Both build their text with `describeWarningText` → `$t(key, { detail })`.
- Request panel header shows operation name, service name, the SOAP version **of the selected endpoint** (`soapVersion` is `versionOf(target, picked)` — one operation can be bound by ports of different versions), the endpoint `<select>` (`endpoint.url`), which takes the whole remaining width of the header row (`min-w-0 flex-1`) while the name/service block stays `max-w-[220px]` and truncates, and the `globe` execute button, which turns into the abort control while a call runs. The header line is labelled with the otherwise unused `wsdl.body` key. The tab bar holds `XML`/`Headers`/`Raw` plus a quiet `format` icon button and a `resetTemplate` button (`disabled` when the operation has no input element). Body edits are saved through a 450 ms debounce that re-arms when `dirty` flips, and only `editableForSave(rows)` reaches `saveWsdlOperationState`.
- The `Headers` tab renders `PropertyEditor` over `headerRows = mergeLockedHeaders(userHeaders, lockedHeadersFor(version, action))`, so mandatory rows come first. `headerIssues` validate only `userHeaders` — merged-table indices differ from the user-row indices, but the messages interpolate `{name}`/`{char}` only, so hints still name the right row. A red dot on the tab carries `validationMessage(headerIssues[0], $t)`, and `blockedMessage(headerIssues, $t)` blocks both `runOperation` and `downloadResponse`. `runOperation` also refuses an empty body when `input_template` is non-empty (`wsdl.emptyBody`); an operation with no input element sends an empty Body on purpose.
- The `Raw` tab is a read-only preview: before the first call `rawRequestText` is `buildSoapRawRequest(envelope, mergeLockedHeaders(...))` for the current endpoint, afterwards the stored `state.raw_request` dump. There are no cURL buttons on this page — SOAP cURL export is out of scope — and the page imports no cURL helpers from `utils/rest.ts`.
- Response panel reuses the `rest.status`/`rest.size`/`rest.time` labels and `formatRestSize`/`formatRestTime`, plus `rest.attachment`/`rest.binary`/`rest.noResponseHeaders`/`rest.download`/`rest.downloaded`/`rest.downloadFailed`, `common.format`/`common.copy`, `formatter.unescape` and `rest.bodyPlaceholder`; do not invent `wsdl.status*` keys. Tabs are `XML`/`Headers`/`Raw`; the body renders only for non-attachment, non-binary responses, otherwise the card shows `response.content_type` and the download button labelled `sharedFileName(attachmentName || 'response.bin')`. `showSaveCheckbox` is deliberately `!isAttachment && !isBinary`, identical to `/rest`, so the checkbox is visible before a response exists. There is no HTML-preview toggle here — previewing markup is a REST-only feature.
- The `Raw` tab of the response panel shows the whole exchange, not bare bytes: `rawResponseDump = buildRawResponse(responseStatusLine, responseHeaders, responseBody)` — the status line, one header per line, the blank line and the body as received — and it is kept in the `rawResponseText` `$state` mirror that a `$effect` re-syncs from the derived dump. `responseStatusLine` is `response?.status_line ?? detail?.state.response_status_line ?? ''`, so a stored answer shows its own first line and a row written before the column existed simply has none (`buildRawResponse` skips a blank line rather than inventing one). The mirror is what `Copy` copies and what `Format` rewrites. When the answer is XML (`rawResponseLanguage` is `xml` exactly when `responseLanguage === 'xml'`), the editor gets `language="xml"` and the toolbar shows `Format` (`canFormatRawResponse` also requires a non-empty body); `formatRawResponse` calls `utils/wsdl.ts::formatRawResponseDump`, which re-indents only the payload and rebuilds the dump around the untouched status line and header block, so the framing never changes. A body without markup throws and answers `formatter.invalid`. Nothing is written back: the stored bytes and the `XML` tab's own `responseText` mirror stay as they were, and every other content type keeps `language="text"` with no `Format` button.
- Response formatting (`transformResponse`) rewrites only the editable mirror (`responseText`), never the stored body: both `Format` and `Unescape` run `formatContent(language, …)` with `doFormat`/`doUnescape` over the received bytes, and the call is refused for a language that is neither `xml` nor `json` (`formatter.invalid` when the text cannot be formatted). A stored body re-read after a restart is therefore never re-formatted. `Copy` copies the mirror of whichever tab is open. `downloadResponse` re-runs the same SOAP call through `downloadRestResponse` — the browser branch is unreachable because the page is desktop-only.
- Saving goes through `saveWsdlResponse`/`clearWsdlResponse`, which write `wsdl_operation_state.response_body`/`response_headers`/`raw_request`/`response_status_line`. `saveWsdlResponse` takes the `RestResponse.status_line` as its fifth argument and sends `null` when it is blank, exactly like `save_rest_response`, so "no status known" is the absence of a first line. `applyDetail` mirrors the stored answer into `response`/`responseHeaders`/`rawResponse` plus `detail.state.response_status_line`, so opening an operation shows the last saved response and its dump without a new call. `toggleSaveResponse(false)` clears the stored answer (status line included) and keeps the current call's answer on screen. `persistResponse` mutates `detail.state` in place, so a body edit cannot overwrite the response and vice versa (`store.rs::response_and_request_do_not_overwrite_each_other` pins the SQL side).

## 5. Persistence, IPC, and Backend

### SQLite

- Database file: `webdev-useful-tools.db` in `app_handle.path().app_data_dir()`.
- Initialized once in `src-tauri/src/lib.rs` via `db::init_schema`.
- Schema is idempotent (`CREATE TABLE IF NOT EXISTS`); there is no migration framework yet, so a new column on an existing table goes through `db.rs::ensure_column` (`PRAGMA table_info` + guarded `ALTER TABLE`), called from `init_schema`. `CREATE TABLE IF NOT EXISTS` never alters an existing table, so without `ensure_column` an installed app would keep the old shape and every read of the new column would fail.
- `AppDb` is a `Mutex<Connection>` managed by Tauri state.

```sql
CREATE TABLE IF NOT EXISTS editor_state (
  tool_key TEXT PRIMARY KEY,
  content TEXT NOT NULL DEFAULT '',
  indent INTEGER NOT NULL DEFAULT 4,
  opt_format INTEGER NOT NULL DEFAULT 1,
  opt_unescape INTEGER NOT NULL DEFAULT 1,
  updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS app_settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS shared_uploads (
  id TEXT PRIMARY KEY,
  filename TEXT NOT NULL,
  content_type TEXT NOT NULL,
  data BLOB NOT NULL,
  size INTEGER NOT NULL,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  expires_at DATETIME
);

CREATE TABLE IF NOT EXISTS rest_projects (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS rest_requests (
  id TEXT PRIMARY KEY,
  project_id TEXT,
  name TEXT NOT NULL DEFAULT '',
  method TEXT NOT NULL DEFAULT 'GET',
  url TEXT NOT NULL DEFAULT '',
  body TEXT NOT NULL DEFAULT '',
  body_type TEXT NOT NULL DEFAULT 'json',
  save_response INTEGER NOT NULL DEFAULT 0,
  response_body TEXT,
  response_headers TEXT NOT NULL DEFAULT '[]',
  raw_request TEXT,
  response_status_line TEXT,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS rest_request_properties (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  request_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  name TEXT NOT NULL DEFAULT '',
  value TEXT NOT NULL DEFAULT '',
  sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS rest_request_form (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  request_id TEXT NOT NULL,
  name TEXT NOT NULL DEFAULT '',
  value TEXT NOT NULL DEFAULT '',
  sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS rest_request_multipart (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  request_id TEXT NOT NULL,
  name TEXT NOT NULL DEFAULT '',
  value TEXT NOT NULL DEFAULT '',
  sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS rest_request_files (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  request_id TEXT NOT NULL,
  name TEXT NOT NULL DEFAULT '',
  path TEXT NOT NULL DEFAULT '',
  filename TEXT NOT NULL DEFAULT '',
  sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS wsdl_projects (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS wsdl_files (
  id TEXT PRIMARY KEY,
  project_id TEXT,
  name TEXT NOT NULL,
  source_url TEXT NOT NULL,
  raw_xml TEXT NOT NULL DEFAULT '',
  parse_warnings TEXT NOT NULL DEFAULT '[]',
  sort_order INTEGER NOT NULL DEFAULT 0,
  imported_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(project_id, source_url)
);

CREATE TABLE IF NOT EXISTS wsdl_services (
  id TEXT PRIMARY KEY,
  file_id TEXT NOT NULL,
  name TEXT NOT NULL,
  sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS wsdl_operations (
  id TEXT PRIMARY KEY,
  service_id TEXT NOT NULL,
  name TEXT NOT NULL,
  soap_action TEXT NOT NULL DEFAULT '',
  input_element TEXT NOT NULL DEFAULT '',
  output_element TEXT NOT NULL DEFAULT '',
  input_template TEXT NOT NULL DEFAULT '',
  sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS wsdl_endpoints (
  id TEXT PRIMARY KEY,
  operation_id TEXT NOT NULL,
  url TEXT NOT NULL,
  soap_version TEXT NOT NULL DEFAULT '1.1',
  binding TEXT NOT NULL DEFAULT '',
  port_name TEXT NOT NULL DEFAULT '',
  sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS wsdl_operation_state (
  operation_id TEXT PRIMARY KEY,
  endpoint_id TEXT,
  request_body TEXT NOT NULL DEFAULT '',
  headers TEXT NOT NULL DEFAULT '[]',
  save_response INTEGER NOT NULL DEFAULT 0,
  response_body TEXT,
  response_headers TEXT NOT NULL DEFAULT '[]',
  raw_request TEXT,
  response_status_line TEXT,
  updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

Rest storage:

- `rest_requests.body` is only the JSON/XML/Text editor buffer. Form-encode rows are never serialized into it.
- `rest_request_form` holds the Form-encode tab table (`name`, `value`, `sort_order`) independently of the text buffer; `write_form` drops rows with a blank name, like header rows.
- `rest_request_multipart` holds the Form tab's **Fields** table and `rest_request_files` its **Files** table (`name`, `path`, `filename`, `sort_order`); `write_multipart` and `write_files` drop blank names too. Only the path and the display name are persisted — never file bytes — and the browser's runtime `File` is stripped by `services/rest.ts::storedFileRow` and by `/rest/+page.svelte::toUpdate`, because `JSON.stringify` would otherwise write `{}` and the row would look loaded.
- Unnamed header, Form-encode and Form (multipart/file) rows are therefore not persisted and vanish after a restart, while unnamed `Query`/`Cookies` values survive in the URL and in the `Cookie` header as `=value`. Both are reported as `name-required`, so nothing disappears without a visible reason.
- `init_schema` migrates legacy `rest_request_properties` rows with `kind = 'form'` into `rest_request_form` and deletes them. Only `kind = 'header'` is written today.
- `rest_requests.response_status_line` holds the status line of the stored answer (`HTTP/1.1 200 OK`) because the Raw response tab is rebuilt from the stored mirror and the version of the exchange is nowhere else. `save_rest_response` writes NULL for an empty line — "no status known" is NULL, not `''` — and `clear_rest_response` NULLs it with the rest of the answer. Databases written before this column gain it through `ensure_column`, so an installed app keeps its saved body and simply rebuilds the dump without a first line (`db.rs::adds_the_status_line_column_to_an_older_database`).
- `wsdl_operation_state.response_status_line` is the same contract on the SOAP side: the WSDL response `Raw` tab rebuilds the exchange from the stored body, headers and this line, and the version of the call is nowhere else. `store.rs::save_response` writes NULL for an empty line (via `optional_status_line`, like `save_rest_response`), `drop_response` NULLs it with the rest of the answer, and `read_state` reads it with `COALESCE(response_status_line, '')`. Databases written before this column gain it through `ensure_column` (`db.rs::adds_the_status_line_column_to_an_older_wsdl_state`), so a saved answer keeps its body and simply shows no first line.
- `rest.rs::resolve_body` (client mirrors: `utils/rest.ts::resolveRequestBody`, `services/rest.ts::wireBody`) gives form rows priority when `body_type = 'form'`: the wire body is `encode_form_urlencoded(form)`, the default content type is `application/x-www-form-urlencoded`, and the text buffer is ignored and left untouched.
- `resolve_body` returns `ResolvedBody { wire: Vec<u8>, dump: String }` and `PreparedCall.body` is `Vec<u8>`, because a multipart body mixes text parts with file bytes. For `body_type = 'multipart'` the wire bytes come from `encode_multipart` (fields first, then files, in table order, boundary from `multipart_boundary()` = a fresh `Uuid::new_v4()`), the dump replaces every file payload with `<bytes of path>` so the raw-request preview keeps the framing without the payload, and `Content-Length` is the real `wire.len()`. Client mirrors: `resolveRequestBody` returns `''` for multipart, `buildMultipartPreview` renders the dump text with the same framing, and the preview `Content-Length` is the dump's own byte length — file sizes are unknowable client-side.
- `rest.rs::prepare_headers` retains and re-adds a user `Content-Type` when `body_type = 'multipart'` so the generated `multipart/form-data; boundary=…` always wins; the browser `fetch` path and `buildCurl` drop the user's content type for the same reason.

REST validation (client-side only, `utils/rest.ts`):

```ts
type PropertyKind = 'header' | 'cookie' | 'query' | 'form' | 'multipart';
type ValidationKind =
  | 'header-name' | 'header-value' | 'cookie-name'
  | 'cookie-value' | 'query-param' | 'form-param'
  | 'multipart-field-name' | 'multipart-field-value'
  | 'multipart-file-name' | 'file-missing' | 'name-required';

interface ValidationIssue { kind; field: 'name' | 'value'; name; char; row?: number }

validateHeaderName(name) / validateHeaderValue(value)
validateCookieName(name) / validateCookieValue(value)
validatePropertyField(kind, field, text)      // single cell, blank text is never an issue
validateRowName(row)                          // value present but name blank ⇒ 'name-required'
validateProperties(kind, rows)                // table, issues carry `row`
validateFileRow(row, index) / validateFileRows(rows)  // multipart file rows
validateQueryUrl(url)                         // raw query segments, issues carry `row`
validateRequest({ url, headers, form, multipart, files, body_type })  // everything that goes on the wire
describeCharacter(character)                  // ␍ ␊ ␉ ␠ or U+XXXX
```

- Header names and cookie names are RFC 9110 §5.6.2 tokens (`TOKEN_PATTERN`); header values reject CR, LF and the rest of the control range (`CONTROL_CHARACTER`), while HTAB and SP stay legal. Non-ASCII bytes are not rejected, matching the `http` crate's obs-text acceptance.
- Cookie values follow RFC 6265 §4.1.1: printable ASCII without DQUOTE, comma, semicolon or backslash. Cookie rows are derived from the `Cookie` header, so they are validated with the cookie rules, not the generic header rules.
- `validateQueryUrl` inspects the **raw** query segments of the URL, not the decoded rows, because the URL field can hold hand-typed text. `buildQuery` percent-encodes every table row, so query rows typed in the table cannot produce an invalid URL and are intentionally not checked cell-by-cell; the row index comes from `querySegments`, which `parseQuery` shares so hint indices line up.
- `%` counts as valid only as a complete `%XX` escape: `firstQueryViolation` consumes the escape with the percent sign, so `hello%20world` passes and `%zz` fails with `char: '%'`.
- To keep a value editable when its name is cleared, `buildQuery` and `buildCookieHeader` emit `=value` for a name-less row (and drop a row with neither name nor value), while `parseCookies` no longer filters name-less pairs. `validateQueryUrl` reports `?=value` as `name-required` — a bare flag like `?debug` and a bare `?=` are legal and unreported.
- Form rows are only checked for control characters (`validateFormText`) because `encodeForm`/`encode_form_urlencoded` percent-encode them on send; spaces, `&` and `=` in form values are intentional. Both encoders skip blank-name rows, which is why a name-less row is reported instead of sent.
- Multipart field names (both tables) are RFC 9110 tokens, because `content-disposition: form-data; name="…"` is written without escaping. Field values are written to the body verbatim, so they reject only the control range (`multipart-field-value`, not the `form-param` "percent-encode it" message): spaces, `&` and `=` are intentional there.
- File rows report `multipart-file-name` for a bad part name and `file-missing` when a row has a `filename` but neither a `path` nor a runtime `File` — that is a persisted row whose bytes were never stored, so the row must not be sent silently. `validateFileRow` takes the index, and `validateFileRows` runs it over the table. Filenames are never validated: `encode_multipart` sanitizes CR/LF and quotes and emits `filename*=UTF-8''<pct-encoded>` alongside a plain `filename` when the name is non-ASCII.
- Rust keeps its own backstop: `encode_multipart` returns an error for a file row with a blank path, so a request that skipped client validation still fails loudly instead of sending an empty part.
- A row with a value but a blank name is `name-required`: `write_headers`/`write_form`/`write_multipart`/`write_files` drop such rows and `parseCookies` used to discard them, so the value would silently never be sent. A row with an empty name **and** empty value is noise and stays unreported, and an empty value with a name is legal. `validatePropertyField` deliberately keeps reporting nothing for blank text so the add-row draft is not flagged while typing; the row-level check belongs to `validateRowName`, which `validatePropertyRow` runs first and `/rest` also applies to the draft inside `hintFor`.
- `utils/validation.ts` is the shared renderer of these issues: `VALIDATION_KEYS` (the `ValidationKind` → i18n key table), `validationMessage`, `propertyFieldHint` and `blockedMessage`, each taking the `$t` function so tests can pin the exact English text (`utils/validation.test.ts`). `/wsdl` uses `validateProperties('header', userHeaders)` on the editable rows only and gates `runOperation` and `downloadResponse` through `blockedMessage`; it never calls `validateRequest`, because a SOAP call has no query/cookie/form tables.
- `validateRequest` collects header rows, cookie rows via `parseCookies`, `validateQueryUrl(url)`, form rows only when `body_type === 'form'`, and — when `body_type === 'multipart'` — the Fields table (`validateProperties('multipart', …)`) plus `validateFileRows(files)`. `/rest` blocks `runRequest`, `copyCurl`, `copyRawCurl` and `downloadResponse` with `rest.validation.blocked` when it returns any issue, paints a red dot on the `Headers`/`Query`/`Cookies`/`Form-encode` tabs, and marks the URL input when the query is invalid. The `Form` tab is a container for two tables, so its dot reads the merged `multipartIssues` while the nested `Fields`/`Files` tabs read their own `multipartFieldIssues`/`multipartFileIssues`.

Editor keys in use:

- `xml`, `json` — content, indent, `opt_format`, `opt_unescape`.
- `url` — content only; other fields use defaults.
- `compare-left`, `compare-right` — content per side; other fields use defaults.

Persistence behavior:

- `saveEditorState` writes `localStorage` immediately, then SQLite when running under Tauri.
- `loadEditorState` prefers SQLite, falls back to `localStorage` when Tauri returns `null` or fails.
- Formatter/URL autosave debounce is 450 ms; Compare uses 500 ms.
- localStorage prefixes: `devtools.editor.`, `devtools.setting.`.
- Theme and locale are stored in `localStorage` only.
- Settings in `app_settings` go through `get_setting`/`set_setting` (`services/db.ts` mirrors them to `localStorage`). Keys in use: `rest.activeProject` and `rest.activeRequest` — the explorer selection restored at startup; `wsdl.activeProject` and `wsdl.activeOperation` — the WSDL explorer selection; an empty string means "no project / no request / no operation selected"; `update.dismissedVersion` — the release version hidden with "Not now" (empty/absent means nothing was dismissed); `share.retention` — the retention the Share page preselects (`none|1d|3d|7d|always`, read through `services/share.ts::loadRetention`, which answers `none` — the default — for a missing or unknown value, so an unreadable setting never stores a payload the user did not ask for).

### Tauri commands

All commands use `#[tauri::command(rename_all = "snake_case")]`. Frontend invokes snake_case argument names.

| Command | Arguments | Returns |
| --- | --- | --- |
| `get_editor_state` | `tool_key` | `EditorState \| null` |
| `save_editor_state` | `tool_key`, `content`, `indent`, `opt_format`, `opt_unescape` | `void` |
| `get_setting` | `key` | `string \| null` |
| `set_setting` | `key`, `value` | `void` |
| `process_large_file` | `input_path`, `output_path`, `mode`, `indent`, `do_format`, `do_unescape`, `do_escape` | `ProcessResult` |
| `save_text_file` | `path`, `content` | `void` |
| `list_share_interfaces` | — | `NetworkInterface[]` |
| `get_share_server_status` | — | `ShareServerStatus` |
| `restart_share_server` | — | `ShareServerStatus` |
| `upload_share_content` | `content`, `filename`, `content_type`, `server`, `retention` | `ShareUploadResult` |
| `upload_share_file` | `path`, `server`, `retention` | `ShareUploadResult` |
| `load_shared_upload` | `server`, `id` | `SharedUploadResponse` |
| `download_shared_upload` | `server`, `id`, `path` | `void` |
| `delete_shared_upload` | `server`, `id` | `void` |
| `consume_pending_deep_link` | — | `DeepLinkTarget \| null` |
| `clear_pending_deep_link` | — | `void` |
| `list_rest_projects` | — | `RestProject[]` |
| `create_rest_project` | `name` | `RestProject` |
| `rename_rest_project` | `id`, `name` | `void` |
| `delete_rest_project` | `id` | `void` |
| `list_rest_requests` | — | `RestRequest[]` |
| `create_rest_request` | `draft` | `RestRequest` |
| `update_rest_request` | `request` | `void` |
| `rename_rest_request` | `id`, `name` | `void` |
| `delete_rest_request` | `id` | `void` |
| `duplicate_rest_request` | `id` | `RestRequest` |
| `save_rest_response` | `id`, `body`, `headers`, `raw_request`, `status_line` | `void` |
| `clear_rest_response` | `id` | `void` |
| `execute_rest_request` | `method`, `url`, `headers`, `body`, `body_type`, `form`, `multipart`, `files` | `RestResponse` |
| `cancel_rest_request` | — | `boolean` |
| `download_rest_response` | `method`, `url`, `headers`, `body`, `body_type`, `form`, `multipart`, `files`, `path` | `u64` |
| `restart_app` | — | `void` |
| `list_wsdl_projects` | — | `WsdlProject[]` |
| `create_wsdl_project` | `name` | `WsdlProject` |
| `rename_wsdl_project` | `id`, `name` | `void` |
| `delete_wsdl_project` | `id` | `void` |
| `list_wsdl_files` | — | `WsdlFileSummary[]` |
| `import_wsdl` | `project_id`, `url` | `WsdlImportResult` |
| `delete_wsdl_file` | `id` | `void` |
| `cancel_wsdl_fetch` | — | `boolean` |
| `get_wsdl_operation` | `operation_id` | `WsdlOperationDetail` |
| `save_wsdl_operation_state` | `operation_id`, `endpoint_id`, `request_body`, `headers`, `save_response` | `void` |
| `save_wsdl_response` | `operation_id`, `body`, `headers`, `raw_request`, `status_line` | `void` |
| `clear_wsdl_response` | `operation_id` | `void` |

`ProcessResult` contains `output_path`, `bytes_in`, `bytes_out`.
`ShareServerStatus` contains `running`, `port`, `error`. `ShareUploadResult` contains `id`, `link`, `server`, `filename`, `size`, `stored` (`false` for a memory-only upload) and `expires_at` (`None` for memory-only or "Always"). `SharedUploadResponse` adds `viewer`, `language`, `content`, `raw_url`, `created_at` and the same `stored`/`expires_at`, so the viewer can say "only in memory" or "available until …". Both upload commands take `retention` as a string and fail with `Unknown retention option` for a code `parse_retention` does not know — the frontend only ever sends the five codes in `RETENTION_OPTIONS`. `delete_shared_upload` goes over HTTP, so it answers `Upload not found` on 404 and `Share server is unavailable: …` when the listener is gone.
`RestRequest`, `RestRequestDraft` and `RestRequestUpdate` carry `multipart: Vec<RestProperty>` and `files: Vec<RestFileRow>`; both fields are `#[serde(default)]` so requests and localStorage records saved before the Form tab still deserialize. `execute_rest_request` and `download_rest_response` take `multipart`/`files` as `Option<…>` and treat `None` as empty, so callers that only send JSON/XML/Text/Form-encode bodies keep working unchanged.
`WsdlFileSummary` carries `id`, `project_id` (`''` = the "no project" bucket), `name`, `source_url`, `warnings`, `services[]` and `imported_at`; `WsdlService` carries `operations[]`, and `WsdlOperation` carries `soap_version`, `soap_action` and `endpoint_count` — no template and no URLs, so the tree stays cheap. `WsdlOperationDetail` adds `input_template`, `input_element`, `output_element`, `endpoints[]` and `state`; `WsdlImportResult` adds `reimported` and `warnings`.

### WSDL import and storage

- Parsing lives only in Rust (`src-tauri/src/wsdl/`), on `quick-xml` with no extra crate. `parser.rs` builds one `XmlDom` arena of elements with resolved namespaces and never matches on prefixes; `xsd.rs` indexes the schema and renders the request template; `store.rs` owns all SQL.
- Limits in `wsdl/mod.rs`: `MAX_WSDL_BYTES = 5 MiB`, `FETCH_TIMEOUT_SECONDS = 30`, `MAX_TEMPLATE_DEPTH = 32`, `MAX_TEMPLATE_ELEMENTS = 2000`, `MAX_WSDL_ELEMENTS = 20 000`, `MAX_NESTING_LEVEL = 512` (`parser.rs`). Only WSDL 1.1 with SOAP 1.1/1.2 bindings is parsed; a WSDL 2.0 root namespace answers `wsdl2:`.
- `fetch_wsdl` shares `rest.rs::normalize_rest_url`, so only `http`/`https` are fetched, and answers prefixed reasons that the frontend maps to i18n keys by prefix (`utils/wsdl.ts::describeErrorText`): `not-url:`, `malformed:`, `not-wsdl:`, `wsdl2:`, `no-operations:`, `cancelled`, `timeout:`, `connect:`, `network:`, `http:`, `too-large:`, `encoding:`. The texts stay English on the wire like the other service messages; an unmapped prefix (e.g. `client:`) is shown verbatim through `wsdl.import.failed`. Fetch failures never write a row.
- `WsdlFetchState` (Tauri state, `wsdl/mod.rs`) cancels an import two ways: `cancel()` fires the `oneshot` of the running fetch, and when nothing runs `cancel_wsdl_fetch` still queues a flag that the *next* `import_wsdl` consumes before touching the network. `cancel.clear()` runs on every exit path, so a stale flag cannot poison a later import. This is a separate channel from `cancel_rest_request`.
- Inheritance is applied in both directions: `xsd.rs::derivation` returns `Derivation { base, attributes, content }` — the type named on `base`, the attributes the derivation declares, and its own group children, collected by `composite(extension_node, context, depth + 1)` — and `complex()` merges them as inherited fields → the derivation's added fields → its attributes. Reading only `base`, as this used to, dropped every element a `complexContent` extension declares over its base, so a derived request wrapper printed the base's requisites without the body element that carries the actual request. `base` is resolved before the derivation's own content, so the element budget reaches the (usually shorter) base first and a truncated template still shows the base fields rather than a deep body. A `base` naming a built-in type goes through `builtin_leaf` rather than `resolve`, which would answer `unknown-type:decimal` for a type the XSD namespace defines. `complex()` keeps `Definition.text`, the value a `simpleContent` extension inherits from its simple base, and `Field.text` carries it to the printer: a `Composite` with only attribute children prints `{text}` inside the tag, so `Currency="text"` and the decimal sample appear together.
- Template comments print through `xsd.rs::note` (a single `{pad}<!-- … -->` line, always `soften`ed, so the `--01` sample of `xsd:gMonth` and an XSD name with `--` cannot break the comment) and `xsd.rs::comment_block` (wraps every line of an optional element or a `choice` branch, keeping that line's own indentation). A note that is already a comment — the optional or cardinality note of a child reached through such a wrapper — goes through `comment_body`, which peels the nested marker pairs and contributes only its text, and a line whose body ends up empty produces no comment at all. One line therefore carries at most one `<!--`/`-->` pair: the old double wrap emitted `<!-- <!-- x --> -->`, and `soften` turned the inner markers into the `<!- - … - ->` garbage. `xsd.rs` tests pin the flattening, the one-comment-per-line rule over a schema whose optional elements nest, the preserved indentation of a commented block, and that the whole template still reads through `parser::build_dom`; `store.rs::writes_the_tree_and_the_request_template` asserts no softened marker reaches SQLite.
- `nillable` contributes nothing to the template: `Field` does not carry it, and no `{tag} is nillable: write xsi:nil="true"…` line is printed. `<!-- optional: tag -->` already says what the element means, and the second note repeated it — do not reintroduce it, and do not read `nillable` back into `Field` unless a note is genuinely wanted.
- Warnings are part of a successful import, not an error: `import-external`, `no-binding:<svc>::<port>`, `no-address:…`, `no-port-type:…`, `no-input:<svc>::<port>::<op>`, `unknown-type:<local>`, `cycle:<local>`, `depth-limit`, `element-limit`. They are stored as a JSON array in `wsdl_files.parse_warnings` and mapped to keys by `utils/wsdl.ts::describeWarningText`.
- Ids are deterministic: `service_id = <file>::<service name>`, `operation_id = <service>::<operation name>`. Re-importing the same URL in the same project keeps the file id and therefore all operation ids and their stored state, and `kept_endpoint_ids` reuses an endpoint id whenever the operation still points at the same URL with the same SOAP version. It reports `reimported: true`; the template column is rewritten, while `request_body`, `headers` and `endpoint_id` are not touched.
- `''` project id is stored as NULL and compared with `project_id IS ?1`, because plain `=` never matches NULL. `UNIQUE(project_id, source_url)` therefore cannot dedupe the no-project bucket — `find_file` is what prevents a duplicate row there.
- `wsdl_operation_state` is created lazily on the first `get_wsdl_operation` of an operation, and `select_operation_detail` re-applies the locked headers on every read, so the stored `headers` array only ever holds user rows plus the recomputed mandatory ones. Locked rows are never persisted as such — `editable_for_save` drops a user row whose name case-insensitively clashes with `Content-Type`/`SOAPAction`, and `merge_locked_headers` writes mandatory rows first.
- `store.rs::locked_headers`, `soap_action_for_call`, `editable_for_save`, `merge_locked_headers` and `locked_header_names` mirror `utils/wsdl.ts` (`lockedHeadersFor`, `soapActionForCall`, `editableForSave`, `mergeLockedHeaders`, `lockedHeaderNames`); change both sides together, as `utils/wsdl.test.ts` and the `store.rs` tests pin the same values.
- Deletes cascade inside one transaction: `remove_project` deletes the project's files and each file's tree and states, `remove_file`/`remove_tree` delete endpoints → operations → services → states. SQLite has no foreign keys here.
- SOAP execution is **not** a WSDL command: `/wsdl/+page.svelte` builds the envelope with `buildEnvelope`, merges the locked headers with `mergeLockedHeaders` and calls `executeRestRequest({ method: 'POST', body_type: 'xml', form: [] })`; abort uses `cancelRestRequest`. A non-2xx answer (a `soap:Fault` with HTTP 500) is returned by `rest.rs` as a response, not an error.
- `services/wsdl.ts` is desktop-only: every call throws `WSDL support is available in the desktop app only` when `isWsdlSupported()` (`isTauriRuntime()`) is false, so the browser bundle never invokes these commands and `/wsdl` renders the desktop stub. Selection is persisted with `get_setting`/`set_setting` under `wsdl.activeProject` and `wsdl.activeOperation`; the pure `resolveWsdlSelection` looks the stored operation up across all visible files, so an operation that moved to another project reopens the project that owns it.

### Tolerant XML/JSON formatting

Invalid markup and invalid JSON are formatted, not rejected. Both layers run the strict path first and fall back to a byte-preserving re-indent, and the client and Rust outputs are kept identical:

```ts
// src/lib/utils/formatters.ts
type XmlVirtual = { kind: 'text'; text: string; verbatim?: boolean }
	| { kind: 'raw'; text: string; block: boolean }
	| { kind: 'element'; name: string; attrs: string; children: XmlVirtual[] };
toVirtualNode(node: Node): XmlVirtual | null;   // strict DOM → virtual tree
serializeVirtual(node, depth, pad, block): string; // one serializer for both paths
tokenizeXml(markup): XmlToken[];                // never validates, never throws
buildTolerantTree(tokens): XmlVirtual[];
prettyJsonTolerant(source, indent): string;     // structural JSON pretty printer
```

```rust
// src-tauri/src/processor.rs
format_xml_tolerant(source: &str, indent: usize) -> Option<String>;
format_json_tolerant(source: &str, indent: usize) -> Option<String>;
json_shape_score(source: &str) -> usize;                          // mirrors jsonShapeScore
json_tolerant_candidate(candidates: &[String]) -> Option<&String>; // mirrors jsonTolerantCandidate
```

- Strict XML is `DOMParser`; when it reports `parsererror`, every candidate (`source`, plus `decodeXmlEntities(source)` when unescaping is on) is tokenized and the one with the most markup tokens becomes the tolerant tree. `buildTolerantTree` drops unmatched closing tags from the stack (unclosed elements close where their parent does, so they serialize as `<a/>`) and keeps a stray `</x>` as a literal content line.
- Tolerant text nodes carry `verbatim: true` and are emitted trimmed but **not** re-escaped: the strict path still escapes `'` to `&apos;`, the tolerant one must not double-escape `&lt;`/`&amp;` that are already entity text in the source.
- Tolerant JSON runs on the body after any loose content: `splitJsonJunk` (client) and `split_json_junk` (Rust) put junk before the first `{`/`[` on its own line and format the body strictly when it parses; junk containing `<` is refused so HTML bodies still report an error, and junk plus a bracket body with no quoted string is refused too, so prose like `Error occurred in {module}` is not treated as JSON. The body itself is scanned by `prettyJsonTolerant`/`format_json_tolerant`: strings are copied verbatim (including their escapes), `{[`/`}`/`]`/`,` break lines with depth, `:` becomes `: `, collapsed whitespace outside strings becomes one space, real newlines start a new line, and nothing is invented for unclosed brackets. `jsonShapeScore`/`jsonTolerantCandidate` (client) and `json_shape_score`/`json_tolerant_candidate` (Rust) pick the unescape candidate with the most structural characters as the layout source, and the strict pass runs over every candidate before any tolerant layout. Both sides use the same order and the same `>=` tie rule, so the browser and `process_large_file` produce identical output whenever the candidates decode the same way; see the pitfall below for the malformed-entity difference.
- Both paths still throw when there is no structure at all — plain `hello world`, or an HTML body in JSON mode — so `formatter.invalid` and the REST format/download guards keep working for content that genuinely cannot be formatted.
- `write_xml_pretty` validates the document shape while streaming (one root, no non-whitespace text at depth 0, balanced start/end names) because `quick-xml` accepts junk before the declaration and multiple roots that `DOMParser` rejects. Rejecting there is what routes both `format_xml_content` and `stream_xml_pretty` to the tolerant path.

### File processing

- `process_large_file` is async and runs `process_file` in `spawn_blocking`.
- `write_temp` writes a temporary `.<name>.<pid>.tmp` next to the destination, then copies it to the final path and deletes the temp file.
- XML: streaming `Reader`/`Writer::new_with_indent`, preserving declarations; text is trimmed and escaped. Whitespace-only text nodes are dropped. A document that fails the shape check (junk outside the root, a second root, an unclosed or unmatched tag) is re-read and re-indented by `format_xml_tolerant`.
- JSON: parsed into `serde_json::Value` and pretty-serialized with a custom 2/3/4-space `PrettyFormatter`; when parsing fails and the text looks like JSON, `format_json_tolerant` re-indents it instead.
- Fast streaming paths are used only when `do_format = true` and `do_unescape = false`. With unescaping enabled, or when streaming cannot parse the file, the whole file is read into memory and formatted through `process_content`.
- Escape mode reads the whole file and writes escaped content; the XML/JSON formatter UI currently passes `do_escape: false`.
- Input with no markup and no JSON structure returns a user-facing error string; temp files are cleaned up. Malformed markup/JSON is no longer an error — see “Tolerant XML/JSON formatting”.
- The frontend defaults the output path to `<name>.formatted.<ext>`.
- Browser mode has no backend file processing: selecting a file loads it into the left editor and formatting stays client-side.

### Share backend

- `http_server::start(store)` binds `0.0.0.0:3030` once at app setup and stores `ShareServerState` in Tauri state. `lib.rs` builds `share::ShareStore::new(database.clone())`, manages it, and hands a clone to the router, so the command path and the HTTP path read the same memory map and the same connection. `restart_share_server` stops the old listener, rebinds (which purges expired rows again), and returns the fresh status.
- The router is not read-only anymore: `GET /health`, `GET /api/uploads/{uuid}`, `GET /api/uploads/{uuid}/raw` and `DELETE /api/uploads/{uuid}` — 204 when a row or a memory entry was dropped, 404 for an unknown or malformed id, 500 on a database error. Creating uploads still happens only through Tauri commands; there is no POST route.
- There is no token anywhere: `build_deep_link`/`parse_deep_link` carry only `server` and `id`, so the link is the whole credential and whoever holds it may also delete the payload (README «Share»). `parse_deep_link` ignores unknown query keys, so a stale link with a `token=` param still opens. Do not reintroduce a secret without redesigning the delete path and the viewer.
- Retention lives in `share.rs`: `Retention { Memory, Days(u32), Always }` and `parse_retention` for the codes `none | 1d | 3d | 7d | always`. `Memory` never touches SQLite — the payload goes into `MemoryUploads` (`MAX_MEMORY_UPLOADS = 24`, evicted by a monotonic sequence, oldest first) with `stored: false` and `created_at` from `db::now_timestamp`, so its link dies with the app. `Days(n)` stores `expires_at` from `db::expiry_timestamp` (`datetime('now', '+N days')`), `Always` stores NULL.
- Expiry is computed in SQL, not in Rust — no chrono dependency. `db::read_shared_upload` filters `expires_at <= datetime('now')` on read, and `db::purge_expired_uploads` runs from `insert_shared_upload`, `ShareStore::find` and `bind_and_serve`. There is no timer; a row that ran out simply stops answering requests and is dropped by the next read, write or server start.
- `ShareStore { db, memory }` is the router state: `find` prefers a memory entry over a row, `delete` removes a memory entry or a row and answers `false` when neither existed, `purge_expired` keeps the database clean. `db::delete_shared_upload` deliberately has no expiry filter, so deleting a row that is already past its expiry still succeeds and erases the row.
- Frontend retention: `services/share.ts` owns `ShareRetention`, `RETENTION_OPTIONS`, `DEFAULT_RETENTION` (`'none'`: an upload is memory-only until the user picks a lifetime, and `normalizeRetention` folds a missing or unknown value back to it), and `loadRetention`/`saveRetention` on setting key `share.retention`; `/share` preselects the stored value (the select is built in the script, never with `$t()` inside a template literal) and passes the code to both upload commands. A successful upload switches to the `Ссылка` tab, which is rendered only while `result` exists and holds the link field, the copy button and the retention/expiry/memory hint; the link block must stay inside that tab, not below the cards, because below the cards it fell out of view behind the fold. `/view` deletes through `deleteSharedUpload` with an in-place two-step confirm (`confirmingDelete`) and then shows the `deleted` state — no `window.confirm` and no floating `role="menu"`, which `npm run check` reports as an a11y warning.
- `normalize_server` accepts only `http://` with port `3030` and loopback or RFC 1918/LAN IPv4 hosts; `normalize_id` accepts only UUID v4. Public or wrong-port hosts are rejected, so deep links cannot point at arbitrary servers.
- Upload limit is 50 MB for both file and content paths; file uploads read the whole file in `spawn_blocking`.
- `build_deep_link`/`parse_deep_link` use scheme `webdev-useful-tools://view?server=...&id=...`; `install_deep_link_handler` stores the target in `PendingDeepLink` and emits `share:deep-link`. The frontend consumes the pending target on mount and navigates to `/view` through `buildViewerUrl`.
- Frontend share pages must gate uploads on `ShareServerStatus.running`; the Share UI shows a Retry button that calls `restart_share_server`.
- `reqwest` is used with `default-features = false, features = ["json", "rustls", "gzip"]`. REST requests support HTTP and HTTPS, automatic gzip decompression, a 120-second timeout, and up to 10 redirects. `src-tauri/tests/http_client.rs` still guards the plain-HTTP client path.

### Updater (desktop)

- `tauri.conf.json > plugins.updater` holds the minisign `pubkey` inline, the single endpoint `https://github.com/DimetriusJonson/dev_tools_svelte/releases/latest/download/latest.json`, `requireSignedVersion: true`, and `windows.installMode: "passive"` (valid: `passive | quiet | basicUi`). `bundle.createUpdaterArtifacts: true` makes `tauri build` emit `<artifact>.sig` and `latest.json`; the private key lives in `~/.tauri/webdev-useful-tools.key` (no password) and is never committed — builds need `TAURI_SIGNING_PRIVATE_KEY`.
- `tauri_plugin_updater::Builder::new().build()` is registered in `lib.rs` (the builder reads the config above through `api.config()`); `build()` is the only method — there is no `init()`. Capability `updater:default` is in `capabilities/default.json`; `tauri-plugin-process` is deliberately not a dependency, restart goes through `update.rs::restart_app`, which calls `AppHandle::request_restart()` so `RunEvent::ExitRequested`/`Exit` still run and stop the share server. `#[tauri::command]` items must live in a submodule and be listed as `update::restart_app`: defining a `pub` command in the crate root and listing it bare in `generate_handler!` collides on `__cmd__*` macros.
- `src/lib/services/updater.ts` wraps the plugin (`check` with a 20 s timeout, `downloadAndInstall` with 300 s, `close`) and invokes `restart_app`; it is a desktop-only service (`isUpdaterSupported()` is `isTauriRuntime()`), so the browser build never imports the plugin.
- `src/lib/stores/update.ts` owns the flow shared by the navbar button and `UpdateBanner.svelte`: one `checkForUpdates({ manual })` feeds both, re-entry is refused while `checking | downloading | installing` and while a finished update is waiting in `ready`. The announced `Update` handle is kept in a module variable and released with `closeUpdate()` whenever the banner is dismissed or replaced — dropping it leaks a temp download.
- Startup checks are silent (`STARTUP_UPDATE_DELAY_MS = 1200` after hydration): they only raise the banner and log to the console. Manual checks always answer with a toast. `classifyUpdateFailure` groups plugin text into `unavailable` (no endpoints, non-HTTPS endpoint, unsupported OS/arch) → error toast, `notFound` (release JSON unfetchable, or no entry for this platform/fallback platform) → info toast with no banner, and `failed` → red banner with the reason. Match the plugin's own wording from `plugins/updater/src/error.rs`: `Could not fetch a valid release JSON from the remote`, `the platform \`…\` was not found in the response \`platforms\` object`, `None of the fallback platforms … were found in the response \`platforms\` object`.
- `installUpdate()` never calls `restartApplication()`: the installer owns relaunch. On Windows `downloadAndInstall` exits the process before launching `msiexec /passive`, so the promise never resolves and the banner stays on "installing" with the Windows hint. Outside Windows the bundle is replaced in place and the banner's `ready` phase offers a manual Restart button that calls `restart_app`.
- "Not now" persists exactly the announced version in `app_settings.update.dismissedVersion`; `isDismissedVersion` is exact-match only, so a newer release is reported again. A manual check ignores the dismissal.
- `.github/workflows/release.yml` is what puts `latest.json` behind that endpoint. A `v<version>` tag push runs `preflight` (frontend tests, `svelte-check`, `cargo test`, the three manifests declaring one version, tag == app version, `TAURI_SIGNING_PRIVATE_KEY` non-empty) and then four `tauri-apps/tauri-action@v1` jobs into one draft release with `tagName: v__VERSION__`. `strategy.max-parallel: 1` is load-bearing: the action reads the `latest.json` already attached to the release and merges its own platform entry, so two jobs uploading it concurrently overwrite each other and clients on the lost OS answer "not found". Do not parallelize this matrix or switch it to a plain `os` matrix.
- Bundle targets are pinned per job instead of inheriting `targets: "all"`: `deb`+`appimage` on Linux (GitHub's Ubuntu runner has no `rpmbuild`, so the rpm target would fail the build), `msi` only on Windows (so the manifest points at the MSI that `plugins.updater.windows.installMode: "passive"` hands to `msiexec`, and `updaterJsonPreferNsis: false` cannot pick a nonexistent NSIS), `app`+`dmg` with `macOS.signingIdentity: "-"` on both Apple targets (there is no Apple certificate; arm64 refuses to run an unsigned bundle at all). `args` carries `--config` as single-quoted JSON because the action shell-tokenizes `args` (`string-argv`) and would drop bare double quotes; the token reaches `tauri build` through `execa` as one argv item.
- The action only writes `latest.json` when a `.sig` exists — otherwise it logs "Signature not found for the updater JSON. Skipping upload...", i.e. a missing secret yields a release that looks fine and updates nothing. That is why the key is asserted in `preflight` rather than discovered at upload time. `releaseBody` doubles as `notes` in the manifest, so it is the text `UpdateBanner` shows and must stay written for humans, not as an i18n key.
- Builds upload to a **draft** so a release never becomes "latest" while only some platforms are on it. `manifest` then asserts the version equals the tag, all four entries (`windows-x86_64`, `linux-x86_64`, `darwin-x86_64`, `darwin-aarch64`) have a signature and an `api.github.com` asset URL, and the assets themselves exist (`.msi`, `.AppImage`, `.deb`, two `.app.tar.gz`, two `.dmg`, at least four `.sig`), and exports the release id as a job output. `publish` patches that id twice — `draft=false`, then `make_latest=true` — and re-reads `releases/latest/download/latest.json` without credentials to prove the endpoint answers. Dropping either job leaves the app permanently on "no update found"; re-running an already published tag fails on purpose because `releaseDraft: true` refuses a published release with the same tag.

### Desktop integration

- `tauri-plugin-single-instance` (with the `deep-link` feature) focuses and unminimizes `main` when a second instance starts.
- `tauri-plugin-deep-link` registers `webdev-useful-tools://` from `tauri.conf.json > plugins.deep-link.desktop`; the frontend does not call its JS API, so no extra capability entry is required.
- Tray menu IDs `open` and `quit` map to `TrayAction::Open` / `Quit`; unknown IDs are ignored.
- Closing the `main` window prevents close and hides it to the tray. Quit exits the process.
- Window title `<localized app.title> <version>` from `package.json`, set by the frontend via `setTitle`; `tauri.conf.json > app > windows > title` is only the initial value. 1440×920, minimum 1100×700.
- Capabilities in `src-tauri/capabilities/default.json`: `core:default`, `core:window:allow-set-title`, `dialog:default`, `dialog:allow-open`, `dialog:allow-save`, `clipboard-manager:allow-read-text`, `updater:default`.
- Clipboard reads go through `tauri-plugin-clipboard-manager` in Tauri. Do not call `navigator.clipboard.readText()` directly from pages: WebView2/WKWebView reject it with a permissions error. In browser mode `utils/clipboard.ts` falls back to `navigator.clipboard` and requires a secure context.
- Custom icon source is `src-tauri/icon-source.png`; SVG companion is `src-tauri/icons/app-icon.svg`. Regenerate the bundle with `npx tauri icon src-tauri/icon-source.png` when the icon changes.

## 6. Feature Implementation Map

- **XML/JSON formatter:** `src/routes/{xml,json}/+page.svelte` → `XmlJsonFormatter.svelte` → `formatters.ts`. Client-side format/escape; file mode uses `processLargeFile`. Both `formatXmlText` and `formatJsonText` fall back to the tolerant re-indent path (`tokenizeXml`/`buildTolerantTree`/`prettyJsonTolerant`) when the strict parse fails, so the `formatter.invalid` toast only appears for text with no markup or no JSON structure.
- **URL:** `src/routes/url/+page.svelte` → `encodeUrl`/`decodeUrl` in `formatters.ts`. Uses `encodeURI`/`decodeURI`, not `encodeURIComponent`.
- **Compare:** `src/routes/compare/+page.svelte` → `diffTexts`/`lineOffsets`/`splitLines` in `diff.ts`; highlights are applied through `CodeMirrorEditor`. Highlight `line` numbers are 1-based in `splitLines`' normalized space and reach the editor through `lineOffsets` (see the pitfall below).
- **Share (desktop):** `src/routes/share/+page.svelte` (interface pick, `file`/`content`/`link` tabs, retention select) + `src/routes/view/+page.svelte` (read-only viewer with the delete control) → `services/share.ts` (`buildViewerUrl`, `deleteSharedUpload`, retention helpers) → `share.rs` (`ShareStore`, retention, memory uploads, deep link) + `http_server.rs` + `db.rs::shared_uploads`. Draft content arrives from `XmlJsonFormatter.svelte` through the `draftShare` store (`readOnly: true`). Tests: `services/share.test.ts` (byte formatting, viewer URL shape with no secret, retention normalization and the settings round trip, the desktop-only delete guard), `src/routes/share/page.test.ts` (the `Ссылка` tab appears and activates on a successful upload, hides when `result` clears, the retention select disappears with it, and the browser stub shows neither upload controls nor the start-up line — see the pitfall below about the `until` helper) and the Rust tests in `share.rs` (deep link, retention parsing, memory eviction, `stored`/`expires_at` in the viewer response), `db.rs` (round trip, legacy migration, retention/purge, delete idempotence, delete of a hidden expired row), `http_server.rs` (GET/raw/404s, delete once, malformed id, memory serve + delete) and `tests/http_client.rs` (plain-HTTP round trip). Behavior and acceptance criteria: README section «Share».
- **Rest client:** `src/routes/rest/+page.svelte` → `RestExplorer.svelte` + `PropertyEditor.svelte` + `PropertyFilesEditor.svelte` → `services/rest.ts` → `utils/rest.ts` → `rest.rs`. Projects/requests persist in `rest_projects`, `rest_requests`, `rest_request_properties` (headers), `rest_request_form` (Form-encode rows), `rest_request_multipart` (Form tab fields) and `rest_request_files` (Form tab file rows, path + display name only). Execution goes through the Rust backend in Tauri; browser mode falls back to `fetch` + `FormData` and `localStorage`. cURL shell quoting (`tokenizeCurl`), imported-body de-escaping (`unescapeImportedBody`), form-part parsing (`splitFormParameters`) and export quoting (`buildCurl`, `multipartCurlParts`, `quotePosix`, `quoteWindows`) live in `utils/rest.ts` and are covered by `utils/rest.test.ts`. Multipart is assembled client-side by `resolveRequestBody`/`buildMultipartPreview`/`buildMultipartFormData` and server-side by `rest.rs::encode_multipart` + `multipart_dump`, both from `multipart_boundary()`; file picking goes through `services/files.ts::pickPartFile`. RFC validation lives in `utils/rest.ts` (`validateRequest`, `validateProperties`, `validateRowName`, `validateFileRows`, `validateQueryUrl`); the page maps `ValidationKind` to the `rest.validation.*` keys, passes `fieldHint` to every table (`hintFor`/`fileHint`), and blocks send/cURL/download when issues exist. Explorer selection persistence lives in `services/rest.ts` (`loadRestSelection`, `saveRestSelection`, pure `resolveRestSelection`, covered by `services/rest.test.ts`): `+page.svelte` restores it in `onMount` and writes it from an `$effect` that is gated on `hydrated` and on a non-reactive `savedSelection` key, so the restore does not rewrite identical values. The response editor takes `responseLanguage` directly, so an HTML-flavoured answer (`text/html`, `application/xhtml+xml`, …) is highlighted by the HTML language, while both `Raw` tabs keep `language="text"`.
- **WSDL client (desktop):** `src/routes/wsdl/+page.svelte` → `WsdlExplorer.svelte` → `services/wsdl.ts` → `utils/wsdl.ts` → `src-tauri/src/wsdl/{mod,parser,xsd,store}.rs`. Import reads the URL from the clipboard (`utils/clipboard.ts`) and runs `import_wsdl`; parsing, the XSD index and the request template are Rust-only, execution reuses `execute_rest_request` (`body_type: 'xml'`, `POST`) and `cancel_rest_request`, and abort/failure toasts reuse `rest.aborted` / `rest.requestFailed`. Projects, files, services, operations, endpoints and per-operation state persist in `wsdl_projects`, `wsdl_files`, `wsdl_services`, `wsdl_operations`, `wsdl_endpoints` and `wsdl_operation_state`; explorer selection goes through `app_settings.wsdl.activeProject`/`wsdl.activeOperation` with the pure `resolveWsdlSelection` (`services/wsdl.test.ts`). The SOAP envelope, the locked headers, the raw-request preview, the endpoint choice and the import error/warning code tables live in `utils/wsdl.ts` (`buildEnvelope`, `buildSoapRawRequest`, `lockedHeadersFor`, `mergeLockedHeaders`, `editableForSave`, `normalizeHeaderRows`, `lockedHeaderNames`, `selectEndpoint`, `describeErrorText`, `describeWarningText`) and are mirrored in `store.rs`/`mod.rs`. Header cell hints, the tab error dot and the blocked-send message come from the shared `utils/validation.ts`, the response size/time formatting and the attachment/binary wording from `utils/rest.ts` (`formatRestSize`, `formatRestTime`, `filenameFromDisposition`), and the download file name is cleaned by `services/files.ts::sharedFileName`. Response persistence goes through `save_wsdl_response`/`clear_wsdl_response`, the response editor takes the detected content language, and the response `Raw` tab rebuilds the whole exchange (`utils/wsdl.ts::formatRawResponseDump` powers its `Format`), so an HTML-flavoured answer is highlighted by the HTML language, an XML one gets the XML language plus `Format` on `Raw`, and any other type stays plain text. Behavior and acceptance criteria: README section «WSDL-клиент». Out of scope: cURL export of a SOAP call, the HTML preview of a response, WSDL 2.0 and non-SOAP bindings, and `xsd:import`/`xsd:include` fetching.
- **Updates (desktop):** `src/routes/+layout.svelte` (startup check + banner slot) + `Navbar.svelte` (manual check) → `UpdateBanner.svelte` → `stores/update.ts` → `services/updater.ts` → `utils/update.ts` → `tauri-plugin-updater` + `src-tauri/src/update.rs`. Dismissal persists through `set_setting`/`get_setting` (`app_settings.update.dismissedVersion`). Pure helpers (percent, dismissal match, failure classification, Windows UA) are covered by `utils/update.test.ts`. Behavior and acceptance criteria: README section «Обновления».
- **Notifications:** `stores/toast.ts` → `Toaster.svelte` in `+layout.svelte`. `utils/clipboard.ts` emits success/failure notifications; pages report errors through `notify`/`notifyError`. A failure whose message is empty falls back to the localized `common.error`, so `notifyError` never shows a blank toast; the stack keeps only the newest `MAX_VISIBLE` items (`stores/toast.test.ts` pins the cap, the message precedence and that fallback per locale). Toast text is centered: the message span is `flex-1` with symmetric `px-7`, and the close button is absolutely positioned at `right-2 top-1/2`, so long messages wrap without colliding with it.
- **Theme/i18n:** `theme.ts`, `i18n.ts`, `Navbar.svelte`, `app.html`.
- **Loading splash:** `src/app.html` markup + inline CSS + pre-hydration label script → `utils/splash.ts` → `+layout.svelte` `onMount`. Tests in `utils/splash.test.ts`. No new storage keys: it reads `devtools.theme` and `devtools.locale`.
- **Persistence:** `services/db.ts` → `db.rs` commands; browser fallback is localStorage.
- **Large files:** `services/files.ts` + `services/db.ts` → `processor.rs`.

## 7. Known Pitfalls & Invariants

- Use `serde_json`'s `preserve_order` feature to keep JSON key order stable.
- XML text must be escaped after entity decoding; `quick-xml` `BytesText::from_escaped` expects already-escaped text. Existing tests assert `&apos;` output. That rule applies to the strict path only: the tolerant re-indent path emits source text verbatim, because re-escaping text that already contains entities double-escapes it.
- Malformed markup and malformed JSON are formatted, not rejected (`formatters.ts`, `processor.rs`). Keep the strict path first, keep the tolerant path free of invented closers or brackets, and keep the client and Rust implementations in step; do not widen the "no structure at all" throw into a generic catch-all success, or plain-text REST responses would stop reporting errors.
- Tolerant JSON candidate selection must stay mirrored in `formatters.ts` and `processor.rs`: every candidate gets the strict pass first, then the best-scoring candidate becomes the layout source, with `>=` so a later tie wins. The case that pins the rule is `formatJsonText('{&quot;a}', 2, true)` -> `{\n  "a}` (and the same input through `process_content`): the decoded candidate must win over the entity-text source. `formatters.test.ts` and `processor.rs` test both sides, so change the two together.
- Numeric entities and `\u` escapes share the `isValidCodePoint` guard in `utils/formatters.ts`: a code point outside `0..0x10FFFF` or inside the surrogate range is kept as its own text (`&#xD800;`, `&#x110000;`, `\uD800`) instead of producing a lone surrogate that the strict XML/JSON pass would later re-escape; `decodeAnsiCEscape` in `utils/rest.ts` follows the same rule for bash `$'…'` imports, so `\x48` (one or more hex digits, bash-style) and a lone-surrogate `\u` escape stay literal. Keep this guard rather than wrapping the conversions in a try/catch.
- Entity decoding is not identical for malformed entities: `quick-xml` `escape::unescape` refuses the whole string, so `decode_entities` returns the source unchanged, while `decodeXmlEntities` in the browser decodes entity by entity and leaves unknown names as text. For `{&quot;&bad;}` with unescape on, the browser re-indents the decoded candidate `{"&bad;}` while `process_content` re-indents the undecoded source, so full parity only holds for input whose entities all decode identically. Keep candidate-selection tests on decodable entities so they test the scoring rule, not this decode difference.
- `quick-xml` `GeneralRef` events need explicit handling; do not silently drop character references.
- URL encoding must use `encodeURI`/`decodeURI` to preserve URL structure. Invalid decode input should return the original string.
- Compare LCS is capped at 4,000,000 cells and inline token diff at 250,000 cells. Over the cap, fall back to prefix/suffix or whole-line detection; do not allocate unbounded DP tables. The cheap LCS path must still keep the common prefix and suffix unhighlighted — `diffTexts(tallLeft, tallRight)` in `diff.test.ts` pins the prefix/suffix index arithmetic.
- `splitLines` normalizes CRLF and lone CR into LF lines, so `lineOffsets` must report a start for **every** normalized line (`\r\n` is one break two characters wide, a lone `\r` is its own break) and the diff's 1-based line numbers live in that normalized space. The two are the pair the Compare page uses to turn highlights into CodeMirror offsets; if `lineOffsets` splits only at `\n`, a CR-only document has no offset for line 2 and the `?? 0` fallback in `buildHighlights` paints the highlight on the wrong line. `diff.test.ts` asserts `lineOffsets(text).length === splitLines(text).length` over mixed endings.
- `tokenDiff` chooses one tokenizer for the whole changed pair through `looksLikeMarkup` (a real `<…>` pair, not any `<` or `>`), because per-line heuristics made the two sides of one changed line highlight differently. `wholeLine` returns no range for an empty line: the Compare page skips `end <= start` ranges, so a zero-width range is invisible and only keeps the line decoration.
- Compare clears highlights when either side changes and on swap.
- `CodeMirrorEditor` must destroy its view on unmount and reconfigure compartments on prop changes.
- Vitest's jsdom environment never replaces a `globalThis` key Node already defines (`getWindowKeys` keeps an existing key that is not in its own list), and Node ≥ 25 defines `localStorage` as a lazy accessor answering `undefined` without `--localstorage-file`. That combination makes `window.localStorage` undefined and kills every storage-touching test file — it is what failed the release preflight when `lts/*` moved the runner to Node 26. `src/lib/test-setup.ts` (wired through `test.setupFiles`, preferring the environment's own jsdom window and falling back to `MemoryStorage`) is the only thing keeping the suite environment-independent: keep it, and do not work around the shadowing by passing `--localstorage-file` or by reading `localStorage` through some other global.
- The loading splash lives in `src/app.html`, not in a component, because it must paint before the bundle exists. Keep its CSS inline there with `var(--token, fallback)` values, keep the markup a sibling of the `%sveltekit.body%` container, and dismiss it from `+layout.svelte` `onMount` through `utils/splash.ts`. Its label strings are the documented exception to the "no hardcoded user-facing strings" rule: they mirror `app.title` and the loading text, and the layout overwrites the title from the dictionary, so update both places when the app name changes.
- Do not add `tauri-plugin-sql`; use the existing `rusqlite` backend.
- Do not add external icon packs or copy icon markup into pages; extend `ICON_PATHS`.
- Do not put theme/locale into SQLite without updating this document and the persistence contract.
- Share uploads and the viewer require Tauri; the browser build shows the desktop-only notice instead of the share UI.
- Never upload from the Share UI when `get_share_server_status.running` is false; deep links to a stopped server are dead. Use Retry (`restart_share_server`) instead of allowing the upload.
- Keep the generated link on the `Ссылка` tab of the upload card, not in a section below it: below the cards it sat behind the fold and the user had to scroll to copy it. The pill is rendered only `{#if result}`, so `mode === 'link'` with no result would paint an empty card — the `$effect` that resets `mode` to `'file'` when `result` clears is what prevents that, and removing it leaves the page blank after a new file is picked.
- `src/routes/share/page.test.ts` mounts the page directly (`mount(SharePage, { target: document.body })` — Svelte 5's `mount` requires a `target`) and turns it into the desktop build by stubbing `window.__TAURI_INTERNALS__`, which `isTauriRuntime()` tests and the Tauri API forwards every command to. It must also mock `@tauri-apps/api/webview`, because `onMount` imports it dynamically and `getCurrentWebview()` reads window metadata the stub does not provide. `Upload file` is `disabled` until `list_share_interfaces` and `get_share_server_status` resolve, so waiting for the button to *exist* is not enough — a click there is a no-op and the upload command never fires; the helper waits for `disabled === false` instead. The dialog stub answers a fixed path every time, so a second pick is asserted by counting `plugin:dialog|open` calls rather than by `toContain`. Assertions on IPC-driven DOM use the local `until` poll (it rethrows the last assertion error with the command list) instead of `vi.waitFor`, which gave the same failures without naming the missing command.
- Share deep links must keep `normalize_server` LAN-only and pinned to port 3030; do not widen it to public hosts.
- Share has no credential other than the id: `DELETE /api/uploads/{id}` is open to whoever can reach the port, which is the product decision behind the viewer's delete button (README «Share»). Do not "fix" it by adding a token to the link without also changing `build_deep_link`/`parse_deep_link`, `buildViewerUrl`, the viewer UI and the README contract.
- Keep retention parsing strict: `parse_retention` knows exactly the five codes in `services/share.ts::RETENTION_OPTIONS`, and an unknown one is an error rather than a silent fallback to a stored lifetime — the client's own default is `none`, so a value nobody chose never becomes a database row. Adding an option means changing both lists and both `en`/`ru` labels (`share.retention.<code>`).
- Memory-only uploads are process-scoped and capped at `MAX_MEMORY_UPLOADS`, evicting the oldest: their link dies with the app and can disappear while the app still runs. Never write a `Retention::Memory` payload to `shared_uploads`, and keep `stored: false` so the UI and the viewer say "only in memory" instead of promising a date.
- Compute expiry only through SQLite (`expiry_timestamp`/`now_timestamp`, `expires_at <= datetime('now')`). Do not add chrono or compare Rust strings against `CURRENT_TIMESTAMP` by hand: the two formats must stay identical or the purge silently keeps or drops the wrong rows.
- `read_shared_upload` hides expired rows but does not delete them; only `purge_expired_uploads` (from `insert_shared_upload`, `ShareStore::find` and `bind_and_serve`) erases them, and `db::delete_shared_upload` has no expiry filter so an explicitly deleted row always goes. Do not add `AND expires_at > now` to the delete — it would answer 404 while leaving the payload in the database.
- The viewer's delete is a two-step in-place confirm (`confirmingDelete`), like the WSDL file menu's delete: the first click only opens the strip, and `removeUpload` clears `upload` and sets `deleted`. Keep it out of `window.confirm` and out of a floating `role="menu"` (a11y warnings in `npm run check`), and keep the delete triggers gated on a loaded `upload`.
- Keep `invoke` argument names snake_case to match the Rust command signatures.
- Browser mode must remain functional for client-side XML/JSON/URL/Compare features.
- New Tauri APIs may require capability updates; check `capabilities/default.json`.
- REST command functions are Rust `cmd_*` wrappers with `#[tauri::command(rename = "...", rename_all = "snake_case")]` because the DB helpers already use the public names. Keep the invoke names unchanged when adding commands.
- Query rows are derived from the URL and Cookies rows are derived from the `Cookie` header. Do not store them twice; edit the URL or header through the shared helpers in `utils/rest.ts`.
- Validation must follow the wire format, not the UI text: query hints read the raw query segments (`validateQueryUrl`) because table rows are re-encoded by `buildQuery`, and form rows only reject control characters because `encodeForm` escapes the rest. Validating decoded query rows would flag perfectly good URLs (`a%20b` decodes to `a b`), and validating form values against RFC 3986 would reject input the client encodes correctly.
- Keep send, cURL copy/export and response download gated by the same `validateRequest` result; a request that cannot be sent must not be exported as a cURL command either.
- Do not let a row's name become optional in the UI: `write_headers`/`write_form` drop blank-name rows and `parseCookies` must keep name-less pairs. Dropping the value instead of keeping `=value` would delete what the user typed the moment they cleared the name; keep `validatePropertyField` silent on blank text so typing a long value before the name is not flagged, and use `validateRowName`/`validateRequest` for the mandatory-name rule.
- Form-encode rows are the only source of truth for `application/x-www-form-urlencoded` bodies and live in `rest_request_form`. Do not serialize them into `rest_requests.body`, and do not parse the text buffer into rows or overwrite the buffer when switching body tabs. `prepare_call` calls `resolve_body`, so the table wins whenever `body_type = 'form'`; the client uses `resolveRequestBody` for the raw preview and cURL export, and `execute_rest_request`/`download_rest_response` receive `form` alongside `body`.
- Multipart works the same way for `body_type = 'multipart'`: `rest_request_multipart` (Fields) and `rest_request_files` (Files) are the only source of truth, the text buffer and the Form-encode table stay untouched, and `execute_rest_request`/`download_rest_response` receive `multipart` and `files` alongside `body`.
- `reqwest` is built with `default-features = false`, so `reqwest::multipart` does not exist. Keep the hand-written `encode_multipart` that assembles the body as `Vec<u8>` and pass it through `builder.body(...)`; do not "simplify" it by enabling reqwest's multipart feature. Files are read with `std::fs`, so no Tauri `fs` capability is needed and none should be added.
- `prepare_call` always generates a fresh `Uuid::new_v4()` boundary, so a Rust test cannot compare a produced wire body against a fixed-boundary length or bytes: read the boundary back out of the `content-type` header (`value.to_str()` — headers are `(String, HeaderValue)`) and assert the framing with that.
- `parseCurl` always returns `multipart` and `files`, so every caller must handle `bodyType: 'multipart'` with an empty text body — including `importCurl` in `/rest`, which sets the tab and both tables instead of the buffer.
- The browser's picked `File` is runtime-only state: `services/rest.ts::storedFileRow` and `/rest/+page.svelte::toUpdate` strip it before persistence, and `buildMultipartFormData` returns `null` without it, so the browser path throws "Choose the file again…". Desktop re-reads bytes from `path` on every run, which is why a moved or renamed file fails at send time rather than at save time, and why `duplicateRestRequest` in the browser loses the file while `duplicate_rest_request` in Rust keeps it.
- Part media types are deliberately **not** mirrored: the browser uses the `MEDIA_TYPES` table, Rust uses `mime_guess::from_path(..).first_raw()` — note `first_guess()` does not exist on `MimeGuess` — and both fall back to `application/octet-stream`. Do not add cross-platform media-type parity tests.
- `buildCurl` Windows output is cmd.exe-safe: arguments use plain `"..."` quotes (caret `^"value^"` is **not** used — cmd does not enter quoting mode with it, so `&` would split the command), inner quotes become `\"`, backslashes double, `%` stays single for interactive cmd.exe paste (batch files require manual `%%`, documented in README), values collapse newlines to spaces, the URL sits right after `curl.exe` on the first line, and parts join with `' ^\r\n'` so cmd.exe runs the multi-line command as one line. `parseCurl` detects the `curl.exe` flavor, strips caret quotes and collapses `%%`, so both the current plain-quote format and legacy/foreign caret-quoted commands round-trip; POSIX carets and percent signs stay untouched.
- `parseCurl` implements three quoting flavors: POSIX single quotes (nothing is escaped, an embedded quote is the `'\''` idiom), POSIX double quotes (the backslash is dropped only in front of `$`, a backtick, `"`, `\` or a newline — `"a\nb"` keeps both characters), and bash ANSI-C `$'…'`, which is what browsers produce for "Copy as cURL (bash)" and which decodes `\n`, `\r`, `\t`, `\\`, `\'`, `\"`, `\xHH`, `\uHHHH` and octal escapes. Empty quoted arguments are tokens of their own so an empty value never shifts the next argument. `unescapeImportedBody` then decodes `\"`, `\'`, `\n`, `\r`, `\t`, `\b`, `\f`, `\v` in an imported non-form body only when the raw text cannot be the payload: a body that already parses as JSON or XML, or one that already contains real CR/LF, is kept verbatim because `\n` there is content rather than an encoded line break. Keep `\u` out of the trigger pattern — JSON bodies use `\uXXXX` as real content.
- cURL form parts are parsed by `splitFormParameters`, which scans `;filename=` and `;type=` with a global regex in source order (each parameter ends at the next `;`), so parameters arrive in any order and a plain field value like `a;b` keeps its semicolon. `--form-string` stays a Fields row even when the value starts with `@`, because that flag never reads a file, and the imported `multipart/form-data; boundary=…` header is dropped from the header table since the encoder owns the boundary. Export writes `--form name="value"` and `--form name=@"path"[;filename=alias]` through `multipartCurlParts` and never emits `--data-raw` for the Form tab; rows without a readable path are skipped.
- The Raw-request preview keeps one module-level `previewBoundary = multipartBoundary()` instead of regenerating per keystroke, so the dump does not churn while the user types; the real request always gets a fresh boundary, which is why a stored dump's boundary never matches the one that was actually sent.
- The Raw response dump is built by `utils/rest.ts::buildRawResponse` over the received bytes, so formatting the `Response` tab must never change it, and its first line comes from `RestResponse.status_line` / `rest_requests.response_status_line` — not from `status` + `status_text`, which together cannot name the protocol version. Keep the two producers of that line in step: `rest.rs::build_status_line` reads `response.version()` **before** the body is consumed (`Version` has no `Display`; `{version:?}` prints `HTTP/1.1`/`HTTP/2.0`) and drops the reason phrase for a code `canonical_reason` never named, while the browser fallback prints `HTTP/1.1 <status> <statusText>` because `fetch` exposes no version. `buildRawResponse` skips a blank status line rather than inventing one, which is what lets a response stored before the column exist show its body and headers. `/wsdl` reuses that same builder and adds `utils/wsdl.ts::formatRawResponseDump` on top: it formats the payload with `formatContent('xml', …)` and rebuilds the dump around the original status line and header rows, so the framing is never parsed or rewritten. Keep it that way — formatting the dump text itself would let a body that happens to look like `Name: value` become a header.
- Response language detection is mirrored: `utils/rest.ts::detectLanguageFromContentType`/`detectLanguageFromHeaders` and `rest.rs::language_for_content_type`/`language_for_body`. Any subtype that contains `html` is HTML and is matched **before** XML, so `application/xhtml+xml` never lands on the XML language, and a header-less body is HTML when it carries `<html` or starts with `<!doctype html` (case-insensitive) — everything else that starts with `<` stays XML. `/rest` and `/wsdl` hand that language straight to `CodeMirrorEditor`, so an HTML answer is highlighted as markup. `/rest`'s two `Raw` dump tabs deliberately stay `language="text"` because they show the bytes as received; `/wsdl`'s response `Raw` tab is the one exception — it renders as XML when the answer is XML, because the whole exchange is shown there and the tab also offers `Format` for that payload. Do not give `/wsdl`'s request `Raw` tab a language beyond `text` and do not add `Format` to `/rest`'s dumps: only the WSDL response `Raw` tab has both. Share's viewer mapping (`share.rs::language_for`) is a separate rule that offers only xml/json/text — giving it HTML means changing that page's language select and its README contract, not just this function.
- The REST Save checkbox applies to text responses only. Attachments and binary responses must be fetched with `download_rest_response` instead of being loaded into the editor.
- Keep REST execution on the Rust backend for desktop to avoid CORS. The browser `fetch` fallback is best-effort and may be blocked by CORS or forbidden headers.
- `services/rest.ts::normalizeUrl` tests the scheme with an **anchored** regex (`^[a-z][a-z\d+.-]*://`), because an unanchored search matched the `://` inside a query (`x?next=http://y`) and handed the constructor an unparseable string; it also guards the constructor with a protocol check so an `ftp://` or blank URL raises the service's own `Only http and https URLs are supported` message (`notifyError` then shows it verbatim, as all `services/` throw texts are English) instead of a raw `TypeError`. It is exported so `src/lib/services/rest.test.ts` pins the shape rules and `/rest` keeps calling it before sending.
- The browser `downloadRestResponse` registers an `AbortController` in the shared cancel set used by `cancelRestRequest`, so cancelling a browser-mode download actually stops the fetch instead of answering success while the body is still streaming. Desktop cancellation stays on `cancel_rest_request`.
- Avoid reintroducing nav hover background colors or removing `overflow-y-hidden`/`py-1`; that combination regressed the layout before.
- Keep the updater endpoint HTTPS-only and signed: `requireSignedVersion` is on, so a release whose minisign signature has no version in its trusted comment is rejected — re-sign old releases with a CLI that records the version instead of relaxing the flag. Do not commit the private key, and do not paste it into `tauri.conf.json`; only the `pubkey` belongs there.
- The updater is desktop-only. Gate both the navbar button and the startup check on `isTauriRuntime()` (`isUpdaterSupported()`): the browser bundle must not import `@tauri-apps/plugin-updater`, and `isUpdaterSupported()` returns before the state leaves `idle` so no banner can appear.
- Never auto-restart after `downloadAndInstall`. On Windows the plugin kills the process to hand over to `msiexec`, so a restart call there is dead code at best, and on macOS/Linux the user decides when to leave the app. Keep `restart_app` wired to the banner's Restart button only.
- Automatic checks must not raise toasts or banners on failure — a user with no network or no published release would get a red error on every start. Only manual checks answer, and `notFound` stays an info toast rather than a red banner; widening `classifyUpdateFailure` matchers requires the plugin's actual `error.rs` strings, so re-read them when the plugin version changes.
- Release the announced `Update` handle (`closeUpdate`) whenever the banner leaves `available`; keeping it alive leaks the download the plugin already reserved, and the store forgets it in `dismissUpdate`/`clearUpdate`/`handleFailure`.
- `update.*` i18n keys must exist in both the `en` and `ru` dictionaries; the banner interpolates `{version}` and `{percent}`, and `update.failed` interpolates `{message}` with the raw plugin text (deliberately untranslated — `classifyUpdateFailure` only groups it).
- The publish job addresses the release by the **id** the manifest job exported, never by tag: `GET /repos/{owner}/{repo}/releases/tags/{tag}` and therefore `gh release edit <tag>` return published releases only, so pointing them at the draft answers "release not found" and the release stays a draft forever — which is what failed run 4 of `v0.1.0` after all four builds had already attached signed bundles. `tauri-action` finds its own draft through the release list, so a re-run for the same tag reuses that draft and re-uploads the assets instead of duplicating the release. `make_latest` goes in a separate PATCH after `draft=false`, because GitHub refuses to mark a draft or a prerelease as latest. Send it with `-f` (raw string), not `-F`: the API declares `make_latest` a string enum (`true|false|legacy`), and `gh release edit --latest` formats it as a string too — a JSON boolean is the wrong type. Publishing itself already defaults `make_latest` to `true`, so the explicit call only pins the intent.

- WSDL parsing matches resolved namespace URIs, never prefixes, and it never validates: a document that is not WSDL 1.1 answers `not-wsdl:` or `wsdl2:`, and a document with no SOAP operation answers `no-operations:` — all before anything is written to SQLite. Do not widen the parser to WSDL 2.0's `service/endpoint/interface` model; the plan keeps it out of scope.
- Import warnings must not become errors. A missing `soap:address`, a binding without a `portType`, an unresolvable or recursive XSD type and a template that hits `MAX_TEMPLATE_DEPTH`/`MAX_TEMPLATE_ELEMENTS` all produce a successful import plus a warning code. Adding a new code means adding it to `WARNING_KEYS` in `utils/wsdl.ts` **and** to both `en` and `ru` dictionaries, or `describeWarningText` falls through to `wsdl.warning.unknownType` with the raw code as the detail.
- Keep the locked-header rules identical in `utils/wsdl.ts` and `wsdl/store.rs` (`soapActionForCall`/`soap_action_for_call` quote once, `editableForSave`/`editable_for_save` drop a user row whose name matches a mandatory one case-insensitively, mandatory rows come first in the merged list). Because the rows are recomputed on every read and write, a stored `Content-Type`/`SOAPAction` must never reach the wire twice; do not persist locked rows and do not let the UI edit them.
- `wsdl_operation_state` is lazy: nothing exists for an operation until `get_wsdl_operation` opens it, so a page must not assume a state row when listing files, and `save_wsdl_operation_state` must be given the operation id the detail was loaded for — `operation_keys` answers `not-found:` for an operation removed by a re-import or a delete.
- Re-import semantics are the reason ids are name-derived: do not switch `service_id`/`operation_id` to `Uuid::new_v4()`, or every re-import of the same URL would orphan the saved bodies, chosen endpoints and headers. `kept_endpoint_ids` likewise must keep matching on (operation, URL, SOAP version) or a stored `endpoint_id` silently falls back to `''` and the endpoint select resets.
- `''` project id is NULL in `wsdl_files` and only `project_id IS ?1` compares it; `UNIQUE(project_id, source_url)` does not dedupe that bucket, so the duplicate check must stay in `find_file` rather than relying on the constraint.
- `cancel_wsdl_fetch` and `cancel_rest_request` are different channels: cancelling an import does not stop a running SOAP call and vice versa. `cancel_wsdl_fetch` queues a cancellation when no import runs, which the next import consumes and answers `cancelled` without touching the network — clear it on every exit path, or a later import fails for a stale reason.
- The browser build must not import `@tauri-apps/*` through `services/wsdl.ts`: `/wsdl` checks `isWsdlSupported()` and renders the stub. A service call from the browser throws `WSDL support is available in the desktop app only`, which `notifyError` shows verbatim (English, like all `services/` throw texts).
- SOAP faults are responses: `execute_rest_request` returns a non-2xx answer as a `RestResponse`, so `/wsdl` must keep showing status 500 with the `soap:Fault` body rather than routing it to `notifyError`. Only transport errors reach the toast.
- Response persistence is gated by the `Save` checkbox and by `isSavableResponse` (a response that is neither an attachment nor binary): `persistResponse` runs only right after a successful call, and `toggleSaveResponse` writes/clears without re-sending. Formatting must never rewrite the stored body — `transformResponse` touches only `responseText`, so `Raw` and the DB keep the bytes as received, and a stored response re-opened after a restart is shown unformatted.
- `downloadResponse` on `/wsdl` re-sends the SOAP call through `downloadRestResponse` rather than reusing the on-screen body: the answer to a repeated call may differ, and the editor mirror may have been formatted. It is gated by the same `headerIssues` as `runOperation`.
- `/wsdl` must not import the cURL helpers of `utils/rest.ts`, and `/rest` must not import WSDL helpers; the shared pieces are `utils/validation.ts`, `formatRestSize`/`formatRestTime`, `filenameFromDisposition` and `services/files.ts::sharedFileName`. Adding a validation message means adding it to both dictionaries once — do not fork a WSDL-specific copy of a `rest.validation.*` text.
- A floating panel whose item only *sets* state — the WSDL file menu's `confirmingFileId` delete step — must not be dismissed by the window click handler that runs later in the same tick, otherwise the confirmation is cleared before it paints and the action never runs. Two rules keep it alive: every menu item stops propagation (`runMenuAction`, `withFileMenu`, `confirmDeleteFile`), and `WsdlExplorer.svelte::onWindowClick` dismisses only on a **connected** target inside no `[role="menu"], [role="dialog"]` subtree. The second check matters because a browser applies the state change inside the item handler, so the clicked button is already detached from the panel when the event reaches the window and `closest()` then finds nothing — the `...` menu used to close instead of asking. Do not fix this by putting `onclick` on the menu wrapper: `role="menu"` then needs `tabindex` and a keyboard handler, which `npm run check` reports as a11y warnings. `WsdlExplorer.test.ts` pins both halves: no menu click reaches a window listener, and a click whose target is detached keeps the confirmation open.
- The same menu's first `Delete` item must only open the confirmation (`confirmDeleteFile` sets `confirmingFileId`, after stopping propagation); wiring `withFileMenu(event, onDeleteFile)` into it deletes the file on the first click and the `{:else}` confirm branch becomes dead code. `WsdlExplorer.test.ts` pins "the callback is not called before the confirmation" for both the `...` button and the row context menu.
- Never call `$t()` inside a template literal in a template attribute: `` aria-label={`${name} · $t(key)}`} `` renders the `$t(key)}` text verbatim (Svelte only substitutes `${…}` chunks), so screen readers announce raw code. Build such strings in the script — `WsdlExplorer.svelte::rowLabel(name, key)` — and assert the rendered `aria-label` in a test.
- The WSDL request-template generator is a starting point, not a validator: it never rejects the user's body, and it never invents closing tags. Keep `depth-limit`/`element-limit` as warnings rather than raising, and keep the truncation comment legible.
- Template comment markers never nest: every note line goes through `xsd.rs::note`, and `comment_block` flattens an already-commented line through `comment_body` instead of wrapping it again, while keeping the line's own leading indentation so an uncommented block nests the way the schema does. Do not add a wrapper that emits `<!-- ` around a comment (`<!-- <!-- x --> -->` is not XML, and `soften` turns the inner markers into the unreadable `<!- - … - ->`), and do not print a note with a bare `format!("{pad}<!-- … -->")` — a name or a built-in sample containing `--` would break the comment.
- A derived type must contribute both halves of the derivation: keep `xsd.rs::derivation` returning `Derivation { base, attributes, content }` and keep `complex()` ordering the inherited fields before the added ones. Following only `base` is the bug this replaced — the `RequestWith…`/`ResponseWith…` wrappers of a real WSDL then lose the body element they declare over their base, and no import warning says so. `Definition.text` is copied at three places (`expand_element`, the definition a resolved reference returns, and the `head` field of `body_template`); dropping it at any hop prints an empty tag for a `simpleContent` type that has attributes, and `Definition::leaf` keeps `text: None` on purpose because a leaf prints its sample from `Kind::Leaf`.

## 8. Adding a Feature

1. **Write the product contract first.** Add or update a `README.md` section with goal, user flow, UI states, persistence, acceptance criteria, out-of-scope items, and tests.
2. **Map the implementation.** Add an `Active feature` note or mapping here: route/component, service, util, Tauri command, DB table/key, and tests.
3. **Inspect the existing code** for the closest pattern before creating files. Prefer extending shared components over page duplicates.
4. **Implement in small slices.** Keep client-side behavior in `src/lib/utils`, Tauri/native behavior behind `src/lib/services`, and Rust-specific logic in `src-tauri/src`.
5. **Add tests at the same layer.** Formatters/diff in Vitest; DB/processor/tray mapping in Rust. Add edge cases listed in the README.
6. **Run verification.** At minimum `npm run test`, `npm run check`, and affected Rust tests; run `npm run build` for frontend changes and a Tauri dev/build smoke test for native changes.
7. **Update docs.** Remove the `Active feature` note once complete; keep README behavior and AGENTS implementation accurate.

Feature mapping template:

```md
## Active feature: <name>
- README: <section / acceptance criteria>
- Route/UI:
- Services/utils:
- Tauri command:
- SQLite table/key:
- Tests:
- Out of scope:
```

## 9. Scope & Non-Goals

- Share is desktop-only and LAN-only. Rest client runs requests through the desktop backend; the browser build has CORS-limited fallback behavior.
- No server-side rendering, server routes, or remote backend.
- No external icon or component libraries.
- No mobile target.
- No `tauri-plugin-sql`; SQLite is owned by `rusqlite`.
- No unbounded in-memory diff for large files.
- Theme and locale remain `localStorage`-based unless persistence is explicitly redesigned.
