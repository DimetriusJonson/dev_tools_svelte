# План фичи: глобальные переменные (Global vars)

Источник требований: `globalVarsFeature.txt`.
Статус: план, код ещё не менялся.

## 1. Цель

Дать пользователю один набор именованных значений, которые можно вставлять в запросы Rest client и WSDL-клиента шаблоном `{{ name }}`:

- значения query-параметров;
- значения заголовков запроса;
- тело запроса (JSON/XML/Text, Form-encode, поля multipart).

Переменные бывают двух типов:

1. **Обычные** — имя и значение лежат в SQLite.
2. **Безопасные** — имя и тип лежат в SQLite, значение — только в системном хранилище секретов (`tauri-plugin-keyring`, внутри — crate `keyring`).

Подстановка выполняется на Rust-бэкенде непосредственно перед отправкой, поэтому секретные значения не обязаны попадать в frontend, в SQLite или в сохранённые дампы запросов.

## 2. Зафиксированные решения

| Вопрос | Решение |
| --- | --- |
| Где работает фича | Только desktop (Tauri). В браузере страница показывает заглушку, а `/rest` не отправляет запрос с шаблонами. |
| Синтаксис имени | `[A-Za-z_][A-Za-z0-9_]*`, максимум 64 символа. |
| Синтаксис шаблона | `{{ name }}`, пробелы внутри фигурных скобок допускаются и обрезаются. |
| Уникальность имён | Без учёта регистра: `Token` и `token` — одна переменная. |
| Где подставлять | Значения query-параметров, значения заголовков, тело, значения Form-encode, значения полей multipart. |
| Где не подставлять | Host/path URL, имена query-параметров, имена заголовков, пути и имена файлов. |
| Неизвестное имя | Ошибка `unknown-var:<name>`, запрос не отправляется. |
| Вложенная подстановка | Нет: один проход, значения переменных не раскрываются повторно. |
| Секреты в `Raw` | Маскируются фиксированной строкой `********`, реальная длина не раскрывается. |
| cURL-экспорт | Блокируется, если в запросе остались шаблоны: клиент не должен молча отправлять `{{token}}` как текст. |
| Раскрытие секрета | По кнопке с глазом, с автоскрытием через 15 секунд. |
| Секрет в ответе сервера | Не маскируется: если сервер вернул секрет в теле, он попадёт в ответ и может быть сохранён. Это ограничение документируется. |

## 3. Продуктовый контракт (черновик раздела README)

### 3.1. Страница «Global vars»

- Открывается кнопкой-иконкой в навбаре слева от кнопки проверки обновлений.
- Показывает таблицу переменных: тип (замочек), имя, значение, действия.
- Позволяет создать, переименовать, изменить значение, переключить тип и удалить переменную.
- Не даёт создать две переменные с одинаковым именем (без учёта регистра) и показывает причину прямо в ячейке имени.
- Кнопка копирования кладёт в буфер обмена ровно `{{ name }}`.
- У безопасной переменной значение показано звёздочками; кнопка с глазом раскрывает его, повторное нажатие или таймер снова скрывает.
- Удаление подтверждается на месте (без `window.confirm`).
- В браузере вместо таблицы — сообщение, что глобальные переменные доступны только в desktop-приложении.

### 3.2. Rest client

- В query-таблице, заголовках, теле, Form-encode и полях multipart можно писать `{{ name }}`.
- При отправке и при скачивании ответа бэкенд подставляет реальные значения.
- Если имя не найдено или секрет недоступен, запрос не уходит, показывается локализованная ошибка.
- `Raw`-вкладка запроса до отправки показывает шаблоны, после отправки — реальные значения обычных переменных и `********` вместо безопасных.
- Сохранённый `raw_request` не содержит секретов.
- Копирование cURL блокируется, пока в запросе есть шаблоны.
- В браузере запрос с шаблонами блокируется с сообщением о desktop-only.

### 3.3. WSDL-клиент

- Работает через те же команды `execute_rest_request` / `download_rest_response`, поэтому подстановка в теле SOAP-конверта, пользовательских заголовках и query-параметрах endpoint-URL работает без отдельного кода.
- Заблокированные заголовки `Content-Type` / `SOAPAction` не шаблонизируются: они формируются приложением.
- Сохранённый `raw_request` не содержит секретов.

### 3.4. Критерии приёмки

