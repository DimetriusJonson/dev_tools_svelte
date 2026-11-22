# План реализации: страница «WSDL-клиент»

Источник требований: `doc/wsdl_page/readme.txt`, макет: `doc/wsdl_page/wsdl_ui.png`.
Статус: в работе — шаги 1–7 выполнены, выполняется частичная версия шага 8 (отправка без таблицы `Headers`). Дата статуса: 2026-11-22.

## 0. Ход работ

| Шаг | Статус | Что лежит в коде |
| --- | --- | --- |
| 1 | готово | README «WSDL-клиент», AGENTS §5 «WSDL storage», §6 «WSDL client», §7 WSDL-инварианты |
| 2 | готово | `db.rs::init_schema` (6 таблиц + 4 индекса), `wsdl/store.rs` + 12 тестов |
| 3 | готово | `wsdl/parser.rs`: `XmlDom`, `parse_wsdl`, `fetch_wsdl` (таймаут 30 с, лимит 5 МиБ, 20 000 элементов, глубина 512), 18 тестов |
| 4 | готово | `wsdl/xsd.rs`: индекс XSD и генератор шаблона (глубина 32, 2000 элементов, циклы и неразрешённые типы — комментарий + warning) |
| 5 | готово | 12 команд в `wsdl/mod.rs` + регистрация в `lib.rs`, `services/wsdl.ts` (`isWsdlSupported()`), `services/wsdl.test.ts` |
| 6 | готово | `routes/wsdl/+page.svelte`, пункт навбара, блок `wsdl.*` в en/ru, ресайзеры |
| 7 | готово | `WsdlExplorer.svelte`: селект проектов с меню, импорт из буфера + help, дерево, `...` файла с подтверждением, раскрытие ветки, восстановление выбора |
| 8 | частично | Отправка собирает locked-заголовки и вызывает `executeRestRequest`; таблица `Headers` с `locked`-строками (`PropertyEditor` проп уже добавлен) не выведена в панель |
| 9 | не начат | Вкладки `XML` (редактируемый body уже есть) и read-only `Raw` |
| 10 | не начат | Вкладки ответа, `Save`/`Copy`/`Format`/скачивание (`save_wsdl_response`/`clear_wsdl_response` уже есть в бэкенде) |
| 11 | частично | Темы через CSS-переменные, aria-метки строк и меню; пустые состояния и индикаторы загрузки есть |
| 12 | готово | `cargo test` 83 теста, Vitest: `utils/wsdl.test.ts` (16), `services/wsdl.test.ts` (16) |
| 13 | не начат | Ручной smoke на публичном WSDL в `npm run tauri dev` |


## 1. Цель

Добавить в приложение полноценный SOAP-клиент: импорт WSDL по URL из буфера обмена, хранение проектов/WSDL-файлов/сервисов/операций в SQLite, генерация шаблона SOAP body по XSD, редактирование HTTP-заголовков с заблокированными обязательными строками, выполнение запроса с отменой, просмотр и сохранение ответа.

## 2. Уточняющие вопросы (до 5) и допущения базового плана

Вопросы не блокируют старт: ниже зафиксированы ответы, принятые по умолчанию. Если ответ отличается — правится только соответствующий шаг.

| № | Вопрос | Почему критично | Допущение в плане |
| --- | --- | --- | --- |
| 1 | Поддерживаем только WSDL 1.1 (SOAP 1.1/1.2 bindings) или ещё WSDL 2.0? | WSDL 2.0 — другая модель (`service/endpoint/interface/operation`), отдельный парсер | Только WSDL 1.1 + SOAP 1.1/1.2. WSDL 2.0 → понятная ошибка импорта, вне scope |
| 2 | Загружать ли внешние `xsd:import`/`xsd:include` по сети? | Без них шаблон для схем с вынесенными типами будет неполным | v1: внешние схемы не загружаются, импорт показывает предупреждение «типы не разрешены», шаблон строится по доступным типам. Загрузка — отдельная фаза |
| 3 | Что делает «Выполнить» в меню файла? | В файле несколько операций | Выполняется выбранная операция этого файла; если ничего не выбрано — первая операция файла (сначала выбирается, затем выполняется) |
| 4 | Нужен ли браузерный fallback (fetch + DOMParser + localStorage)? | Удваивает парсер WSDL/XSD на TS и упирается в CORS | Desktop-only, как Share: в браузере показывается локализованная заглушка «доступно в desktop-версии». Fallback — вне scope |
| 5 | Какие заголовки считать обязательными и блокировать? | От этого зависит UX таблицы Headers | SOAP 1.1: `Content-Type: text/xml; charset=utf-8` + `SOAPAction: "<action>"`. SOAP 1.2: `Content-Type: application/soap+xml; charset=utf-8; action="<action>"`. Строки блокируются по имени (без учёта регистра), значения пересобираются перед отправкой |

