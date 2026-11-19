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
- **Editors:** CodeMirror 6 (`basicSetup`, JSON/XML language packages, one-dark highlight style).
- **Database:** SQLite via `rusqlite` with `bundled` feature. Not `tauri-plugin-sql`.
- **File processing:** `quick-xml 0.42`, `serde_json` with `preserve_order`.
- **Tauri plugins:** `dialog`, `single-instance`, `deep-link`, `clipboard-manager`.
- **Testing:** Vitest + jsdom for frontend; Rust unit tests for DB, processing, and tray mapping.
- **Current status:** XML, JSON, URL, Compare, Share (desktop only), Rest client, themes, i18n, persistence, notifications, tray, single-instance, deep links, and large-file processing are implemented.

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
```

- `npm run dev` runs Vite on `http://localhost:1420` with `strictPort: true`.
- If `npm run tauri dev` fails with "Port 1420 is already in use", a stale Vite is holding it. When Tauri runs on Windows while a WSL-side `npm run dev` is alive, WSL localhost forwarding reserves 1420 on the Windows side too; find it with `ss -ltnp | grep 1420` (WSL) and stop that `vite` process.
- `node_modules` installed from Windows only carries the win32 rollup/esbuild binaries, so Vitest, `npm run check` and `npm run build` fail under WSL with "Cannot find module @rollup/rollup-linux-x64-gnu". Restore them without touching `package-lock.json`: `npm pack @rollup/rollup-linux-x64-gnu@<rollup version> @esbuild/linux-x64@<esbuild version>`, then `tar -xzf <tgz> -C node_modules/@rollup/rollup-linux-x64-gnu --strip-components=1` (and `-C node_modules/@esbuild/linux-x64`), then `chmod +x node_modules/@esbuild/linux-x64/bin/esbuild`. A Windows-side `npm i` deletes these directories again; `npm ci` inside WSL is the persistent fix.
- `npm run check` runs `svelte-kit sync && svelte-check`.
- `npm run build` writes the static frontend to `build/`.
- Rust tests live in `src-tauri/src/*.rs` under `#[cfg(test)]`.

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
│   ├── components/
│   │   ├── CodeMirrorEditor.svelte       # reusable CodeMirror wrapper + highlight API
│   │   ├── Icon.svelte                   # inline SVG path dictionary
│   │   ├── Navbar.svelte                 # nav pills, language, theme controls
│   │   ├── PropertyEditor.svelte         # shared name/value table for REST properties + invalid-cell hints
│   │   ├── RestExplorer.svelte           # REST projects and request list UI
│   │   ├── Toaster.svelte                # global notification stack
│   │   └── XmlJsonFormatter.svelte       # shared XML/JSON tool UI
│   ├── services/
│   │   ├── db.ts                         # Tauri IPC + localStorage fallback
│   │   ├── files.ts                      # native dialogs and path helpers
│   │   ├── rest.ts                       # REST IPC wrappers + browser fallback
│   │   ├── share.ts                      # share IPC wrappers + viewer URL helpers
│   │   └── share.test.ts
│   ├── stores/
│   │   ├── i18n.ts                       # en/ru dictionaries, locale detection
│   │   ├── share.ts                      # draftShare handoff from XML/JSON result
│   │   ├── theme.ts                      # system/dark/light mode
│   │   └── toast.ts                      # transient success/error/info notifications
│   └── utils/
│       ├── clipboard.ts                  # clipboard read/write with notification fallback
│       ├── diff.ts                       # line LCS + inline token diff
│       ├── diff.test.ts
│       ├── formatters.ts                 # XML/JSON/URL transformations
│       ├── formatters.test.ts
│       ├── rest.ts                       # query/cookies/form + cURL helpers + RFC validation
│       ├── rest.test.ts
│       ├── splash.ts                     # loading splash labels, version, dismissal
│       └── splash.test.ts
└── routes/
    ├── +layout.svelte                    # navbar, full-height shell, splash dismissal, deep-link navigation
    ├── +layout.ts                        # ssr = false; prerender = false
    ├── +page.ts                          # 307 redirect to /xml
    ├── xml/+page.svelte                  # XmlJsonFormatter mode="xml"
    ├── json/+page.svelte                 # XmlJsonFormatter mode="json"
    ├── url/+page.svelte                  # URL encode/decode
    ├── compare/+page.svelte              # diff UI
    ├── share/+page.svelte                # desktop share UI: interfaces, file/content upload
    ├── view/+page.svelte                 # read-only viewer opened by deep link
    └── rest/+page.svelte                 # REST client: explorer, request editor, response