1. Обычная переменная, использованная в query, заголовке, теле, Form-encode и multipart-поле, доходит до сервера раскрытой.
2. Безопасная переменная делает то же, но её значение отсутствует в SQLite, в `raw_request` и в localStorage.
3. После перезапуска приложения безопасная переменная продолжает подставляться.
4. Переименование переменной не ломает keyring-запись (ключ — id, не имя).
5. Удаление безопасной переменной удаляет и запись в keyring.
6. Дубликат имени (в другом регистре) отклоняется и на клиенте, и в SQLite.
7. Неизвестный шаблон блокирует отправку с понятным сообщением.
8. `Raw`-дамп после отправки не содержит секретов.
9. В браузере страница переменных и отправка с шаблонами недоступны.
10. `npm run test`, `npm run check`, `npm run build`, `cargo test` проходят.

## 4. UI

### 4.1. Навбар

Файл: `src/lib/components/Navbar.svelte`.

- Добавить `<a href="/global-vars">` с иконкой `variable`, только внутри `{#if desktop}` и слева от кнопки обновления.
- Оформление как у остальных круглых кнопок навбара: `h-11 w-11 rounded-full bg-[var(--nav-active-bg)] text-white`, `hover:scale-105 active:scale-95`.
- На `/global-vars` добавить `aria-current="page"` и заметное кольцо/акцент, потому что активной pill-кнопки у этой страницы нет.
- `title` и `aria-label` — `$t('nav.globalVars')`.
- В браузере кнопка не рендерится (как и кнопка обновления).

### 4.2. Страница `/global-vars`

Файл: `src/routes/global-vars/+page.svelte`.

Структура:

- Заголовок: `vars.title`, короткое описание `vars.description`.
- Карточка со списком. Строка таблицы:
  - кнопка типа: `lock` (активен, `aria-pressed="true"`, accent) или `unlock` (обычная, muted);
  - имя: input, сохраняется по blur/Enter с debounce 450 мс;
  - значение:
    - обычная переменная — обычный input;
    - безопасная — `••••••` + кнопка `eye`; после раскрытия — input с реальным значением, blur сохраняет новое значение;
  - кнопка `copy` — копирует `templateFor(name)`;
  - кнопка `trash` — двухшаговое подтверждение на месте (`vars.deleteConfirm`, `common.cancel`, `vars.delete`).
- Под списком — строка добавления: тип, имя, значение, кнопка `plus`/`vars.add`.
- Пустое состояние: `vars.empty`.
- Ошибки имени: `--danger-bg`/`--danger-text`, `aria-invalid`, `title` с причиной.
- Ошибки загрузки/сохранения/раскрытия — через `notifyError`.

Состояния:

- `vars` — загруженный список;
- `draft` — имя/значение/тип новой переменной;
- `revealed` — `Map<id, string>` с раскрытыми секретами;
- `revealTimers` — таймеры автоскрытия;
- `confirmingId` — строка, у которой открыто удаление;
- `confirmingSecureId` — строка, у которой открыто подтверждение перевода secure → plain;
- `savingId` — строка, которая сейчас пишется;
- `loading`, `error`.

Поведение:

- Загрузка: `onMount` → `isVarsSupported()`; если да — `listGlobalVars()`, иначе заглушка `vars.desktopOnly`.
- Создание: trim имени; проверка `validateVarName`; вызов `createGlobalVar`; при `duplicate-name` — ошибка в ячейке.
- Переименование: тот же `validateVarName` с исключением текущего id.
- Переключение plain → secure: отправить `secure: true, value: null`; бэкенд переносит текущее значение в keyring.
- Переключение secure → plain: сначала подтверждение, потом `secure: false, value: null`; бэкенд достаёт секрет и кладёт в SQLite.
- Раскрытие: `revealGlobalVar(id)`; значение хранится только в памяти страницы; таймер 15 с сбрасывает его.
- Копирование шаблона: `copyText(templateFor(name))`; тост `common.copied`.
- Удаление: `deleteGlobalVar(id)`, удаление из списка, очистка reveal-состояния.

Доступность:

- Все иконки-кнопки имеют `title` и `aria-label`.
- Кнопка типа имеет `aria-pressed`.
- Строка удаления — не `role="menu"`, чтобы не ловить a11y-предупреждения.

### 4.3. Rest client

Файл: `src/routes/rest/+page.svelte`.

- Перед `runRequest`, `copyCurl`, `copyRawCurl`, `downloadResponse`:
  - если `!isTauriRuntime()` и в запросе есть шаблоны — блокировать с `vars.desktopOnlySend`;
  - для cURL — если шаблоны есть, блокировать с `vars.curlUnavailable` (см. открытые вопросы);
  - неизвестные имена ловит бэкенд; дополнительная клиентская проверка — опциональный слой (см. разрез 5).