Дополнительные допущения: проекты WSDL отдельные от REST (`wsdl_projects`, не переиспользуем `rest_projects`); обязательные заголовки не удаляются и не редактируются, остальные — обычные строки `PropertyEditor`; SOAP Header (WS-Security и т.п.) в v1 не генерируется; MTOM/attachments вне scope.

## 3. Границы

В scope:

- пункт навигации `WSDL` и маршрут `/wsdl`;
- проекты WSDL: создать / переименовать / удалить (как в REST);
- импорт WSDL по URL из буфера обмена, парсинг, дерево `WSDL file → service → operation`;
- контекстное меню файла `...`: «Выполнить», «Удалить»; выбор операции открывает её справа;
- endpoint-селектор по всем `soap:address location` операции;
- вкладки запроса `XML` (CodeMirror, шаблон из XSD), `Headers` (PropertyEditor с locked-строками), `Raw` (read-only envelope);
- выполнение/отмена через существующий Rust-клиент, ответ: `XML` / `Headers` / `Raw`, панель Status/Time/Size, `Save`, копирование, форматирование;
- сохранение состояния операции (endpoint, body, headers, save-флаг, ответ) в SQLite;
- восстановление выбранных проекта/операции между запусками;
- i18n en/ru, светлая/тёмная темы, ресайзеры, доступность с клавиатуры.

Вне scope v1: WSDL 2.0, внешние XSD, SOAP Header/WS-Security, MTOM/attachments, браузерный fallback, импорт WSDL из файла (только URL из буфера), авторизация импорта, синхронизация между устройствами.

## 4. Архитектура

```text
/wsdl/+page.svelte
  ├─ WsdlExplorer.svelte ── services/wsdl.ts ── invoke ── wsdl.rs ── db.rs (SQLite)
  ├─ utils/wsdl.ts (envelope, locked headers, endpoint choice)
  └─ execute_rest_request / cancel_rest_request / download_rest_response (reuse)
```

Ключевые решения:

1. **Парсинг только в Rust** (`quick-xml`). WSDL — недоверенный XML; frontend `DOMParser` не используется, чтобы исключить XXE/расхождения. `quick-xml` не разворачивает внешние сущности.
2. **HTTP-выполнение не дублируется**: запрос уходит через существующие `execute_rest_request` (POST, `body_type = "xml"`, готовый envelope в `body`, сгенерированные заголовки) и `cancel_rest_request`. Это сразу даёт таймаут 120 с, gzip, редиректы, отмену, статус/время/размер/заголовки/raw request и обработку бинарных ответов. Новый сетевой код для SOAP не пишется.
3. **SOAP Fault — это ответ, а не ошибка транспорта**: `execute_rest_request` возвращает HTTP 500 с телом; UI показывает статус 500 и XML fault. Ошибкой показывается только сетевая ошибка/отмена.
4. **Шаблон body и envelope разделены**: в БД и во вкладке `XML` хранится body-фрагмент (то, что пользователь редактирует), envelope собирается клиентом (`utils/wsdl.ts`) для `Raw` и для отправки. Так шаблон остаётся читаемым, а `Raw` всегда соответствует актуальному body.
5. **Стабильные идентификаторы операций**: `operation.id` детерминирован от `(file_id, service_name, operation_name)`, поэтому повторный импорт того же URL обновляет дерево и шаблоны, не теряя сохранённое состояние операции.
6. **Никаких новых npm/cargo-зависимостей**: `quick-xml`, `reqwest`, `rusqlite`, `serde`, CodeMirror, Tailwind уже есть.

## 5. Модель данных (SQLite)