src-tauri/
├── Cargo.toml
├── tauri.conf.json
├── capabilities/default.json
├── icon-source.png                       # source PNG for icon generation
├── icons/                                # generated bundle + app-icon.svg
└── src/
    ├── main.rs
    ├── lib.rs                            # tray, single-instance, DB init, commands, deep link
    ├── db.rs                             # schema, editor/settings CRUD, shared_uploads CRUD
    ├── processor.rs                      # large-file XML/JSON processing
    ├── rest.rs                           # REST projects/requests persistence, execution, cancellation
    ├── share.rs                          # interfaces, deep link, upload/viewer commands, server state
    └── http_server.rs                    # read-only axum share server on port 3030

src-tauri/tests/http_client.rs            # reqwest/axum plain-HTTP round trip
screenshots/                              # design references, not automated tests
static/favicon.svg
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
  language?: 'xml' | 'json' | 'text';
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

- `Highlight` has `from`, `to`, `class`, and optional `line`.
- `line: true` creates a line decoration; otherwise a mark decoration.
- Reconfigure language, read-only mode, placeholder, and theme through compartments instead of recreating the view.
- Keep external value synchronization guarded by `nextValue !== view.state.doc.toString()`.

### Icons

- **No external icon libraries.** All icons live in `ICON_PATHS` in `src/lib/components/Icon.svelte`.
- Store one or more SVG `d` strings per icon. Render only `<path d={...} />`.
- **Never** use empty fragments `<>...</>` inside `<svg>`.
- Current names include `copy`, `download`, `share`, `chevronDown`, `sun`, `moon`, `monitor`, `arrowRight`, `check`, `swap`, `file`, `compare`, `server`, `sparkles`, `globe`, `more`, `plus`, `trash`, `clipboard`, `braces`, `alignLeft`, `eraser`, `eye`, `play`, `folder`, `close`.

### Styling and themes

- Use CSS variables from `src/app.css`; do not hardcode palette colors in new components when an existing variable fits.
- List-row states use `--row-hover` and `--row-active` (light: gray hover + amber-tinted active, dark: lighter gray + amber-tinted active), not `--panel-bg-alt`, because `--panel-bg-alt` is nearly invisible on the white panel. The selected REST request row also paints an inset accent stripe (`shadow-[inset_3px_0_0_var(--accent)]`) so the selection is legible in both themes. The REST row `...` actions button is rendered only for the selected request (`{#if active}`), never revealed by hover; right-click still opens the menu on any row.
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

### Localization

- `src/lib/stores/i18n.ts` contains `en` and `ru` dictionaries.
- Use `$t('key', { param: value })`; never hardcode user-facing strings in pages/components.
- Locale storage key: `devtools.locale`; default is `ru` when `navigator.language` starts with `ru`, otherwise `en`.
- Keep `document.documentElement.lang` in sync with the selected locale. `src/app.html` sets `lang` and the localized splash/document title from the stored locale before the bundle loads; the layout then owns the real title (`app.title` + version).
- The window/tab title comes from the `app.title` key plus the app version (`$app/environment` `version`, set from `package.json` via `kit.version.name` in `svelte.config.js`). `+layout.svelte` updates `document.title` and the Tauri window title through `getCurrentWindow().setTitle()` on locale change.
- REST validation messages live in `rest.validation.headerName|headerValue|cookieName|cookieValue|queryParam|formParam|blocked`. They interpolate `{name}` and `{char}`; `+page.svelte` maps each `ValidationKind` to its key through `VALIDATION_KEYS`, and `blocked` wraps the first issue as `{detail}`. The en strings use double quotes because the RFC token character list contains an apostrophe.

### Layout interactions