- На вкладке `Raw` запроса добавить подсказку `vars.rawHint`: «Шаблоны раскрываются при отправке; безопасные значения в дампе маскируются».
- Ничего не менять в `buildQuery`: шаблон в query-таблице кодируется как `%7B%7Bname%7D%7D`, бэкенд декодирует пару и подставляет значение. Это сохраняет текущую RFC-валидацию URL.
- Для читаемости `Raw`-превью можно добавить `humanizeTemplateEncoding(url)` (декодировать только `%7B%7B…%7D%7D`), но это не обязательно для первой версии.

### 4.4. WSDL-клиент

Файл: `src/routes/wsdl/+page.svelte`.

- Отдельных изменений в отправке не нужно: она идёт через `executeRestRequest` / `downloadRestResponse`.
- Добавить такую же подсказку на вкладку `Raw` запроса.
- Убедиться, что `mergeLockedHeaders` не пытается валидировать/подставлять locked-строки: подстановка идёт на бэкенде по всем значениям, а locked-значения не содержат шаблонов.

## 5. Данные и безопасность

### 5.1. SQLite

Новая таблица в `src-tauri/src/db.rs::init_schema`:

```sql
CREATE TABLE IF NOT EXISTS global_vars (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  secure INTEGER NOT NULL DEFAULT 0,
  value TEXT NOT NULL DEFAULT '',
  sort_order INTEGER NOT NULL DEFAULT 0,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_global_vars_name
  ON global_vars(name COLLATE NOCASE);
```

Правила:

- `secure = 1` → `value` всегда `''`; реальное значение только в keyring.
- `secure = 0` → значение в `value`.
- Имя хранится trim-нутым; уникальность без учёта регистра обеспечивает индекс.
- `sort_order` — на будущее; список сортируется по `name COLLATE NOCASE`.
- Миграция не нужна: таблица новая, `CREATE TABLE IF NOT EXISTS` достаточно.

### 5.2. Keyring

Зависимость: `tauri-plugin-keyring = "0.1"` (обёртка над `keyring 3.6`, поддерживает Tauri 2).

Инициализация в `src-tauri/src/lib.rs`:

```rust
.plugin(tauri_plugin_keyring::init())
```

Доступ только из Rust:

```rust
use tauri_plugin_keyring::KeyringExt;
// app.keyring().set_password(SERVICE, id, value)
```

- JS-команды плагина не используются, поэтому `capabilities/default.json` не меняется. Не добавлять `keyring:default`: иначе любая страница webview сможет читать секреты.
- Сервис: `const KEYRING_SERVICE: &str = "app.webdev.usefultools.global-vars";`
- Пользователь (account): `id` переменной (UUID). Переименование не трогает keyring.
- Значения — UTF-8 строки.
- Лимит безопасного значения: 2048 байт (ограничение Windows Credential Manager). Превышение → `secret-too-large`.
- `tauri-plugin-keyring::Keyring::get_password` возвращает `Ok(None)` и при отсутствии записи, и при ошибке хранилища. Поэтому на чтении различаем только «нет секрета»; ошибки записи/удаления пробрасываются. Если понадобится различать «нет записи» и «хранилище недоступно», добавить прямую зависимость `keyring = "3"` и читать через `Entry::get_password()`.

Абстракция для тестов:

```rust
pub trait SecretStore: Send + Sync {
    fn get(&self, id: &str) -> Result<Option<String>, String>;
    fn set(&self, id: &str, value: &str) -> Result<(), String>;
    fn delete(&self, id: &str) -> Result<(), String>;
}
```

- `KeyringSecrets { app: AppHandle }` — реальная реализация.
- `MemorySecrets` — в тестах, без обращения к ОС.

Переходы типов:

- **plain → secure**: прочитать `value` из строки (или взять новое значение), записать в keyring, затем одним `UPDATE` поставить `secure = 1, value = ''`. Если `UPDATE` упал — удалить только что созданную запись.
- **secure → plain**: прочитать секрет (или взять новое значение), записать в `value` и `secure = 0`, затем удалить keyring-запись. Если удаление не удалось — вернуть ошибку, но данные уже в SQLite; повторное удаление идемпотентно.
- **удаление**: сначала удалить keyring-запись (отсутствие — не ошибка), затем строку. Если удаление секрета упало — строку не удалять, чтобы не оставить «сироту».
- **создание secure**: записать keyring, затем строку; при ошибке SQL — удалить запись.

### 5.3. Ограничения и правила

- Имя: `[A-Za-z_][A-Za-z0-9_]*`, 1–64 символа, trim.
- Значение обычной переменной: без ограничения (SQLite), UI может ограничить 64 KiB.
- Значение безопасной: до 2048 байт UTF-8.
- Пустое значение разрешено для обоих типов; для secure это означает запись с пустой строкой.
- Переменные глобальные, не привязаны к проекту/запросу.
- Удаление/переименование не обновляет сохранённые запросы: шаблоны в них остаются как текст. Это осознанное поведение; ошибка unknown-var подскажет причину.