Добавляется в `db.rs::init_schema` идемпотентно (`CREATE TABLE IF NOT EXISTS`), без фреймворка миграций.

```sql
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
  updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_wsdl_files_project ON wsdl_files(project_id);
CREATE INDEX IF NOT EXISTS idx_wsdl_services_file ON wsdl_services(file_id);
CREATE INDEX IF NOT EXISTS idx_wsdl_operations_service ON wsdl_operations(service_id);
CREATE INDEX IF NOT EXISTS idx_wsdl_endpoints_operation ON wsdl_endpoints(operation_id);
```

Правила хранения:

- `headers` — JSON-массив `{name, value}`. Locked-строки не хранятся флагом, а вычисляются по имени и SOAP-версии выбранного endpoint; при загрузке и перед отправкой они добавляются/перезаписываются каноническими значениями, пользовательские дубликаты с тем же именем отбрасываются.
- `wsdl_operation_state` создаётся лениво при первом открытии операции: `request_body = input_template`, `endpoint_id` — первый endpoint, `headers` — сгенерированные locked-строки.
- Удаление проекта/файла выполняется в транзакции и удаляет дочерние строки вручную (как в REST).
- Настройки выбора: `app_settings` `wsdl.activeProject`, `wsdl.activeOperation` (пустая строка = ничего не выбрано); в браузере — `localStorage` через `get_setting`/`set_setting`.
- `parse_warnings` — JSON-массив строк (например, «xsd:import не загружен: ns»), показывается в help-подсказке импорта.

## 6. Tauri-команды

Все — `#[tauri::command(rename = "...", rename_all = "snake_case")]`, обёртки `cmd_*` в `wsdl.rs` (как в `rest.rs`, чтобы не конфликтовать с именами DB-хелперов).

| Команда | Аргументы | Возвращает |
| --- | --- | --- |
| `list_wsdl_projects` | — | `WsdlProject[]` |
| `create_wsdl_project` | `name` | `WsdlProject` |
| `rename_wsdl_project` | `id`, `name` | `void` |
| `delete_wsdl_project` | `id` | `void` |
| `list_wsdl_files` | — | `WsdlFileSummary[]` (файл + services + operations без шаблонов) |
| `import_wsdl` | `project_id`, `url` | `WsdlImportResult` (summary + `warnings`) |
| `delete_wsdl_file` | `id` | `void` |
| `get_wsdl_operation` | `operation_id` | `WsdlOperationDetail` (операция + endpoints + state/дефолты) |
| `save_wsdl_operation_state` | `operation_id`, `endpoint_id`, `request_body`, `headers`, `save_response` | `void` |
| `save_wsdl_response` | `operation_id`, `body`, `headers`, `raw_request` | `void` |
| `clear_wsdl_response` | `operation_id` | `void` |

Выполнение и скачивание ответа: переиспользуются `execute_rest_request`, `cancel_rest_request`, `download_rest_response` без изменений контракта.

`WsdlOperationDetail` содержит `soap_version`, `soap_action`, `input_template`, `input_element`, `output_element`, `endpoints[]`, `state`.

## 7. Файловая карта

Новые файлы:

| Файл | Назначение |
| --- | --- |
| `src/routes/wsdl/+page.svelte` | Страница: layout, ресайзеры, вкладки, выполнение, ответ |
| `src/lib/components/WsdlExplorer.svelte` | Проект, импорт, help, дерево, контекстные меню |
| `src/lib/services/wsdl.ts` | IPC-обёртки, `isWsdlSupported()`, восстановление выбора |
| `src/lib/services/wsdl.test.ts` | `resolveWsdlSelection`, сериализация состояния |
| `src/lib/utils/wsdl.ts` | Envelope, locked headers, выбор endpoint, коды ошибок/warnings → i18n-ключи |
| `src/lib/utils/wsdl.test.ts` | Envelope/headers/endpoint/маппинг кодов ошибок и warnings |
| `src-tauri/src/wsdl/mod.rs` | Модели, команды, регистрация |
| `src-tauri/src/wsdl/parser.rs` | WSDL 1.1: definitions/messages/portType/binding/service |
| `src-tauri/src/wsdl/xsd.rs` | Индекс XSD и генерация шаблона body |
| `src-tauri/src/wsdl/store.rs` | SQLite CRUD, транзакции, ленивое состояние |