- Vertical resizer uses `.resizer-v`; horizontal resizer uses `.resizer-h`; active state adds `.active`.
- XML/JSON layout columns: source, 10px vertical resizer, 232px controls, 14px spacer, result. The grid has a single row.
- Both XML/JSON panels use an `h-11` header bar above the editor: the source panel holds the hidden file input, `Select file`, the current file name/status, and the `Format` file button; the output panel holds share/copy/download.
- URL layout: source textarea, 12px horizontal resizer, centered buttons, result textarea.
- Compare layout: two equal panes and a fixed 160px center column; it must fill the available height.
- Copy feedback lasts 1.5 s. Formatter errors clear after 4.5 s.
- `PropertyEditor.svelte` renders no column-title header row; `nameLabel`/`valueLabel` props are used only in `aria-label`, and placeholders convey the column meaning. Keep the add-row input directly under the container border.
- `PropertyEditor.svelte` takes an optional `fieldHint(field, row, index) => string | null`. A non-null hint paints the cell with `--danger-bg`/`--danger-text`, sets `aria-invalid` and puts the localized reason in `title`. `commitDraft()` refuses an invalid draft, so a broken name or value never becomes a row; the text stays on screen highlighted for correction.

## 5. Persistence, IPC, and Backend

### SQLite

- Database file: `webdev-useful-tools.db` in `app_handle.path().app_data_dir()`.
- Initialized once in `src-tauri/src/lib.rs` via `db::init_schema`.
- Schema is idempotent (`CREATE TABLE IF NOT EXISTS`); there is no migration framework yet.
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
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP
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
```

Rest storage:

- `rest_requests.body` is only the JSON/XML/Text editor buffer. Form-encode rows are never serialized into it.
- `rest_request_form` holds the Form-encode tab table (`name`, `value`, `sort_order`) independently of the text buffer; `write_form` drops rows with a blank name, like header rows.
- `init_schema` migrates legacy `rest_request_properties` rows with `kind = 'form'` into `rest_request_form` and deletes them. Only `kind = 'header'` is written today.
- `rest.rs::resolve_body` (client mirrors: `utils/rest.ts::resolveRequestBody`, `services/rest.ts::wireBody`) gives form rows priority when `body_type = 'form'`: the wire body is `encode_form_urlencoded(form)`, the default content type is `application/x-www-form-urlencoded`, and the text buffer is ignored and left untouched.

REST validation (client-side only, `utils/rest.ts`):

```ts
type PropertyKind = 'header' | 'cookie' | 'query' | 'form';
type ValidationKind =
  | 'header-name' | 'header-value' | 'cookie-name'
  | 'cookie-value' | 'query-param' | 'form-param';

interface ValidationIssue { kind; field: 'name' | 'value'; name; char; row?: number }