## 6. Backend

### 6.1. Новый модуль `src-tauri/src/vars.rs`

Содержит:

- модели `GlobalVar`, `GlobalVarInput`;
- `SecretStore`, `KeyringSecrets`, `MemorySecrets` (последний — под `#[cfg(test)]`);
- CRUD по SQLite;
- резолвер шаблонов;
- загрузку значений для запроса;
- маппинг ошибок в коды.

Публичные типы:

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalVar {
    pub id: String,
    pub name: String,
    pub secure: bool,
    /// `None` для безопасной переменной: значение не покидает keyring без явного reveal.
    pub value: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Default)]
pub struct VarValues {
    /// name.to_ascii_lowercase() → реальное значение
    pub real: HashMap<String, String>,
    /// name.to_ascii_lowercase() → значение для дампа (`********` для secure)
    pub masked: HashMap<String, String>,
}

pub const SECRET_MASK: &str = "********";
pub const MAX_SECRET_BYTES: usize = 2048;
```

Функции:

```rust
pub fn list(conn: &Connection) -> Result<Vec<GlobalVar>, String>;
pub fn create(conn: &Connection, secrets: &dyn SecretStore, input: &GlobalVarInput) -> Result<GlobalVar, String>;
pub fn update(conn: &Connection, secrets: &dyn SecretStore, id: &str, input: &GlobalVarInput) -> Result<GlobalVar, String>;
pub fn delete(conn: &Connection, secrets: &dyn SecretStore, id: &str) -> Result<(), String>;
pub fn reveal(conn: &Connection, secrets: &dyn SecretStore, id: &str) -> Result<String, String>;

pub fn load_values(
    conn: &Connection,
    secrets: &dyn SecretStore,
    referenced: &[String],
) -> Result<VarValues, String>;

pub fn substitute(text: &str, values: &HashMap<String, String>) -> Result<String, String>;
```

`GlobalVarInput`:

```rust
pub struct GlobalVarInput {
    pub name: String,
    pub secure: bool,
    /// `None` — оставить текущее значение (для update); `Some("")` — очистить.
    pub value: Option<String>,
}
```

### 6.2. Tauri-команды

Добавить в `src-tauri/src/vars.rs` и в `generate_handler!`:

| Команда | Аргументы | Возврат |
| --- | --- | --- |
| `list_global_vars` | — | `GlobalVar[]`, у secure `value = null` |
| `create_global_var` | `name`, `secure`, `value` | `GlobalVar` |
| `update_global_var` | `id`, `name`, `secure`, `value` (`string \| null`) | `GlobalVar` |
| `delete_global_var` | `id` | `void` |
| `reveal_global_var` | `id` | `string` |

Все — `#[tauri::command(rename = "...", rename_all = "snake_case")]`, имена аргументов snake_case.

Для keyring команды принимают `app: tauri::AppHandle` и создают `KeyringSecrets { app }`. Блокирующие обращения к keyring выполняются до сетевого запроса; в async-командах отправки — через `tauri::async_runtime::spawn_blocking`, чтобы не блокировать runtime.

### 6.3. Резолвер

Грамматика:

- имя — `[A-Za-z_][A-Za-z0-9_]*`;
- шаблон — `{{`, пробелы, имя, пробелы, `}}`;
- всё, что не подходит под имя (например `{{ not a name }}`), остаётся литералом;
- поиск имени регистронезависимый, ключ карты — lowercase.

Алгоритм `substitute`:

1. Идти по строке, искать `{{`.
2. Найти ближайший `}}`; если его нет — остаток строки копируется как есть.
3. Внутренность trim-нуть; если это не валидное имя — скопировать `{{` и продолжить поиск с позиции +2.
4. Найти имя в карте (lowercase).
5. Нет имени → `Err("unknown-var:<original name>")`.
6. Заменить на значение, продолжить после `}}`.
7. Один проход: значения не сканируются повторно.

`substitute_url`:

- принимает уже разобранный `Url`;
- проходит `query_pairs()` (декодированные имена и значения);
- подставляет только **значения**;
- если хотя бы одно значение изменилось, пересобирает query через `query_pairs_mut().clear().extend_pairs(...)`;
- если шаблонов нет, URL возвращается байт-в-байт (не меняет `%20` на `+`).

Проверка «неподдерживаемых мест»:

- после подстановки проверить path/host, имена query-параметров и имена заголовков на валидный шаблон;
- если найден — `Err("unresolved-template:<name>")`, чтобы пользователь не отправил литерал `{{name}}` в имени.

`load_values`:

- получает список имён, реально встречающихся в запросе;
- читает только эти строки из `global_vars` (`name COLLATE NOCASE`);
- для secure-строк читает keyring; отсутствие → `secret-missing:<name>`;
- строит `real` и `masked` (для secure — `SECRET_MASK`);
- неизвестное имя → `unknown-var:<name>`.

Сбор имён для запроса делает `rest.rs` (см. ниже), потому что query-значения нужно сначала декодировать.

### 6.4. Интеграция в `prepare_call`

Файл: `src-tauri/src/rest.rs`.

- `prepare_call` получает `vars: &VarValues`.
- В `cmd_execute_rest_request` и `cmd_download_rest_response` добавить `db: tauri::State<'_, AppDb>` и `app: tauri::AppHandle`.
- Перед `prepare_call` собрать упомянутые имена:
  - query-значения из разобранного URL (`query_pairs`, decoded);
  - значения заголовков;
  - тело (для `json`/`xml`/`text`);
  - значения `form`;
  - значения `multipart`;
  - значения файловых строк не сканируются.
- Вызвать `vars::load_values` и передать результат в `prepare_call`.
- Для тестов `prepare_call` вызывается с `VarValues::default()`.

Порядок внутри `prepare_call`:

1. `normalize_rest_url(url)` — как сейчас.
2. `substitute_url` для реальных и маскированных значений.
3. Подстановка значений заголовков (имена не трогаем).
4. Подстановка тела, `form`, `multipart`.
5. Проверка неподдерживаемых мест (path/host, имена query/header).
6. `resolve_body` для реальных значений → `wire`.
7. `resolve_body` для маскированных значений → `masked_dump`.
8. `prepare_headers` для реальных значений → заголовки на провод.
9. `prepare_headers` для маскированных значений, но с **реальным** `resolved` (чтобы `Content-Length` и `Content-Type` совпадали с проводом).
10. `build_raw_request(method, masked_url, masked_headers, masked_dump)` → `raw_request`.
11. `PreparedCall { method, url: real_url, headers: real_headers_map, body: real_wire, raw_request }`.

Важно:

- `Content-Length` в дампе остаётся реальным, даже если тело в дампе замаскировано.
- Для multipart `resolve_body` вызывается с одним и тем же boundary; маскированный дамп сохраняет рамку, но заменяет значения полей.
- `encode_multipart` дополнительно отклоняет `\r`/`\n` в значениях полей (после подстановки), чтобы секрет с переводом строки не мог внедрить новую part. Это зеркалит клиентскую валидацию `validateMultipartValue`.
- `raw_request` никогда не содержит реальных secure-значений.
- `RestResponse.raw_request` остаётся строкой; frontend ничего не меняет.

Замечание по query-параметрам:

- `buildQuery` на клиенте кодирует `{{token}}` как `%7B%7Btoken%7D%7D`; это нормально.
- Бэкенд декодирует пару через `query_pairs()`, подставляет значение и кодирует результат заново.
- Перекодирование выполняется только если значение реально изменилось; иначе URL не трогается.

### 6.5. Ошибки

Коды (английские, как остальные сообщения сервисов):

| Код | Когда |
| --- | --- |
| `unknown-var:<name>` | Шаблон с валидным именем, но переменной нет |
| `secret-missing:<name>` | Secure-переменная есть в БД, но записи в keyring нет |
| `secret-store:<detail>` | Ошибка записи/удаления keyring |
| `secret-too-large` | Secure-значение больше 2048 байт |
| `duplicate-name:<name>` | Имя уже занято (без учёта регистра) |
| `invalid-name:<name>` | Имя не соответствует грамматике |
| `unresolved-template:<name>` | Шаблон в неподдерживаемом месте (path/host, имя query/header) |

Frontend: `utils/vars.ts::describeVarError(error, t)` разбирает префикс и возвращает локализованный текст; неизвестный формат показывается как есть через `notifyError`.

### 6.6. Rust-тесты

В `src-tauri/src/vars.rs` (in-memory SQLite + `MemorySecrets`):

- create/list/update/delete обычной переменной;
- secure create: `value` в БД пустой, в `MemorySecrets` есть;
- secure update значения;
- plain → secure и secure → plain;
- удаление secure удаляет запись в keyring;
- duplicate-name в другом регистре;
- invalid-name;
- list не отдаёт secure value;
- reveal отдаёт secure value;
- secret-missing;
- secret-too-large.

В `src-tauri/src/rest.rs`:

- подстановка в header value, body, form, multipart field;
- подстановка в query value (включая `%7B%7B...%7D%7D` и `+`/`%20`);
- неизвестная переменная → ошибка, запрос не строится;
- шаблон в path/header name → `unresolved-template`;
- `raw_request` содержит `********` и не содержит секрет;
- `Content-Length` в дампе соответствует реальному телу;
- multipart с `\r\n` в подставленном значении отклоняется;
- `prepare_call` без переменных ведёт себя как раньше (все существующие тесты).

## 7. Frontend

### 7.1. `src/lib/services/vars.ts`

По образцу `services/wsdl.ts`:

```ts
export interface GlobalVar {
  id: string;
  name: string;
  secure: boolean;
  value: string | null;
  created_at: string;
  updated_at: string;
}

export function isVarsSupported(): boolean; // isTauriRuntime()
export async function listGlobalVars(): Promise<GlobalVar[]>;
export async function createGlobalVar(input: { name: string; secure: boolean; value: string }): Promise<GlobalVar>;
export async function updateGlobalVar(input: { id: string; name: string; secure: boolean; value: string | null }): Promise<GlobalVar>;
export async function deleteGlobalVar(id: string): Promise<void>;
export async function revealGlobalVar(id: string): Promise<string>;
```

- Все функции бросают `Global variables are available in the desktop app only` вне Tauri.
- Никаких `localStorage`-фолбэков: единственный источник — SQLite + keyring.

### 7.2. `src/lib/utils/vars.ts`

Чистые функции:

```ts
export const VAR_NAME_PATTERN = /^[A-Za-z_][A-Za-z0-9_]*$/;
export const TEMPLATE_PATTERN = /\{\{\s*([A-Za-z_][A-Za-z0-9_]*)\s*\}\}/g;
export const SECRET_DOTS = '••••••';

export function templateFor(name: string): string; // `{{ name }}`
export function extractTemplateNames(text: string): string[];
export function containsTemplate(text: string): boolean;
export function validateVarName(name: string, existing: string[], currentId?: string): 'required' | 'invalid' | 'duplicate' | null;
export function describeVarError(error: unknown, t: (key: string, params?: Record<string, unknown>) => string): string;
```

- `extractTemplateNames` повторяет правила Rust-сканера: невалидная внутренность игнорируется.
- `validateVarName` сравнивает имена без учёта регистра.
- `describeVarError` знает коды из 6.5.

### 7.3. Страница

Файл: `src/routes/global-vars/+page.svelte` (детали в 4.2).

- В браузере — заглушка с иконкой `variable`/`server` и текстом `vars.desktopOnly` / `vars.desktopOnlyHint`.
- В desktop — таблица, draft, подтверждения, reveal-таймеры.
- Не использовать `window.confirm`.
- Не использовать `$t()` внутри template literal в атрибутах: строки собирать в скрипте.

### 7.4. Навбар, иконки, i18n

- `Navbar.svelte`: кнопка `variable` слева от update.
- `Icon.svelte`: добавить `lock`, `unlock`, `variable`; каждый использовать.
- `i18n.ts`: добавить `nav.globalVars` и ключи `vars.*` в `en` и `ru`:
  - `vars.title`, `vars.description`, `vars.add`, `vars.empty`;
  - `vars.name`, `vars.value`, `vars.namePlaceholder`, `vars.valuePlaceholder`;
  - `vars.secure`, `vars.secureOn`, `vars.secureOff`, `vars.secureHint`;
  - `vars.copyTemplate`, `vars.templateCopied`;
  - `vars.reveal`, `vars.hide`;
  - `vars.delete`, `vars.deleteConfirm`, `vars.deleteHint`;
  - `vars.nameRequired`, `vars.nameInvalid`, `vars.nameDuplicate`;
  - `vars.secureToPlain`, `vars.secureToPlainConfirm`;
  - `vars.desktopOnly`, `vars.desktopOnlyHint`, `vars.desktopOnlySend`;
  - `vars.unknown`, `vars.secretMissing`, `vars.secretStore`, `vars.secretTooLarge`;
  - `vars.curlUnavailable`, `vars.rawHint`, `vars.loadFailed`, `vars.saveFailed`.
- `i18n.test.ts` проверит паритет ключей и плейсхолдеров.

### 7.5. Rest/WSDL guards

- `src/routes/rest/+page.svelte`:
  - `isTauriRuntime()` уже импортирован;
  - `requestHasTemplates(request)` из `utils/vars.ts` (URL через `parseQuery`, заголовки, тело, form, multipart);
  - блокировка в браузере и для cURL;
  - подсказка на `Raw`.
- `src/routes/wsdl/+page.svelte`:
  - подсказка на `Raw`;
  - ошибки бэкенда уже показываются через `notifyError`; при желании добавить `describeVarError` в обработчик.

### 7.6. Frontend-тесты