Изменяемые файлы:

| Файл | Изменение |
| --- | --- |
| `src/lib/components/Navbar.svelte` | пункт `{ path: '/wsdl', label: 'nav.wsdl' }` |
| `src/lib/components/PropertyEditor.svelte` | проп `locked?: (row, index) => boolean`: input readonly, нет `trash`, нет drag; канонические значения не редактируются |
| `src/lib/components/Icon.svelte` | иконки `help` (справка импорта) и `warning` (предупреждения парсинга) |
| `src/lib/stores/i18n.ts` | `nav.wsdl`, блок `wsdl.*` в en и ru |
| `src-tauri/src/db.rs` | таблицы/индексы + тесты схемы |
| `src-tauri/src/lib.rs` | `mod wsdl;` + `generate_handler![...]` |
| `src-tauri/src/rest.rs` | не изменяется: переиспользуются `pub fn normalize_rest_url` и `pub const DEFAULT_USER_AGENT`, импортный клиент со своим таймаутом 30 с создаётся в `wsdl/parser.rs` |
| `README.md` | раздел «WSDL-клиент»: поведение, критерии, ограничения |
| `AGENTS.md` | карта файлов, таблицы, команды, ключи настроек, инварианты |

## 8. Пошаговый план

| Шаг | Действие | Результат/критерий готовности | Оценка времени | Риски |
| --- | --- | --- | --- | --- |
| 1 | Зафиксировать контракт в `README.md` (раздел WSDL) и карту в `AGENTS.md` по шаблону «Active feature» | Раздел описывает UX, состояния, хранение, критерии, ограничения; AGENTS содержит маршрут/компоненты/таблицы/команды | 2–3 ч | Требования readme неполны (см. вопросы) — закрыть допущениями и явно записать в README |
| 2 | Спроектировать и добавить схему SQLite + CRUD в `wsdl/store.rs`, тесты на in-memory БД | Таблицы создаются идемпотентно; create/rename/delete/list проектов; импорт-вставка файла с деревом; удаление каскадом; повторный импорт сохраняет state | 8–10 ч | Потеря состояния при повторном импорте; забыть каскад; конфликт `UNIQUE(project_id, source_url)` |
| 3 | Реализовать загрузку WSDL: URL-валидация, HTTPS/HTTP, таймаут 30 с, лимит 5 МиБ, парсинг `quick-xml` в модели `file → service → operation → endpoints` | Публичный WSDL импортируется; битый XML/404/таймаут → понятная ошибка без записи в БД; WSDL 2.0 и «нет операций» отклоняются | 16–20 ч | Разные диалекты (`wsdl:`/`def:`), несколько `port`/`binding`, SOAP 1.2 namespace, огромные файлы, сеть в тестах |
| 4 | Индекс XSD и генератор шаблона body: `element/complexType/simpleType/sequence/all/choice/attributes`, `minOccurs/maxOccurs`, `ref`, `nillable`, `elementFormDefault`, защита от рекурсии | Для тестовых схем генерируется ожидаемый XML; глубина ≤ 32, элементов ≤ 2000; `maxOccurs>1` → один образец + комментарий; неразрешённые типы → комментарий + warning, без падения | 16–24 ч | Самая рискованная часть: расширения/ограничения, substitution groups, `xsd:any`, циклические типы, пространства имён |
| 5 | Добавить Tauri-команды, модели serde, регистрацию в `lib.rs`, `services/wsdl.ts` с `isWsdlSupported()` | `npm run check` проходит; invoke-имена snake_case; в браузере сервис не падает, страница показывает заглушку | 6–8 ч | Расхождение имён аргументов; забыть `mod wsdl;`; большой payload дерева |
| 6 | Добавить маршрут `/wsdl`, пункт навигации, i18n-ключи, каркас страницы (explorer + правая область, два ресайзера) | Кнопка WSDL в навбаре активна на `/wsdl`; layout повторяет REST (h-screen, min-h-0, resizer-v); en/ru | 4–6 ч | Сломать высоты/overflow; навбар-скроллбар (держать `overflow-y-hidden py-1`) |
| 7 | `WsdlExplorer.svelte`: селект проектов + меню create/rename/delete, кнопка «Paste WSDL url from clipboard» + help, дерево с раскрытием, `...` у файла (Выполнить/Удалить), выбор операции, восстановление выбора | Импорт из буфера создаёт файл и дерево; выбор переживает перезапуск; удаление с подтверждением; «Выполнить» запускает выбранную/первую операцию | 10–12 ч | Буфер обмена пуст/не URL; дубликаты URL; длинные имена; конфликт с контекстным меню |
| 8 | Правая панель: endpoint-селектор, кнопка-глобус (выполнение/отмена), генерация locked-заголовков, `PropertyEditor` с `locked`, вкладка `Headers` | Endpoint-список из всех `soap:address`; locked-строки нельзя менять/удалять; смена endpoint меняет SOAP-версию и locked-набор; пустой endpoint блокирует запуск с тостом | 8–10 ч | Пользовательский дубликат `Content-Type`; SOAP 1.2 action в media type; регистр имён |
| 9 | Вкладки `XML` и `Raw` запроса: CodeMirror, шаблон из операции, сборка envelope, read-only preview, кнопка сброса к шаблону (опционально) | Правка body сохраняется; `Raw` показывает envelope с актуальным body; форматирование XML работает; пустой body не отправляется | 6–8 ч | Namespace-префиксы; повторная генерация затирает правки; размер шаблона |
| 10 | Ответ: вкладки `XML`/`Headers`/`Raw`, панель Status/Time/Size, `Save`, Copy, Format, скачивание бинарного/attachment-ответа; SOAP Fault как ответ | 200 и 500 с fault отображаются одинаково корректно; Save сохраняет и восстанавливает ответ; Copy/Format работают; отмена показывает тост | 6–8 ч | Ответ не XML; бинарный ответ; charset; повторное использование `formatContent` на fault |
| 11 | Доводка UI: темы, hover/active-состояния, доступность (aria, клавиатура), пустые состояния, индикаторы загрузки, подтверждения удаления | Нет хардкод-цветов; фокус видим; пустые состояния локализованы; вёрстка не прыгает при переключении вкладок | 4–6 ч | Расхождение с макетом; регресс навбара/ресайзеров |
| 12 | Тесты: Rust (парсер, XSD-шаблон, store), Vitest (envelope, headers, endpoint, selection), фикстуры WSDL 1.1/1.2 | `cargo test`, `npm run test` зелёные; покрыты malformed WSDL, несколько endpoints, SOAP 1.2, рекурсивный тип, повторный импорт | 10–14 ч | Фикстуры с внешними XSD; сетевые тесты нестабильны — только офлайн-фикстуры + ручной smoke |
| 13 | Ручной smoke на публичных WSDL, финальные `npm run check`, `npm run build`, `cargo fmt --check`, обновление README/AGENTS, снятие пометки «Active feature» | Чек-лист приёмки выполнен; документация соответствует коду; артефакты не закоммичены | 3–4 ч | Внешние сервисы недоступны/меняются; блокировка порта 1420 при `tauri dev` |