validateHeaderName(name) / validateHeaderValue(value)
validateCookieName(name) / validateCookieValue(value)
validatePropertyField(kind, field, text)      // single cell, blank text is never an issue
validateProperties(kind, rows)                // table, issues carry `row`
validateQueryUrl(url)                         // raw query segments, issues carry `row`
validateRequest({ url, headers, form, body_type })  // everything that goes on the wire
describeCharacter(character)                  // ␍ ␊ ␉ ␠ or U+XXXX
```

- Header names and cookie names are RFC 9110 §5.6.2 tokens (`TOKEN_PATTERN`); header values reject CR, LF and the rest of the control range (`CONTROL_CHARACTER`), while HTAB and SP stay legal. Non-ASCII bytes are not rejected, matching the `http` crate's obs-text acceptance.
- Cookie values follow RFC 6265 §4.1.1: printable ASCII without DQUOTE, comma, semicolon or backslash. Cookie rows are derived from the `Cookie` header, so they are validated with the cookie rules, not the generic header rules.
- `validateQueryUrl` inspects the **raw** query segments of the URL, not the decoded rows, because the URL field can hold hand-typed text. `buildQuery` percent-encodes every table row, so query rows typed in the table cannot produce an invalid URL and are intentionally not checked cell-by-cell; the row index comes from `querySegments`, which `parseQuery` shares so hint indices line up.
- `%` counts as valid only as a complete `%XX` escape: `firstQueryViolation` consumes the escape with the percent sign, so `hello%20world` passes and `%zz` fails with `char: '%'`.
- Form rows are only checked for control characters (`validateFormText`) because `encodeFormUrlencoded` percent-encodes them on send; spaces, `&` and `=` in form values are intentional.
- `validateRequest` collects header rows, cookie rows via `parseCookies`, `validateQueryUrl(url)` and form rows only when `body_type === 'form'`. `/rest` blocks `runRequest`, `copyCurl`, `copyRawCurl` and `downloadResponse` with `rest.validation.blocked` when it returns any issue, paints a red dot on the `Headers`/`Query`/`Cookies`/`Form-encode` tabs, and marks the URL input when the query is invalid.

Editor keys in use:

- `xml`, `json` — content, indent, `opt_format`, `opt_unescape`.
- `url` — content only; other fields use defaults.
- `compare-left`, `compare-right` — content per side; other fields use defaults.

Persistence behavior:

- `saveEditorState` writes `localStorage` immediately, then SQLite when running under Tauri.
- `loadEditorState` prefers SQLite, falls back to `localStorage` when Tauri returns `null` or fails.
- Formatter/URL autosave debounce is 450 ms; Compare uses 500 ms.
- localStorage prefixes: `devtools.editor.`, `devtools.setting.`.
- Theme and locale are stored in `localStorage` only. `get_setting`/`set_setting` exist for future settings.

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
| `upload_share_content` | `content`, `filename`, `content_type`, `server` | `ShareUploadResult` |
| `upload_share_file` | `path`, `server` | `ShareUploadResult` |
| `load_shared_upload` | `server`, `id` | `SharedUploadResponse` |
| `download_shared_upload` | `server`, `id`, `path` | `void` |
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
| `save_rest_response` | `id`, `body`, `headers`, `raw_request` | `void` |
| `clear_rest_response` | `id` | `void` |
| `execute_rest_request` | `method`, `url`, `headers`, `body`, `body_type`, `form` | `RestResponse` |
| `cancel_rest_request` | — | `boolean` |
| `download_rest_response` | `method`, `url`, `headers`, `body`, `body_type`, `form`, `path` | `u64` |

`ProcessResult` contains `output_path`, `bytes_in`, `bytes_out`.
`ShareServerStatus` contains `running`, `port`, `error`. `ShareUploadResult` contains `id`, `link`, `server`, `filename`, `size`.

### File processing

- `process_large_file` is async and runs `process_file` in `spawn_blocking`.
- `write_temp` writes a temporary `.<name>.<pid>.tmp` next to the destination, then copies it to the final path and deletes the temp file.
- XML: streaming `Reader`/`Writer::new_with_indent`, preserving declarations; text is trimmed and escaped. Whitespace-only text nodes are dropped.
- JSON: parsed into `serde_json::Value` and pretty-serialized with a custom 2/3/4-space `PrettyFormatter`.
- Fast streaming paths are used only when `do_format = true` and `do_unescape = false`. With unescaping enabled, the whole file is read into memory.
- Escape mode reads the whole file and writes escaped content; the XML/JSON formatter UI currently passes `do_escape: false`.
- Invalid input returns a user-facing error string; temp files are cleaned up.
- The frontend defaults the output path to `<name>.formatted.<ext>`.
- Browser mode has no backend file processing: selecting a file loads it into the left editor and formatting stays client-side.

### Share backend

- `http_server::start` binds `0.0.0.0:3030` once at app setup and stores `ShareServerState` in Tauri state; `restart_share_server` stops the old listener, rebinds, and returns the fresh status.
- The router is read-only: `GET /health`, `GET /api/uploads/{uuid}`, `GET /api/uploads/{uuid}/raw`. Uploads happen only through Tauri commands.
- `normalize_server` accepts only `http://` with port `3030` and loopback or RFC 1918/LAN IPv4 hosts; `normalize_id` accepts only UUID v4. Public or wrong-port hosts are rejected, so deep links cannot point at arbitrary servers.
- Upload limit is 50 MB for both file and content paths; file uploads read the whole file in `spawn_blocking`.
- `build_deep_link`/`parse_deep_link` use scheme `webdev-useful-tools://view?server=...&id=...`; `install_deep_link_handler` stores the target in `PendingDeepLink` and emits `share:deep-link`. The frontend consumes the pending target on mount and navigates to `/view` through `buildViewerUrl`.
- Frontend share pages must gate uploads on `ShareServerStatus.running`; the Share UI shows a Retry button that calls `restart_share_server`.
- `reqwest` is used with `default-features = false, features = ["json", "rustls", "gzip"]`. REST requests support HTTP and HTTPS, automatic gzip decompression, a 120-second timeout, and up to 10 redirects. `src-tauri/tests/http_client.rs` still guards the plain-HTTP client path.

### Desktop integration