- `src/lib/utils/vars.test.ts`:
  - грамматика имени и шаблона;
  - `templateFor` с пробелами;
  - `extractTemplateNames` (несколько, дубликаты, невалидные);
  - `validateVarName` (пусто, невалидно, дубликат в другом регистре, исключение текущего id);
  - `describeVarError` для всех кодов.
- `src/lib/services/vars.test.ts`:
  - вне Tauri каждая функция бросает desktop-only;
  - в Tauri вызывает правильные имена команд и snake_case-аргументы.
- `src/routes/global-vars/page.test.ts` (по образцу `share/page.test.ts`):
  - браузерная заглушка;
  - список с secure-строкой показывает звёздочки, глаз вызывает `reveal_global_var`;
  - копирование шаблона (мок `$lib/utils/clipboard`);
  - duplicate-name показывает ошибку и не вызывает create;
  - удаление требует подтверждения;
  - переключение secure → plain показывает подтверждение.
- `npm run check` и `npm run build` — обязательны.

## 8. Краевые случаи

- **Пустое имя** — не создаётся, `vars.nameRequired`.
- **Имя с пробелами/дефисом/точкой** — невалидно; шаблон с такой внутренностью остаётся литералом.
- **`{{ token }}` и `{{token}}`** — одно и то же имя.
- **`{{Token}}` при переменной `token`** — подставляется (регистронезависимо).
- **Два шаблона в одной строке** — оба подставляются.
- **Вложенные `{{a{{b}}}}`** — литерал, без рекурсии.
- **Значение содержит `{{other}}`** — остаётся как есть (один проход).
- **Значение содержит кавычки/переводы строк** — в JSON/XML вставляется как есть; ответственность пользователя. Для multipart-полей CR/LF отклоняются.
- **Секрет в query** — кодируется бэкендом, специальные символы безопасны.
- **Секрет в заголовке с CR/LF** — `HeaderValue` отклонит, запрос не уйдёт.
- **Секрет в ответе** — не маскируется (ограничение).
- **Удалённая переменная** — запросы с её шаблоном падают с `unknown-var`.
- **Переименование** — запросы со старым именем падают с `unknown-var`; keyring-запись не теряется.
- **Keyring недоступен (Linux без Secret Service)** — создание/раскрытие/использование secure падает с понятной ошибкой; обычные переменные работают.
- **Windows Credential Manager** — лимит 2048 байт.
- **Браузер** — страница-заглушка; `/rest` с шаблонами блокируется.
- **cURL** — блокируется при шаблонах (рекомендация; см. открытые вопросы).
- **Raw-превью до отправки** — показывает шаблоны, не значения; это ожидаемо и безопасно.
- **Raw после отправки** — secure-значения `********`.
- **Сохранённый `raw_request`** — без секретов.
- **Ответ, сохранённый в SQLite** — может содержать секрет, если сервер его вернул; документировать.

## 9. Срезы внедрения

### Срез 1. Хранилище и keyring

- `db.rs`: таблица `global_vars`.
- `vars.rs`: модели, CRUD, `SecretStore`, `KeyringSecrets`, `MemorySecrets`.
- `lib.rs`: `mod vars;`, `.plugin(tauri_plugin_keyring::init())`, команды.
- `Cargo.toml`: `tauri-plugin-keyring`.
- Тесты: CRUD, дубликаты, переходы типов, удаление секрета.

DoD: `cargo test` зелёный; вручную через `invoke` (или временную страницу) создаётся/читается/удаляется переменная.

### Срез 2. Резолвер и `prepare_call`

- `vars.rs`: `substitute`, `load_values`, коды ошибок.
- `rest.rs`: `prepare_call(..., vars)`, сбор имён, маскированный дамп, multipart-гард.
- Тесты: подстановка во всех местах, маскирование, ошибки.

DoD: REST-запрос с обычной и secure-переменной уходит раскрытым; `raw_request` маскирован; `cargo test` зелёный.

### Срез 3. Frontend-сервис, утилиты, страница

- `services/vars.ts`, `utils/vars.ts`.
- `routes/global-vars/+page.svelte`.
- `Navbar.svelte`, `Icon.svelte`, `i18n.ts`.
- Тесты: utils, service, page.

DoD: страница создаёт/редактирует/удаляет переменные, копирует шаблон, раскрывает секрет; `npm run test`, `npm run check`, `npm run build`.

### Срез 4. Интеграция в REST/WSDL UI

- Блокировки в браузере и для cURL.
- Подсказки `Raw`.
- Маппинг ошибок `describeVarError`.
- Тесты: `utils/vars`, при необходимости — точечные тесты страниц.

DoD: сценарии из 3.4 проходят вручную; регрессий в существующих тестах нет.