Итого: **99–123 ч** (~13–16 рабочих дней одного разработчика). Критический путь: шаги 3–4 (парсер и XSD), их стоит начинать первыми после схемы БД.

## 9. Критерии приёмки

1. В навбаре есть пункт `WSDL`; переход на `/wsdl` подсвечивает его, как остальные разделы.
2. Проекты WSDL создаются, переименовываются и удаляются; выбранные проект и операция восстанавливаются после перезапуска приложения.
3. Кнопка импорта читает URL из буфера обмена, импортирует WSDL и строит дерево `файл → service → operation`; невалидный URL, 404, таймаут и битый XML дают локализованную ошибку и не создают запись.
4. Повторный импорт того же URL обновляет дерево/шаблоны, но не затирает сохранённые body, headers, endpoint и ответ операции.
5. Выбор операции показывает все её `soap:address location` в выпадающем списке и генерирует шаблон body по XSD.
6. Вкладка `Headers` содержит `Content-Type` и `SOAPAction` (SOAP 1.1) или `Content-Type` с `action` (SOAP 1.2); эти строки нельзя редактировать и удалять, остальные строки редактируются.
7. Вкладка `Raw` запроса показывает корректный envelope с текущим body; envelope меняется при правке body.
8. Кнопка-глобус отправляет POST; во время запроса показывает вращение; повторное нажатие отменяет запрос и показывает уведомление.
9. Ответ показывает Status, Time, Size; `XML`/`Headers`/`Raw` заполняются; `Format` форматирует XML (включая fault), `Copy` копирует, `Save` сохраняет ответ и восстанавливает его после перезапуска.
10. HTTP 500 с `soap:Fault` отображается как ответ со статусом 500 и XML тела, а не как сетевая ошибка.
11. Контекстное меню файла `...` содержит «Выполнить» и «Удалить»; удаление требует подтверждения и удаляет файл, сервисы, операции, endpoints и состояния.
12. Все строки локализованы в en и ru; светлая и тёмная темы не имеют хардкод-цветов; интерактивные элементы доступны с клавиатуры.
13. В браузерной сборке страница показывает заглушку «доступно в desktop-версии» и не импортирует Tauri-команды.
14. `npm run test`, `npm run check`, `npm run build`, `cargo test`, `cargo fmt --check` проходят.