- `tauri-plugin-single-instance` (with the `deep-link` feature) focuses and unminimizes `main` when a second instance starts.
- `tauri-plugin-deep-link` registers `webdev-useful-tools://` from `tauri.conf.json > plugins.deep-link.desktop`; the frontend does not call its JS API, so no extra capability entry is required.
- Tray menu IDs `open` and `quit` map to `TrayAction::Open` / `Quit`; unknown IDs are ignored.
- Closing the `main` window prevents close and hides it to the tray. Quit exits the process.
- Window title `<localized app.title> <version>` from `package.json`, set by the frontend via `setTitle`; `tauri.conf.json > app > windows > title` is only the initial value. 1440×920, minimum 1100×700.
- Capabilities in `src-tauri/capabilities/default.json`: `core:default`, `core:window:allow-set-title`, `dialog:default`, `dialog:allow-open`, `dialog:allow-save`, `clipboard-manager:allow-read-text`.
- Clipboard reads go through `tauri-plugin-clipboard-manager` in Tauri. Do not call `navigator.clipboard.readText()` directly from pages: WebView2/WKWebView reject it with a permissions error. In browser mode `utils/clipboard.ts` falls back to `navigator.clipboard` and requires a secure context.
- Custom icon source is `src-tauri/icon-source.png`; SVG companion is `src-tauri/icons/app-icon.svg`. Regenerate the bundle with `npx tauri icon src-tauri/icon-source.png` when the icon changes.

## 6. Feature Implementation Map

- **XML/JSON formatter:** `src/routes/{xml,json}/+page.svelte` → `XmlJsonFormatter.svelte` → `formatters.ts`. Client-side format/escape; file mode uses `processLargeFile`.
- **URL:** `src/routes/url/+page.svelte` → `encodeUrl`/`decodeUrl` in `formatters.ts`. Uses `encodeURI`/`decodeURI`, not `encodeURIComponent`.
- **Compare:** `src/routes/compare/+page.svelte` → `diffTexts`/`lineOffsets`/`splitLines` in `diff.ts`; highlights are applied through `CodeMirrorEditor`.
- **Share (desktop):** `src/routes/share/+page.svelte` → `services/share.ts` → `share.rs` + `http_server.rs`; uploads go to `shared_uploads` in `db.rs`. Draft content arrives from `XmlJsonFormatter.svelte` through the `draftShare` store (`readOnly: true`). Viewer: `src/routes/view/+page.svelte` opened via deep link or `/view?server=...&id=...`.
- **Rest client:** `src/routes/rest/+page.svelte` → `RestExplorer.svelte` + `PropertyEditor.svelte` → `services/rest.ts` → `utils/rest.ts` → `rest.rs`. Projects/requests persist in `rest_projects`, `rest_requests`, `rest_request_properties` (headers) and `rest_request_form` (Form-encode rows). Execution goes through the Rust backend in Tauri; browser mode falls back to `fetch` and `localStorage`. RFC validation lives in `utils/rest.ts` (`validateRequest`, `validateProperties`, `validateQueryUrl`); the page maps `ValidationKind` to the `rest.validation.*` keys, passes `fieldHint` to every `PropertyEditor`, and blocks send/cURL/download when issues exist.
- **Notifications:** `stores/toast.ts` → `Toaster.svelte` in `+layout.svelte`. `utils/clipboard.ts` emits success/failure notifications; pages report errors through `notify`/`notifyError`. Toast text is centered: the message span is `flex-1` with symmetric `px-7`, and the close button is absolutely positioned at `right-2 top-1/2`, so long messages wrap without colliding with it.
- **Theme/i18n:** `theme.ts`, `i18n.ts`, `Navbar.svelte`, `app.html`.
- **Loading splash:** `src/app.html` markup + inline CSS + pre-hydration label script → `utils/splash.ts` → `+layout.svelte` `onMount`. Tests in `utils/splash.test.ts`. No new storage keys: it reads `devtools.theme` and `devtools.locale`.
- **Persistence:** `services/db.ts` → `db.rs` commands; browser fallback is localStorage.
- **Large files:** `services/files.ts` + `services/db.ts` → `processor.rs`.

## 7. Known Pitfalls & Invariants