### Срез 5. Документация и приёмка

- `README.md`: раздел «Глобальные переменные (desktop)» + критерии.
- `AGENTS.md`: схема, команды, файлы, правила, pitfalls.
- Ручной чек-лист (ниже).

DoD: документация соответствует коду; `cargo test`, `npm run test`, `npm run check`, `npm run build` зелёные.

## 10. Файловая карта

Новые файлы:

```text
src-tauri/src/vars.rs
src/routes/global-vars/+page.svelte
src/routes/global-vars/page.test.ts
src/lib/services/vars.ts
src/lib/services/vars.test.ts
src/lib/utils/vars.ts
src/lib/utils/vars.test.ts
```

Изменяемые файлы:

```text
src-tauri/Cargo.toml                 # tauri-plugin-keyring
src-tauri/src/db.rs                  # таблица global_vars
src-tauri/src/lib.rs                 # mod vars, plugin init, команды
src-tauri/src/rest.rs                # prepare_call, команды, маскирование
src/lib/components/Navbar.svelte     # кнопка Global vars
src/lib/components/Icon.svelte       # lock, unlock, variable
src/lib/stores/i18n.ts               # nav.globalVars + vars.*
src/routes/rest/+page.svelte         # guards, raw hint
src/routes/wsdl/+page.svelte         # raw hint
README.md                            # продуктовый раздел
AGENTS.md                            # инженерный контракт
```

Не меняются:

- `src-tauri/capabilities/default.json` — keyring используется только из Rust, JS-разрешения не нужны.
- `package.json` — JS-зависимостей нет.
- `globalVarsFeature.txt` — исходное требование, не трогаем.

## 11. Ручной чек-лист

1. Создать обычную переменную `base` со значением `https://httpbin.org`.
2. Создать secure-переменную `token`.
3. В Rest client:
   - query `q={{ base }}` (или значение с `{{token}}`);
   - заголовок `Authorization: Bearer {{ token }}`;
   - JSON-тело с `"token": "{{ token }}"`;
   - Form-encode и multipart-поле с шаблоном.
4. Отправить: сервер получает реальные значения; `Raw` показывает `********` вместо токена.
5. Сохранить ответ; перезапустить приложение; убедиться, что secure-значение всё ещё подставляется.
6. Открыть `/global-vars`, нажать глаз — значение раскрылось; через 15 с снова скрылось.
7. Скопировать шаблон — в буфере `{{ token }}`.
8. Попробовать создать `Token` — ошибка дубликата.
9. Удалить secure-переменную; проверить, что запись в keyring удалена (например, через `secret-tool`/Keychain Access или повторным созданием).
10. Ввести `{{missing}}` — запрос блокируется с понятным сообщением.
11. Открыть приложение в браузере: `/global-vars` — заглушка; `/rest` с шаблоном — блокировка.
12. WSDL: шаблон в теле SOAP и в пользовательском заголовке — подставляется; `Raw` маскирован.
13. Linux без Secret Service: создание secure-переменной даёт понятную ошибку, обычные работают.

## 12. Риски и открытые вопросы

### Риски

- **Keyring на Linux** требует запущенного Secret Service (GNOME Keyring/KWallet). В headless-окружении secure-переменные недоступны; это нужно явно сообщать, а не падать.
- **`get_password` глотает ошибки**: плагин возвращает `None` и при отсутствии записи, и при недоступном хранилище. Если различие важно, добавить прямую зависимость `keyring` и читать через `Entry`.
- **Секрет в ответе**: приложение не может запретить серверу вернуть секрет; сохранённый ответ может содержать его. Документировать.
- **Перекодирование query** при подстановке может менять `%20` на `+` (только в затронутой query-строке). Это допустимо, но нужно зафиксировать тестом.
- **Лимит Windows** 2048 байт на secure-значение.
- **Производительность**: keyring читается только для реально упомянутых secure-переменных; кэша между запросами нет.
- **Секреты в памяти**: значения попадают в `VarValues` на время запроса; после запроса карта уничтожается. Не логировать.

### Открытые вопросы (рекомендации по умолчанию)

1. **cURL при шаблонах**: блокировать (рекомендуется) или копировать с литералами `{{name}}`?
2. **Браузер**: только заглушка (рекомендуется) или localStorage-фолбэк для обычных переменных?
3. **Раскрытие секрета**: автоскрытие через 15 с (рекомендуется) или до повторного нажатия?
4. **Регистр имён**: без учёта регистра (рекомендуется) или с учётом?
5. **Path/host URL**: не поддерживать в v1 (рекомендуется) или поддержать с процентным кодированием?
6. **Импорт/экспорт переменных**: вне scope v1.