## 10. Проверка

```bash
npm run test
npm run check
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo fmt --check --manifest-path src-tauri/Cargo.toml
npm run tauri dev
```

Ручной smoke (desktop): импорт `https://www.dataaccess.com/webservicesserver/NumberConversion.wso?WSDL` (или другого публичного doc/lit WSDL), выбор операции, проверка шаблона, выполнение, ответ, сохранение, перезапуск. Дополнительно: WSDL с SOAP 1.2 binding, WSDL с несколькими `port`, заведомо битый XML, URL без схемы.

## 11. Риски и меры

| Риск | Вероятность | Влияние | Мера |
| --- | --- | --- | --- |
| Сложность XSD: расширения, `choice`, `xsd:any`, рекурсия | Высокая | Шаблон неполный/невалидный | Ограничить v1 базовыми конструкциями, не падать, помечать неразрешённое комментарием и warning; тесты на фикстурах |
| Внешние `xsd:import/include` | Высокая | Пустые/частичные шаблоны | v1 — warning и понятное сообщение; отдельная фаза с кэшем и лимитами |
| Разные диалекты WSDL (префиксы, несколько bindings, SOAP 1.1+1.2) | Средняя | Неверный endpoint/заголовки | Нормализация по namespace URI, а не по префиксу; endpoint хранит `soap_version`; тесты на оба binding |
| Повторный импорт затирает правки | Средняя | Потеря работы пользователя | Стабильные id операций, обновление только шаблонов/дерева, state не трогается |
| `Content-Type`/`SOAPAction` продублированы пользователем | Средняя | Неверный запрос | Канонические locked-строки пересобираются перед отправкой, дубликаты отбрасываются |
| SOAP Fault принимается за ошибку | Средняя | Пользователь не видит причину | Использовать `RestResponse` со статусом 500; тест на fault-ответ |
| XXE/огромный WSDL | Низкая | Безопасность/память | Парсинг только `quick-xml` без внешних сущностей; лимит 5 МиБ; таймаут 30 с |
| Буфер обмена недоступен/пуст | Средняя | Импорт не работает | Тосты с причиной; повторное чтение по кнопке; в desktop — `clipboard-manager` |
| Браузерная сборка ломается из-за Tauri-импортов | Низкая | Падение страницы | `isWsdlSupported()` + динамический импорт, как в updater/share |
| Регресс навбара/ресайзеров | Низкая | Визуальные дефекты | Переиспользовать классы `resizer-v`, `overflow-y-hidden py-1`, проверить обе темы |

## 12. Порядок поставки

- **M1. Бэкенд импорта** (шаги 2–5): схема, парсер, XSD-шаблон, команды. Демо: импорт WSDL и чтение дерева через invoke.
- **M2. Читающий UI** (шаги 6–7): навигация, explorer, выбор операции, шаблон и endpoint без выполнения.
- **M3. Выполнение SOAP** (шаги 8–10): headers с locked, envelope, запуск/отмена, ответ, сохранение.
- **M4. Полировка и приёмка** (шаги 11–13): темы, a11y, тесты, smoke, документация.