- Use `serde_json`'s `preserve_order` feature to keep JSON key order stable.
- XML text must be escaped after entity decoding; `quick-xml` `BytesText::from_escaped` expects already-escaped text. Existing tests assert `&apos;` output.
- `quick-xml` `GeneralRef` events need explicit handling; do not silently drop character references.
- URL encoding must use `encodeURI`/`decodeURI` to preserve URL structure. Invalid decode input should return the original string.
- Compare LCS is capped at 4,000,000 cells and inline token diff at 250,000 cells. Over the cap, fall back to prefix/suffix or whole-line detection; do not allocate unbounded DP tables.
- Compare clears highlights when either side changes and on swap.
- `CodeMirrorEditor` must destroy its view on unmount and reconfigure compartments on prop changes.
- The loading splash lives in `src/app.html`, not in a component, because it must paint before the bundle exists. Keep its CSS inline there with `var(--token, fallback)` values, keep the markup a sibling of the `%sveltekit.body%` container, and dismiss it from `+layout.svelte` `onMount` through `utils/splash.ts`. Its label strings are the documented exception to the "no hardcoded user-facing strings" rule: they mirror `app.title` and the loading text, and the layout overwrites the title from the dictionary, so update both places when the app name changes.
- Do not add `tauri-plugin-sql`; use the existing `rusqlite` backend.
- Do not add external icon packs or copy icon markup into pages; extend `ICON_PATHS`.
- Do not put theme/locale into SQLite without updating this document and the persistence contract.
- Share uploads and the viewer require Tauri; the browser build shows the desktop-only notice instead of the share UI.
- Never upload from the Share UI when `get_share_server_status.running` is false; deep links to a stopped server are dead. Use Retry (`restart_share_server`) instead of allowing the upload.
- Share deep links must keep `normalize_server` LAN-only and pinned to port 3030; do not widen it to public hosts.
- Keep `invoke` argument names snake_case to match the Rust command signatures.
- Browser mode must remain functional for client-side XML/JSON/URL/Compare features.
- New Tauri APIs may require capability updates; check `capabilities/default.json`.
- REST command functions are Rust `cmd_*` wrappers with `#[tauri::command(rename = "...", rename_all = "snake_case")]` because the DB helpers already use the public names. Keep the invoke names unchanged when adding commands.
- Query rows are derived from the URL and Cookies rows are derived from the `Cookie` header. Do not store them twice; edit the URL or header through the shared helpers in `utils/rest.ts`.
- Validation must follow the wire format, not the UI text: query hints read the raw query segments (`validateQueryUrl`) because table rows are re-encoded by `buildQuery`, and form rows only reject control characters because `encodeFormUrlencoded` escapes the rest. Validating decoded query rows would flag perfectly good URLs (`a%20b` decodes to `a b`), and validating form values against RFC 3986 would reject input the client encodes correctly.
- Keep send, cURL copy/export and response download gated by the same `validateRequest` result; a request that cannot be sent must not be exported as a cURL command either.
- Form-encode rows are the only source of truth for `application/x-www-form-urlencoded` bodies and live in `rest_request_form`. Do not serialize them into `rest_requests.body`, and do not parse the text buffer into rows or overwrite the buffer when switching body tabs. `prepare_call` calls `resolve_body`, so the table wins whenever `body_type = 'form'`; the client uses `resolveRequestBody` for the raw preview and cURL export, and `execute_rest_request`/`download_rest_response` receive `form` alongside `body`.
- `buildCurl` Windows output is cmd.exe-safe: arguments use plain `"..."` quotes (caret `^"value^"` is **not** used — cmd does not enter quoting mode with it, so `&` would split the command), inner quotes become `\"`, backslashes double, `%` stays single for interactive cmd.exe paste (batch files require manual `%%`, documented in README), values collapse newlines to spaces, the URL sits right after `curl.exe` on the first line, and parts join with `' ^\r\n'` so cmd.exe runs the multi-line command as one line. `parseCurl` detects the `curl.exe` flavor, strips caret quotes and collapses `%%`, so both the current plain-quote format and legacy/foreign caret-quoted commands round-trip; POSIX carets and percent signs stay untouched.
- The REST Save checkbox applies to text responses only. Attachments and binary responses must be fetched with `download_rest_response` instead of being loaded into the editor.
- Keep REST execution on the Rust backend for desktop to avoid CORS. The browser `fetch` fallback is best-effort and may be blocked by CORS or forbidden headers.
- Avoid reintroducing nav hover background colors or removing `overflow-y-hidden`/`py-1`; that combination regressed the layout before.

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
