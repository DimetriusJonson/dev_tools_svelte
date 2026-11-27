use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::header::{HeaderName, HeaderValue};
use reqwest::{Client, Method, StatusCode, Url, Version};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::db::AppDb;
use crate::vars::{self, VarValues};

pub const DEFAULT_USER_AGENT: &str = "WebDevUsefulTools Client";
const REQUEST_TIMEOUT_SECS: u64 = 120;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestProperty {
    pub name: String,
    pub value: String,
}

/// A `multipart/form-data` file part. Only the picked path and the display name are
/// stored; the bytes are read from disk at send time and never enter the database.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestFileRow {
    pub name: String,
    pub path: String,
    pub filename: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestProject {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestRequest {
    pub id: String,
    pub project_id: Option<String>,
    pub name: String,
    pub method: String,
    pub url: String,
    pub body: String,
    pub body_type: String,
    pub save_response: bool,
    pub headers: Vec<RestProperty>,
    pub form: Vec<RestProperty>,
    #[serde(default)]
    pub multipart: Vec<RestProperty>,
    #[serde(default)]
    pub files: Vec<RestFileRow>,
    pub response_body: Option<String>,
    pub response_headers: Vec<RestProperty>,
    pub raw_request: Option<String>,
    /// The status line of the stored answer (`HTTP/1.1 200 OK`), so the Raw response
    /// tab can rebuild the dump after a restart without re-sending the request.
    pub response_status_line: Option<String>,
    /// Tabs that were open when this request was last edited: the request panel, the
    /// `Form` sub-tables and the response panel.
    pub request_tab: String,
    pub multipart_tab: String,
    pub response_tab: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RestRequestDraft {
    pub project_id: Option<String>,
    pub name: String,
    pub method: String,
    pub url: String,
    pub body: String,
    pub body_type: String,
    pub save_response: bool,
    pub headers: Vec<RestProperty>,
    pub form: Vec<RestProperty>,
    #[serde(default)]
    pub multipart: Vec<RestProperty>,
    #[serde(default)]
    pub files: Vec<RestFileRow>,
    /// Optional for callers written before the tabs were stored: an absent code reads as
    /// the first tab of its panel.
    #[serde(default)]
    pub request_tab: String,
    #[serde(default)]
    pub multipart_tab: String,
    #[serde(default)]
    pub response_tab: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RestRequestUpdate {
    pub id: String,
    pub project_id: Option<String>,
    pub name: String,
    pub method: String,
    pub url: String,
    pub body: String,
    pub body_type: String,
    pub save_response: bool,
    pub headers: Vec<RestProperty>,
    pub form: Vec<RestProperty>,
    #[serde(default)]
    pub multipart: Vec<RestProperty>,
    #[serde(default)]
    pub files: Vec<RestFileRow>,
    #[serde(default)]
    pub request_tab: String,
    #[serde(default)]
    pub multipart_tab: String,
    #[serde(default)]
    pub response_tab: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RestResponse {
    pub status: u16,
    pub status_text: String,
    /// `HTTP/1.1 200 OK` — the status line as the server sent it, version included.
    pub status_line: String,
    pub headers: Vec<RestProperty>,
    pub body: String,
    pub size: u64,
    pub duration_ms: f64,
    pub content_type: String,
    pub language: Option<String>,
    pub is_attachment: bool,
    pub filename: Option<String>,
    pub is_binary: bool,
    pub raw_request: String,
}

#[derive(Clone, Default)]
pub struct RestCancelState {
    inner: Arc<Mutex<CancelRegistry>>,
}

#[derive(Default)]
struct CancelRegistry {
    next_id: u64,
    running: HashMap<u64, oneshot::Sender<()>>,
}

impl RestCancelState {
    /// Registers a call and answers its id plus the signal to await. Every call owns a
    /// slot of its own: starting a second request must not drop the first request's
    /// sender, because a dropped sender wakes the first request as "aborted".
    fn begin(&self) -> (u64, oneshot::Receiver<()>) {
        let (sender, receiver) = oneshot::channel();
        let mut registry = self.registry();
        let id = registry.next_id;
        registry.next_id = registry.next_id.wrapping_add(1);
        registry.running.insert(id, sender);
        (id, receiver)
    }

    /// Drops only the slot of the call that created it, so a request that finishes
    /// never aborts a call started after it.
    fn clear(&self, id: u64) {
        self.registry().running.remove(&id);
    }

    /// Aborts every call still in flight and reports whether anything was running.
    fn cancel(&self) -> bool {
        let senders = {
            let mut registry = self.registry();
            std::mem::take(&mut registry.running)
        };
        senders
            .into_values()
            .map(|sender| sender.send(()).is_ok())
            .fold(false, |cancelled, sent| cancelled || sent)
    }

    fn registry(&self) -> std::sync::MutexGuard<'_, CancelRegistry> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn next_sort_order(conn: &Connection, table: &str) -> rusqlite::Result<i64> {
    let sql = format!("SELECT COALESCE(MAX(sort_order), -1) + 1 FROM {table}");
    conn.query_row(&sql, [], |row| row.get(0))
}

/// Tab codes are stored lowercase. A value outside the panel's own list — an empty code
/// from a draft written before the tabs were stored, or a hand-edited row — reads as the
/// first tab, so the panel never renders a tab that does not exist.
fn normalize_tab(value: &str, allowed: &[&str], fallback: &str) -> String {
    let code = value.trim().to_ascii_lowercase();
    match allowed.iter().find(|candidate| **candidate == code) {
        Some(candidate) => candidate.to_string(),
        None => fallback.to_string(),
    }
}

pub fn normalize_request_tab(value: &str) -> String {
    normalize_tab(value, &["headers", "query", "cookies", "raw"], "headers")
}

pub fn normalize_multipart_tab(value: &str) -> String {
    normalize_tab(value, &["fields", "files"], "fields")
}

pub fn normalize_response_tab(value: &str) -> String {
    normalize_tab(value, &["response", "headers", "raw"], "response")
}

fn properties_for(
    conn: &Connection,
    request_id: &str,
    kind: &str,
) -> rusqlite::Result<Vec<RestProperty>> {
    let mut statement = conn.prepare(
        "SELECT name, value FROM rest_request_properties WHERE request_id = ?1 AND kind = ?2 ORDER BY sort_order, id",
    )?;
    let rows = statement
        .query_map(params![request_id, kind], |row| {
            Ok(RestProperty {
                name: row.get(0)?,
                value: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn write_properties(
    conn: &Connection,
    request_id: &str,
    kind: &str,
    rows: &[RestProperty],
) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM rest_request_properties WHERE request_id = ?1 AND kind = ?2",
        params![request_id, kind],
    )?;
    let mut statement = conn.prepare(
        "INSERT INTO rest_request_properties (request_id, kind, name, value, sort_order) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for (index, property) in rows.iter().enumerate() {
        if property.name.trim().is_empty() {
            continue;
        }
        statement.execute(params![
            request_id,
            kind,
            property.name,
            property.value,
            index as i64
        ])?;
    }
    Ok(())
}

fn form_for(conn: &Connection, request_id: &str) -> rusqlite::Result<Vec<RestProperty>> {
    let mut statement = conn.prepare(
        "SELECT name, value FROM rest_request_form WHERE request_id = ?1 ORDER BY sort_order, id",
    )?;
    let rows = statement
        .query_map(params![request_id], |row| {
            Ok(RestProperty {
                name: row.get(0)?,
                value: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn write_form(conn: &Connection, request_id: &str, rows: &[RestProperty]) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM rest_request_form WHERE request_id = ?1",
        params![request_id],
    )?;
    let mut statement = conn.prepare(
        "INSERT INTO rest_request_form (request_id, name, value, sort_order) VALUES (?1, ?2, ?3, ?4)",
    )?;
    for (index, property) in rows.iter().enumerate() {
        if property.name.trim().is_empty() {
            continue;
        }
        statement.execute(params![
            request_id,
            property.name,
            property.value,
            index as i64
        ])?;
    }
    Ok(())
}

/// The Form tab's field table, stored apart from the Form-encode rows so switching
/// between the two tabs never rewrites either one.
fn multipart_for(conn: &Connection, request_id: &str) -> rusqlite::Result<Vec<RestProperty>> {
    let mut statement = conn.prepare(
        "SELECT name, value FROM rest_request_multipart WHERE request_id = ?1 ORDER BY sort_order, id",
    )?;
    let rows = statement
        .query_map(params![request_id], |row| {
            Ok(RestProperty {
                name: row.get(0)?,
                value: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn write_multipart(
    conn: &Connection,
    request_id: &str,
    rows: &[RestProperty],
) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM rest_request_multipart WHERE request_id = ?1",
        params![request_id],
    )?;
    let mut statement = conn.prepare(
        "INSERT INTO rest_request_multipart (request_id, name, value, sort_order) VALUES (?1, ?2, ?3, ?4)",
    )?;
    for (index, property) in rows.iter().enumerate() {
        if property.name.trim().is_empty() {
            continue;
        }
        statement.execute(params![
            request_id,
            property.name,
            property.value,
            index as i64
        ])?;
    }
    Ok(())
}

fn files_for(conn: &Connection, request_id: &str) -> rusqlite::Result<Vec<RestFileRow>> {
    let mut statement = conn.prepare(
        "SELECT name, path, filename FROM rest_request_files WHERE request_id = ?1 ORDER BY sort_order, id",
    )?;
    let rows = statement
        .query_map(params![request_id], |row| {
            Ok(RestFileRow {
                name: row.get(0)?,
                path: row.get(1)?,
                filename: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// File rows keep only the path and the display name, so a row whose name is blank has
/// nothing to attach to and is not stored — the client reports it as `name-required`.
fn write_files(conn: &Connection, request_id: &str, rows: &[RestFileRow]) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM rest_request_files WHERE request_id = ?1",
        params![request_id],
    )?;
    let mut statement = conn.prepare(
        "INSERT INTO rest_request_files (request_id, name, path, filename, sort_order) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for (index, file) in rows.iter().enumerate() {
        if file.name.trim().is_empty() {
            continue;
        }
        statement.execute(params![
            request_id,
            file.name,
            file.path,
            file.filename,
            index as i64
        ])?;
    }
    Ok(())
}

pub fn read_rest_request(conn: &Connection, id: &str) -> rusqlite::Result<Option<RestRequest>> {
    let mut statement = conn.prepare(
        r#"
		SELECT id, project_id, name, method, url, body, body_type, save_response,
		       response_body, response_headers, raw_request, created_at, updated_at,
		       response_status_line, request_tab, multipart_tab, response_tab
		FROM rest_requests WHERE id = ?1
		"#,
    )?;
    let request = statement
        .query_row(params![id], |row| {
            let response_headers: String = row.get(9)?;
            Ok(RestRequest {
                id: row.get(0)?,
                project_id: row.get(1)?,
                name: row.get(2)?,
                method: row.get(3)?,
                url: row.get(4)?,
                body: row.get(5)?,
                body_type: row.get(6)?,
                save_response: row.get::<_, i64>(7)? != 0,
                response_body: row.get(8)?,
                response_headers: serde_json::from_str(&response_headers).unwrap_or_default(),
                headers: Vec::new(),
                form: Vec::new(),
                multipart: Vec::new(),
                files: Vec::new(),
                raw_request: row.get(10)?,
                response_status_line: row.get(13)?,
                request_tab: normalize_request_tab(&row.get::<_, String>(14)?),
                multipart_tab: normalize_multipart_tab(&row.get::<_, String>(15)?),
                response_tab: normalize_response_tab(&row.get::<_, String>(16)?),
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        })
        .optional()?;

    match request {
        Some(mut request) => {
            request.headers = properties_for(conn, id, "header")?;
            request.form = form_for(conn, id)?;
            request.multipart = multipart_for(conn, id)?;
            request.files = files_for(conn, id)?;
            Ok(Some(request))
        }
        None => Ok(None),
    }
}

pub fn list_rest_projects(conn: &Connection) -> rusqlite::Result<Vec<RestProject>> {
    let mut statement = conn.prepare(
        "SELECT id, name, created_at FROM rest_projects ORDER BY sort_order, created_at, id",
    )?;
    let projects = statement
        .query_map([], |row| {
            Ok(RestProject {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(projects)
}

pub fn create_rest_project(conn: &Connection, name: &str) -> Result<RestProject, String> {
    let id = Uuid::new_v4().to_string();
    let sort_order = next_sort_order(conn, "rest_projects").map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT INTO rest_projects (id, name, sort_order) VALUES (?1, ?2, ?3)",
        params![id, name, sort_order],
    )
    .map_err(|error| error.to_string())?;
    conn.query_row(
        "SELECT id, name, created_at FROM rest_projects WHERE id = ?1",
        params![id],
        |row| {
            Ok(RestProject {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
            })
        },
    )
    .map_err(|error| error.to_string())
}

pub fn rename_rest_project(conn: &Connection, id: &str, name: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE rest_projects SET name = ?1 WHERE id = ?2",
        params![name, id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn delete_rest_project(conn: &Connection, id: &str) -> Result<(), String> {
    let transaction = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "UPDATE rest_requests SET project_id = NULL, updated_at = CURRENT_TIMESTAMP WHERE project_id = ?1",
            params![id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute("DELETE FROM rest_projects WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(())
}

pub fn list_rest_requests(conn: &Connection) -> Result<Vec<RestRequest>, String> {
    let mut statement = conn
        .prepare("SELECT id FROM rest_requests ORDER BY sort_order, created_at, id")
        .map_err(|error| error.to_string())?;
    let ids = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;

    let mut requests = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(request) = read_rest_request(conn, &id).map_err(|error| error.to_string())? {
            requests.push(request);
        }
    }
    Ok(requests)
}

pub fn create_rest_request(
    conn: &Connection,
    draft: &RestRequestDraft,
) -> Result<RestRequest, String> {
    let id = Uuid::new_v4().to_string();
    let sort_order = next_sort_order(conn, "rest_requests").map_err(|error| error.to_string())?;
    let transaction = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            r#"
		INSERT INTO rest_requests
			(id, project_id, name, method, url, body, body_type, save_response, sort_order,
			 request_tab, multipart_tab, response_tab)
		VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
		"#,
            params![
                id,
                draft.project_id,
                draft.name,
                draft.method,
                draft.url,
                draft.body,
                draft.body_type,
                i64::from(draft.save_response),
                sort_order,
                normalize_request_tab(&draft.request_tab),
                normalize_multipart_tab(&draft.multipart_tab),
                normalize_response_tab(&draft.response_tab),
            ],
        )
        .map_err(|error| error.to_string())?;
    write_properties(&transaction, &id, "header", &draft.headers)
        .map_err(|error| error.to_string())?;
    write_form(&transaction, &id, &draft.form).map_err(|error| error.to_string())?;
    write_multipart(&transaction, &id, &draft.multipart).map_err(|error| error.to_string())?;
    write_files(&transaction, &id, &draft.files).map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;

    read_rest_request(conn, &id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Could not read the created request".to_string())
}

pub fn update_rest_request(conn: &Connection, update: &RestRequestUpdate) -> Result<(), String> {
    let transaction = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    let updated = transaction
        .execute(
            r#"
		UPDATE rest_requests SET
			project_id = ?2,
			name = ?3,
			method = ?4,
			url = ?5,
			body = ?6,
			body_type = ?7,
			save_response = ?8,
			request_tab = ?9,
			multipart_tab = ?10,
			response_tab = ?11,
			updated_at = CURRENT_TIMESTAMP
		WHERE id = ?1
		"#,
            params![
                update.id,
                update.project_id,
                update.name,
                update.method,
                update.url,
                update.body,
                update.body_type,
                i64::from(update.save_response),
                normalize_request_tab(&update.request_tab),
                normalize_multipart_tab(&update.multipart_tab),
                normalize_response_tab(&update.response_tab)
            ],
        )
        .map_err(|error| error.to_string())?;
    // Child rows belong to a request that exists: writing them for an unknown id would
    // leave rows nothing can read or delete.
    if updated == 0 {
        return Err("Request not found".to_string());
    }
    write_properties(&transaction, &update.id, "header", &update.headers)
        .map_err(|error| error.to_string())?;
    write_form(&transaction, &update.id, &update.form).map_err(|error| error.to_string())?;
    write_multipart(&transaction, &update.id, &update.multipart)
        .map_err(|error| error.to_string())?;
    write_files(&transaction, &update.id, &update.files).map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(())
}

pub fn rename_rest_request(conn: &Connection, id: &str, name: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE rest_requests SET name = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
        params![name, id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn delete_rest_request(conn: &Connection, id: &str) -> Result<(), String> {
    let transaction = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM rest_request_properties WHERE request_id = ?1",
            params![id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM rest_request_form WHERE request_id = ?1",
            params![id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM rest_request_multipart WHERE request_id = ?1",
            params![id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM rest_request_files WHERE request_id = ?1",
            params![id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute("DELETE FROM rest_requests WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(())
}

pub fn duplicate_rest_request(conn: &Connection, id: &str) -> Result<RestRequest, String> {
    let source = read_rest_request(conn, id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Request not found".to_string())?;
    let name = if source.name.trim().is_empty() {
        source.url.clone()
    } else {
        format!("{} copy", source.name)
    };
    let draft = RestRequestDraft {
        project_id: source.project_id,
        name,
        method: source.method,
        url: source.url,
        body: source.body,
        body_type: source.body_type,
        save_response: source.save_response,
        headers: source.headers,
        form: source.form,
        multipart: source.multipart,
        files: source.files,
        request_tab: source.request_tab,
        multipart_tab: source.multipart_tab,
        response_tab: source.response_tab,
        ..Default::default()
    };
    create_rest_request(conn, &draft)
}

pub fn save_rest_response(
    conn: &Connection,
    id: &str,
    body: &str,
    headers: &[RestProperty],
    raw_request: &str,
    status_line: &str,
) -> Result<(), String> {
    let encoded = serde_json::to_string(headers).map_err(|error| error.to_string())?;
    // An empty status line is "no status known", which is NULL rather than ''.
    let line = if status_line.is_empty() {
        None
    } else {
        Some(status_line)
    };
    conn.execute(
        r#"
		UPDATE rest_requests SET
			response_body = ?2,
			response_headers = ?3,
			raw_request = ?4,
			response_status_line = ?5,
			updated_at = CURRENT_TIMESTAMP
		WHERE id = ?1
		"#,
        params![id, body, encoded, raw_request, line],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn clear_rest_response(conn: &Connection, id: &str) -> Result<(), String> {
    conn.execute(
        r#"
		UPDATE rest_requests SET
			response_body = NULL,
			response_headers = '[]',
			raw_request = NULL,
			response_status_line = NULL,
			updated_at = CURRENT_TIMESTAMP
		WHERE id = ?1
		"#,
        params![id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// True when the text starts with `scheme://`, the same rule as
/// `services/rest.ts::normalizeUrl`. A plain `contains("://")` search would also match
/// a query value such as `?next=http://y` and hand `Url::parse` a bare host, which it
/// rejects as a relative URL.
fn has_url_scheme(value: &str) -> bool {
    let bytes = value.as_bytes();
    if !matches!(bytes.first(), Some(byte) if byte.is_ascii_alphabetic()) {
        return false;
    }
    let mut index = 1;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b':' {
            return bytes.get(index + 1) == Some(&b'/') && bytes.get(index + 2) == Some(&b'/');
        }
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.')) {
            return false;
        }
        index += 1;
    }
    false
}

pub fn normalize_rest_url(raw: &str) -> Result<Url, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Enter a request URL".to_string());
    }
    let candidate = if has_url_scheme(trimmed) {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };
    let url = Url::parse(&candidate).map_err(|error| format!("Invalid URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Only http and https URLs are supported".to_string());
    }
    if url.host_str().is_none() {
        return Err("The URL has no host".to_string());
    }
    Ok(url)
}

pub fn language_for_content_type(content_type: &str) -> Option<String> {
    let value = content_type.to_ascii_lowercase();
    if value.is_empty() {
        return None;
    }
    // The same rule as `detectLanguageFromContentType`: every subtype that says HTML is
    // HTML-flavoured, checked before XML so `application/xhtml+xml` never becomes XML.
    if value.contains("html") {
        return Some("html".to_string());
    }
    if value.contains("json") {
        return Some("json".to_string());
    }
    if value.contains("xml") {
        return Some("xml".to_string());
    }
    if value.starts_with("text/")
        || value.contains("javascript")
        || value.contains("ecmascript")
        || value.contains("x-www-form-urlencoded")
        || value.contains("yaml")
        || value.contains("csv")
        || value.contains("svg")
    {
        return Some("text".to_string());
    }
    None
}

pub fn language_for_body(content_type: &str, body: &str) -> Option<String> {
    if let Some(language) = language_for_content_type(content_type) {
        return Some(language);
    }
    let trimmed = body.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with('<') {
        // An HTML tag or an HTML doctype is HTML even without a `text/html` header;
        // other markup is better read by the XML language. Mirrors `detectLanguageFromHeaders`.
        let lower = trimmed.to_ascii_lowercase();
        if lower.contains("<html") || lower.starts_with("<!doctype html") {
            return Some("html".to_string());
        }
        return Some("xml".to_string());
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Some("json".to_string());
    }
    Some("text".to_string())
}

pub fn is_text_content_type(content_type: &str) -> bool {
    if content_type.trim().is_empty() {
        return true;
    }
    language_for_content_type(content_type).is_some()
}

/// Value of one `Content-Disposition` parameter, honouring a quoted string so a `;`
/// inside `filename="a;b.txt"` does not end the value. The surrounding quotes are
/// removed; nothing is unescaped, matching `dispositionParameter` in
/// `src/lib/utils/rest.ts`.
fn disposition_parameter<'a>(value: &'a str, name: &str) -> Option<&'a str> {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        // Skip the separators between parameters.
        while index < bytes.len() && (bytes[index].is_ascii_whitespace() || bytes[index] == b';') {
            index += 1;
        }
        let key_start = index;
        while index < bytes.len() && bytes[index] != b'=' && bytes[index] != b';' {
            index += 1;
        }
        let key = value[key_start..index].trim();
        if index >= bytes.len() || bytes[index] == b';' {
            // A bare disposition type (`attachment`) or a parameter without a value.
            continue;
        }
        index += 1; // '='
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index < bytes.len() && bytes[index] == b'"' {
            index += 1;
            let value_start = index;
            let mut escaped = false;
            while index < bytes.len() {
                let byte = bytes[index];
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    break;
                }
                index += 1;
            }
            if key.eq_ignore_ascii_case(name) {
                return Some(&value[value_start..index]);
            }
            if index < bytes.len() {
                index += 1; // the closing quote
            }
        } else {
            let value_start = index;
            while index < bytes.len() && bytes[index] != b';' {
                index += 1;
            }
            if key.eq_ignore_ascii_case(name) {
                return Some(value[value_start..index].trim());
            }
        }
    }
    None
}

/// Strips the `UTF-8''` charset prefix of an RFC 5987 value, exactly like
/// `filenameFromDisposition` in `src/lib/utils/rest.ts`; any other charset stays part
/// of the text, so both sides answer the same for a value neither can decode.
fn strip_utf8_charset(value: &str) -> &str {
    match value.get(..7) {
        Some(prefix) if prefix.eq_ignore_ascii_case("UTF-8''") => &value[7..],
        _ => value,
    }
}

pub fn filename_from_content_disposition(value: &str) -> Option<String> {
    if let Some(raw) = disposition_parameter(value, "filename*") {
        let encoded = strip_utf8_charset(raw.trim());
        // Mirrors `filenameFromDisposition`: a broken extended value keeps the text
        // the server sent instead of falling back to the plain parameter.
        return match percent_decode(encoded) {
            Ok(decoded) if !decoded.is_empty() => Some(decoded),
            Ok(_) => None,
            Err(_) if encoded.is_empty() => None,
            Err(_) => Some(encoded.to_string()),
        };
    }
    if let Some(raw) = disposition_parameter(value, "filename") {
        let name = raw.trim().trim_matches('"').trim_matches('\'').trim();
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    None
}

/// True when the disposition type is `attachment`. Only the token before the first
/// `;` counts, so `inline; filename="attachment.pdf"` is not an attachment. Mirrors
/// `isAttachmentDisposition` in `src/lib/utils/rest.ts`.
fn is_attachment_disposition(value: &str) -> bool {
    value
        .split(';')
        .next()
        .unwrap_or(value)
        .trim()
        .eq_ignore_ascii_case("attachment")
}

fn percent_decode(value: &str) -> Result<String, ()> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).map_err(|_| ())?;
            let byte = u8::from_str_radix(hex, 16).map_err(|_| ())?;
            output.push(byte);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).map_err(|_| ())
}

fn header_value(headers: &[RestProperty], name: &str) -> String {
    headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case(name))
        .map(|header| header.value.clone())
        .unwrap_or_default()
}

fn has_header(headers: &[RestProperty], name: &str) -> bool {
    headers
        .iter()
        .any(|header| header.name.eq_ignore_ascii_case(name))
}

/// Content type of a body. A multipart body always carries its boundary, so the
/// client can never supply a wrong one by hand.
fn default_content_type(body_type: &str, boundary: Option<&str>) -> String {
    match body_type {
        "json" => "application/json".to_string(),
        "xml" => "application/xml".to_string(),
        "form" => "application/x-www-form-urlencoded".to_string(),
        "multipart" => format!(
            "multipart/form-data; boundary={}",
            boundary.unwrap_or(MULTIPART_BOUNDARY_PLACEHOLDER)
        ),
        _ => "text/plain".to_string(),
    }
}

/// Stands in for the boundary in a raw dump whose boundary is not chosen yet.
const MULTIPART_BOUNDARY_PLACEHOLDER: &str = "BoundaryPlaceholder";

/// Media type of a file part: the desktop counterpart of
/// `utils/rest.ts::contentTypeForFilename`.
fn content_type_for_path(path: &Path) -> String {
    mime_guess::from_path(path)
        .first_raw()
        .unwrap_or("application/octet-stream")
        .to_string()
}

/// Last segment of a POSIX or Windows path.
fn filename_of(path: &str) -> String {
    let trimmed = path.trim_end_matches(|char| char == '/' || char == '\\');
    match trimmed.rfind(|char| char == '/' || char == '\\') {
        Some(index) => trimmed[index + 1..].to_string(),
        None => trimmed.to_string(),
    }
}

/// Name a file part carries on the wire: the alias the user kept, else the picked
/// path's file name.
fn file_part_filename(file: &RestFileRow) -> String {
    if !file.filename.trim().is_empty() {
        return file.filename.trim().to_string();
    }
    filename_of(file.path.trim())
}

/// Part names and filenames are written inside the quoting of
/// `Content-Disposition`, so the quote delimiter and the framing characters are
/// neutralised instead of breaking the header.
fn sanitized_parameter(value: &str) -> String {
    value
        .chars()
        .map(|char| match char {
            '"' | '\\' => '_',
            '\r' | '\n' => ' ',
            other => other,
        })
        .collect()
}

/// RFC 5987 `attr-char`: characters a `filename*` may carry unescaped. A filename
/// outside that set additionally gets the extended parameter, as the client does.
fn is_attr_char(char: char) -> bool {
    char.is_ascii_alphanumeric()
        || matches!(
            char,
            ' ' | '!'
                | '#'
                | '$'
                | '%'
                | '&'
                | '\''
                | '*'
                | '+'
                | '-'
                | '.'
                | '^'
                | '_'
                | '`'
                | '|'
                | '~'
                | '@'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
        )
}

fn needs_extended_filename(filename: &str) -> bool {
    !filename.is_empty() && !filename.chars().all(is_attr_char)
}

/// Same percent set as `encodeURIComponent`, which the client writes in its preview.
fn percent_encode_attr(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        let char = *byte as char;
        if byte.is_ascii_alphanumeric()
            || matches!(char, '-' | '_' | '.' | '~' | '!' | '*' | '\'' | '(' | ')')
        {
            out.push(char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn file_disposition(name: &str, filename: &str) -> String {
    let mut line = format!(
        "content-disposition: form-data; name=\"{}\"; filename=\"{}\"",
        sanitized_parameter(name),
        sanitized_parameter(filename)
    );
    if needs_extended_filename(filename) {
        line.push_str(&format!(
            "; filename*=UTF-8''{}",
            percent_encode_attr(filename)
        ));
    }
    line
}

/// A field value that carries a line break would open a new part in the body, so a
/// substituted secret cannot smuggle one in. Mirrors `validateMultipartValue`.
fn check_multipart_value(name: &str, value: &str) -> Result<(), String> {
    if value.contains('\r') || value.contains('\n') {
        return Err(format!("Field {name} cannot contain a line break"));
    }
    Ok(())
}

/// `reqwest` is compiled without its `multipart` feature, so the body is assembled
/// here: a boundary, one part per row, and the file bytes read from the picked path.
/// Rows with a blank name are skipped exactly as they are skipped in storage.
pub fn encode_multipart(
    fields: &[RestProperty],
    files: &[RestFileRow],
    boundary: &str,
) -> Result<Vec<u8>, String> {
    let mut out: Vec<u8> = Vec::new();
    for field in fields {
        if field.name.trim().is_empty() {
            continue;
        }
        check_multipart_value(field.name.trim(), &field.value)?;
        out.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        out.extend_from_slice(
            format!(
                "content-disposition: form-data; name=\"{}\"\r\n\r\n",
                sanitized_parameter(field.name.trim())
            )
            .as_bytes(),
        );
        out.extend_from_slice(field.value.as_bytes());
        out.extend_from_slice(b"\r\n");
    }
    for file in files {
        if file.name.trim().is_empty() {
            continue;
        }
        let path = file.path.trim();
        if path.is_empty() {
            return Err(
                "This file is not available on this machine. Pick it again before sending."
                    .to_string(),
            );
        }
        let bytes =
            std::fs::read(path).map_err(|error| format!("Could not read {path}: {error}"))?;
        let filename = file_part_filename(file);
        out.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        out.extend_from_slice(file_disposition(file.name.trim(), &filename).as_bytes());
        out.extend_from_slice(
            format!(
                "\r\ncontent-type: {}\r\n\r\n",
                content_type_for_path(Path::new(path))
            )
            .as_bytes(),
        );
        out.extend_from_slice(&bytes);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Ok(out)
}

/// Raw-request dump of a multipart body. File payloads are replaced by a marker: the
/// dump is stored as text, so a binary blob would not survive it. Mirrors
/// `utils/rest.ts::buildMultipartPreview`.
pub fn multipart_dump(fields: &[RestProperty], files: &[RestFileRow], boundary: &str) -> String {
    let mut out = String::new();
    for field in fields {
        if field.name.trim().is_empty() {
            continue;
        }
        out.push_str(&format!("--{boundary}\r\n"));
        out.push_str(&format!(
            "content-disposition: form-data; name=\"{}\"\r\n\r\n",
            sanitized_parameter(field.name.trim())
        ));
        out.push_str(&field.value);
        out.push_str("\r\n");
    }
    for file in files {
        if file.name.trim().is_empty() {
            continue;
        }
        let path = file.path.trim();
        let filename = file_part_filename(file);
        let guess = if path.is_empty() { &filename } else { path };
        out.push_str(&format!("--{boundary}\r\n"));
        out.push_str(&file_disposition(file.name.trim(), &filename));
        out.push_str(&format!(
            "\r\ncontent-type: {}\r\n\r\n",
            content_type_for_path(Path::new(guess))
        ));
        if path.is_empty() {
            out.push_str(&format!("<bytes of {filename}, not loaded>\r\n"));
        } else {
            out.push_str(&format!("<bytes of {path}>\r\n"));
        }
    }
    out.push_str(&format!("--{boundary}--\r\n"));
    out
}

/// Percent-encode set of `application/x-www-form-urlencoded`: unreserved ASCII plus
/// `*` stays literal, spaces become `+`, every other byte becomes `%XX`. Mirrors
/// `URLSearchParams.toString()` used by the frontend.
fn encode_form_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'*' => {
                out.push(byte as char);
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Serialises the Form-encode table into a request body. Rows with a blank name are
/// skipped, matching how they are persisted in `rest_request_form`.
pub fn encode_form_urlencoded(rows: &[RestProperty]) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(rows.len());
    for row in rows {
        if row.name.trim().is_empty() {
            continue;
        }
        parts.push(format!(
            "{}={}",
            encode_form_value(&row.name),
            encode_form_value(&row.value)
        ));
    }
    parts.join("&")
}

/// Body that goes on the wire, plus the text version of the same body for the raw
/// dump. A multipart dump replaces file payloads with a marker, because the dump is
/// stored as text and binary bytes would not survive that.
pub struct ResolvedBody {
    pub wire: Vec<u8>,
    pub dump: String,
}

/// Chooses the body that goes on the wire. The Form-encode table and the Form tab's
/// tables have their own storage and take priority whenever their tab is selected;
/// the JSON/XML/Text buffer is then ignored and never modified.
pub fn resolve_body(
    body_type: &str,
    body: &str,
    form: &[RestProperty],
    multipart: &[RestProperty],
    files: &[RestFileRow],
    boundary: &str,
) -> Result<ResolvedBody, String> {
    if body_type == "multipart" {
        return Ok(ResolvedBody {
            wire: encode_multipart(multipart, files, boundary)?,
            dump: multipart_dump(multipart, files, boundary),
        });
    }
    let text = if body_type == "form" {
        encode_form_urlencoded(form)
    } else {
        body.to_string()
    };
    Ok(ResolvedBody {
        wire: text.clone().into_bytes(),
        dump: text,
    })
}

fn host_header(url: &Url) -> String {
    match (url.host_str(), url.port()) {
        (Some(host), Some(port)) => format!("{host}:{port}"),
        (Some(host), None) => host.to_string(),
        _ => String::new(),
    }
}

fn prepare_headers(
    url: &Url,
    headers: &[RestProperty],
    resolved: &ResolvedBody,
    body_type: &str,
    boundary: Option<&str>,
) -> Vec<RestProperty> {
    let mut prepared = headers
        .iter()
        .filter(|header| !header.name.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>();

    if !has_header(&prepared, "user-agent") {
        prepared.push(RestProperty {
            name: "user-agent".to_string(),
            value: DEFAULT_USER_AGENT.to_string(),
        });
    }
    if !has_header(&prepared, "accept-encoding") {
        prepared.push(RestProperty {
            name: "accept-encoding".to_string(),
            value: "gzip".to_string(),
        });
    }
    if !has_header(&prepared, "host") {
        prepared.push(RestProperty {
            name: "host".to_string(),
            value: host_header(url),
        });
    }
    if !resolved.wire.is_empty() {
        // A multipart body must carry the boundary the bytes were built with, so a
        // hand-typed Content-Type is replaced rather than kept.
        if body_type == "multipart" {
            prepared.retain(|header| !header.name.eq_ignore_ascii_case("content-type"));
        }
        if !has_header(&prepared, "content-type") {
            prepared.push(RestProperty {
                name: "content-type".to_string(),
                value: default_content_type(body_type, boundary),
            });
        }
        if !has_header(&prepared, "content-length") {
            prepared.push(RestProperty {
                name: "content-length".to_string(),
                value: resolved.wire.len().to_string(),
            });
        }
    }

    prepared
}

pub fn build_raw_request(method: &str, url: &Url, headers: &[RestProperty], body: &str) -> String {
    let query = url
        .query()
        .map(|value| format!("?{value}"))
        .unwrap_or_default();
    let mut path = url.path().to_string();
    if path.is_empty() {
        path = "/".to_string();
    }
    let mut lines = vec![format!("{method} {path}{query} HTTP/1.1")];
    for header in headers {
        lines.push(format!("{}: {}", header.name, header.value));
    }
    let mut raw = lines.join("\r\n");
    raw.push_str("\r\n\r\n");
    raw.push_str(body);
    raw
}

/// The status line the server answered with, e.g. `HTTP/1.1 200 OK`. It is the first
/// line of the Raw response dump and the only part of a response that is not otherwise
/// stored, so it travels to SQLite with the saved body and headers.
///
/// `Version`'s `Display` is not implemented in the `http` crate; `Debug` prints the
/// protocol token (`HTTP/1.1`, `HTTP/2.0`), which is what the wire format wants.
pub fn build_status_line(version: Version, status: StatusCode) -> String {
    match status.canonical_reason() {
        Some(reason) => format!("{version:?} {} {reason}", status.as_u16()),
        // A non-standard code has no reason phrase: keep the line free of a trailing space.
        None => format!("{version:?} {}", status.as_u16()),
    }
}

fn build_headers_map(headers: &[RestProperty]) -> Result<Vec<(HeaderName, HeaderValue)>, String> {
    let mut result = Vec::with_capacity(headers.len());
    for header in headers {
        let name = HeaderName::from_bytes(header.name.trim().as_bytes())
            .map_err(|_| format!("Invalid header name: {}", header.name))?;
        let value = HeaderValue::from_str(&header.value)
            .map_err(|_| format!("Invalid header value for {}", header.name))?;
        result.push((name, value));
    }
    Ok(result)
}

fn build_client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|error| format!("Could not create HTTP client: {error}"))
}

#[derive(Debug)]
struct PreparedCall {
    method: Method,
    url: Url,
    headers: Vec<(HeaderName, HeaderValue)>,
    body: Vec<u8>,
    raw_request: String,
}

/// A boundary that cannot appear inside the body it separates: a fresh UUID.
fn multipart_boundary() -> String {
    Uuid::new_v4().to_string()
}

fn substitute_rows(
    rows: &[RestProperty],
    values: &HashMap<String, String>,
) -> Result<Vec<RestProperty>, String> {
    rows.iter()
        .map(|row| {
            Ok(RestProperty {
                name: row.name.clone(),
                value: vars::substitute(&row.value, values)?,
            })
        })
        .collect()
}

/// Names the variables that a request actually mentions. Query values are read decoded
/// because the client writes `{{ token }}` into the URL as `%7B%7Btoken%7D%7D`, and only
/// the body the selected tab produces is scanned, so an unused buffer with a broken
/// template does not block a send.
fn referenced_var_names(
    url: &Url,
    headers: &[RestProperty],
    body: &str,
    body_type: &str,
    form: &[RestProperty],
    multipart: &[RestProperty],
) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let add = |text: &str, names: &mut Vec<String>| {
        for name in vars::template_names(text) {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    };

    for (_, value) in url.query_pairs() {
        add(&value, &mut names);
    }
    for header in headers {
        add(&header.value, &mut names);
    }
    match body_type {
        "form" => {
            for row in form {
                add(&row.value, &mut names);
            }
        }
        "multipart" => {
            for row in multipart {
                add(&row.value, &mut names);
            }
        }
        _ => add(body, &mut names),
    }
    names
}

/// A template in a place the resolver never touches would go on the wire as the literal
/// `{{ name }}`, so it is reported instead of silently sent. Part names and file rows are
/// written into the body verbatim, which is why they are checked here too.
fn reject_unsupported_templates(
    url: &Url,
    headers: &[RestProperty],
    multipart: &[RestProperty],
    files: &[RestFileRow],
) -> Result<(), String> {
    if let Some(name) = vars::first_template_name(&vars::decoded_braces(url.path())) {
        return Err(format!("unresolved-template:{name}"));
    }
    for header in headers {
        if let Some(name) = vars::first_template_name(&header.name) {
            return Err(format!("unresolved-template:{name}"));
        }
    }
    for row in multipart {
        if let Some(name) = vars::first_template_name(&row.name) {
            return Err(format!("unresolved-template:{name}"));
        }
    }
    for file in files {
        for text in [&file.name, &file.path, &file.filename] {
            if let Some(name) = vars::first_template_name(text) {
                return Err(format!("unresolved-template:{name}"));
            }
        }
    }
    Ok(())
}

/// Every value a request needs before it goes on the wire. Keyring reads are blocking,
/// so they happen outside the async runtime instead of stalling it.
async fn load_request_values(
    db: &tauri::State<'_, AppDb>,
    app: &tauri::AppHandle,
    url: &str,
    headers: &[RestProperty],
    body: &str,
    body_type: &str,
    form: &[RestProperty],
    multipart: &[RestProperty],
) -> Result<VarValues, String> {
    let parsed = normalize_rest_url(url)?;
    let names = referenced_var_names(&parsed, headers, body, body_type, form, multipart);
    if names.is_empty() {
        return Ok(VarValues::default());
    }
    let database = db.inner().clone();
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let connection = database.conn.lock().map_err(|error| error.to_string())?;
        vars::load_values(
            &connection,
            &vars::KeyringSecrets::new(handle),
            &names
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[allow(clippy::too_many_arguments)]
fn prepare_call(
    method: &str,
    url: &str,
    headers: &[RestProperty],
    body: &str,
    body_type: &str,
    form: &[RestProperty],
    multipart: &[RestProperty],
    files: &[RestFileRow],
    values: &VarValues,
) -> Result<PreparedCall, String> {
    let parsed = normalize_rest_url(url)?;
    reject_unsupported_templates(&parsed, headers, multipart, files)?;
    let method = Method::from_bytes(method.trim().to_ascii_uppercase().as_bytes())
        .map_err(|_| format!("Invalid HTTP method: {method}"))?;
    let boundary = match body_type {
        "multipart" => Some(multipart_boundary()),
        _ => None,
    };
    let boundary_for_body = boundary
        .as_deref()
        .unwrap_or(MULTIPART_BOUNDARY_PLACEHOLDER);

    // The wire gets the real values; the dump is built from the masked map so a safe
    // variable never reaches SQLite as text.
    let real_url = vars::substitute_url(&parsed, &values.real)?;
    let masked_url = vars::substitute_url(&parsed, &values.masked)?;
    let real_headers = substitute_rows(headers, &values.real)?;
    let masked_headers = substitute_rows(headers, &values.masked)?;
    let real_body = vars::substitute(body, &values.real)?;
    let masked_body = vars::substitute(body, &values.masked)?;
    let real_form = substitute_rows(form, &values.real)?;
    let masked_form = substitute_rows(form, &values.masked)?;
    let real_multipart = substitute_rows(multipart, &values.real)?;
    let masked_multipart = substitute_rows(multipart, &values.masked)?;

    let resolved = resolve_body(
        body_type,
        &real_body,
        &real_form,
        &real_multipart,
        files,
        boundary_for_body,
    )?;
    // Same boundary and the real `Content-Length`, only the field values are masked, so
    // the dump keeps the framing of the bytes that are actually sent.
    let masked_resolved = resolve_body(
        body_type,
        &masked_body,
        &masked_form,
        &masked_multipart,
        files,
        boundary_for_body,
    )?;
    let prepared = prepare_headers(&real_url, &real_headers, &resolved, body_type, boundary.as_deref());
    let dump_headers = prepare_headers(&masked_url, &masked_headers, &resolved, body_type, boundary.as_deref());
    let wire_headers = prepared
        .iter()
        .filter(|header| {
            !header.name.eq_ignore_ascii_case("host")
                && !header.name.eq_ignore_ascii_case("content-length")
        })
        .cloned()
        .collect::<Vec<_>>();
    let header_map = build_headers_map(&wire_headers)?;
    let raw_request = build_raw_request(
        method.as_str(),
        &masked_url,
        &dump_headers,
        &masked_resolved.dump,
    );
    Ok(PreparedCall {
        method,
        url: real_url,
        headers: header_map,
        body: resolved.wire,
        raw_request,
    })
}

fn response_headers(response: &reqwest::Response) -> Vec<RestProperty> {
    response
        .headers()
        .iter()
        .map(|(name, value)| RestProperty {
            name: name.as_str().to_string(),
            value: value.to_str().unwrap_or("").to_string(),
        })
        .collect()
}

#[tauri::command(rename = "list_rest_projects", rename_all = "snake_case")]
pub fn cmd_list_rest_projects(db: tauri::State<'_, AppDb>) -> Result<Vec<RestProject>, String> {
    let connection = db.conn.lock().map_err(|error| error.to_string())?;
    list_rest_projects(&connection).map_err(|error| error.to_string())
}

#[tauri::command(rename = "create_rest_project", rename_all = "snake_case")]
pub fn cmd_create_rest_project(
    db: tauri::State<'_, AppDb>,
    name: String,
) -> Result<RestProject, String> {
    let connection = db.conn.lock().map_err(|error| error.to_string())?;
    create_rest_project(&connection, name.trim())
}

#[tauri::command(rename = "rename_rest_project", rename_all = "snake_case")]
pub fn cmd_rename_rest_project(
    db: tauri::State<'_, AppDb>,
    id: String,
    name: String,
) -> Result<(), String> {
    let connection = db.conn.lock().map_err(|error| error.to_string())?;
    rename_rest_project(&connection, &id, name.trim())
}

#[tauri::command(rename = "delete_rest_project", rename_all = "snake_case")]
pub fn cmd_delete_rest_project(db: tauri::State<'_, AppDb>, id: String) -> Result<(), String> {
    let connection = db.conn.lock().map_err(|error| error.to_string())?;
    delete_rest_project(&connection, &id)
}

#[tauri::command(rename = "list_rest_requests", rename_all = "snake_case")]
pub fn cmd_list_rest_requests(db: tauri::State<'_, AppDb>) -> Result<Vec<RestRequest>, String> {
    let connection = db.conn.lock().map_err(|error| error.to_string())?;
    list_rest_requests(&connection)
}

#[tauri::command(rename = "create_rest_request", rename_all = "snake_case")]
pub fn cmd_create_rest_request(
    db: tauri::State<'_, AppDb>,
    draft: RestRequestDraft,
) -> Result<RestRequest, String> {
    let connection = db.conn.lock().map_err(|error| error.to_string())?;
    create_rest_request(&connection, &draft)
}

#[tauri::command(rename = "update_rest_request", rename_all = "snake_case")]
pub fn cmd_update_rest_request(
    db: tauri::State<'_, AppDb>,
    request: RestRequestUpdate,
) -> Result<(), String> {
    let connection = db.conn.lock().map_err(|error| error.to_string())?;
    update_rest_request(&connection, &request)
}

#[tauri::command(rename = "rename_rest_request", rename_all = "snake_case")]
pub fn cmd_rename_rest_request(
    db: tauri::State<'_, AppDb>,
    id: String,
    name: String,
) -> Result<(), String> {
    let connection = db.conn.lock().map_err(|error| error.to_string())?;
    rename_rest_request(&connection, &id, name.trim())
}

#[tauri::command(rename = "delete_rest_request", rename_all = "snake_case")]
pub fn cmd_delete_rest_request(db: tauri::State<'_, AppDb>, id: String) -> Result<(), String> {
    let connection = db.conn.lock().map_err(|error| error.to_string())?;
    delete_rest_request(&connection, &id)
}

#[tauri::command(rename = "duplicate_rest_request", rename_all = "snake_case")]
pub fn cmd_duplicate_rest_request(
    db: tauri::State<'_, AppDb>,
    id: String,
) -> Result<RestRequest, String> {
    let connection = db.conn.lock().map_err(|error| error.to_string())?;
    duplicate_rest_request(&connection, &id)
}

#[tauri::command(rename = "save_rest_response", rename_all = "snake_case")]
pub fn cmd_save_rest_response(
    db: tauri::State<'_, AppDb>,
    id: String,
    body: String,
    headers: Vec<RestProperty>,
    raw_request: String,
    status_line: String,
) -> Result<(), String> {
    let connection = db.conn.lock().map_err(|error| error.to_string())?;
    save_rest_response(
        &connection,
        &id,
        &body,
        &headers,
        &raw_request,
        &status_line,
    )
}

#[tauri::command(rename = "clear_rest_response", rename_all = "snake_case")]
pub fn cmd_clear_rest_response(db: tauri::State<'_, AppDb>, id: String) -> Result<(), String> {
    let connection = db.conn.lock().map_err(|error| error.to_string())?;
    clear_rest_response(&connection, &id)
}

#[tauri::command(rename = "execute_rest_request", rename_all = "snake_case")]
pub async fn cmd_execute_rest_request(
    cancel: tauri::State<'_, RestCancelState>,
    db: tauri::State<'_, AppDb>,
    app: tauri::AppHandle,
    method: String,
    url: String,
    headers: Vec<RestProperty>,
    body: String,
    body_type: String,
    form: Option<Vec<RestProperty>>,
    multipart: Option<Vec<RestProperty>>,
    files: Option<Vec<RestFileRow>>,
) -> Result<RestResponse, String> {
    let form = form.unwrap_or_default();
    let multipart = multipart.unwrap_or_default();
    let files = files.unwrap_or_default();
    let values = load_request_values(&db, &app, &url, &headers, &body, &body_type, &form, &multipart)
        .await?;
    let PreparedCall {
        method,
        url,
        headers,
        body: request_body,
        raw_request,
    } = prepare_call(
        &method,
        &url,
        &headers,
        &body,
        &body_type,
        &form,
        &multipart,
        &files,
        &values,
    )?;
    let client = build_client()?;
    let (cancel_id, receiver) = cancel.begin();

    let mut builder = client.request(method, url);
    for (name, value) in &headers {
        builder = builder.header(name, value);
    }
    if !request_body.is_empty() {
        builder = builder.body(request_body);
    }

    let operation = async move {
        let started = Instant::now();
        let response = builder
            .send()
            .await
            .map_err(|error| format!("Request failed: {error}"))?;
        let duration_ms = started.elapsed().as_secs_f64() * 1000.0;
        let status = response.status();
        let version = response.version();
        let headers = response_headers(&response);
        let content_type = header_value(&headers, "content-type");
        let disposition = header_value(&headers, "content-disposition");
        let is_attachment = is_attachment_disposition(&disposition);
        let filename = filename_from_content_disposition(&disposition);
        let content_length = response.content_length();

        let mut body_text = String::new();
        let mut size = content_length.unwrap_or(0);
        let mut is_binary = false;
        let mut language = language_for_content_type(&content_type);

        if !is_attachment && (content_type.is_empty() || is_text_content_type(&content_type)) {
            let bytes = response
                .bytes()
                .await
                .map_err(|error| format!("Could not read the response: {error}"))?;
            size = bytes.len() as u64;
            match String::from_utf8(bytes.to_vec()) {
                Ok(text) => {
                    language = language_for_body(&content_type, &text);
                    body_text = text;
                }
                Err(_) => {
                    is_binary = true;
                    language = None;
                }
            }
        } else if !is_attachment {
            is_binary = true;
            language = None;
        }

        if is_attachment {
            language = None;
            is_binary = true;
        }

        Ok::<RestResponse, String>(RestResponse {
            status: status.as_u16(),
            status_text: status.canonical_reason().unwrap_or("").to_string(),
            status_line: build_status_line(version, status),
            headers,
            body: body_text,
            size,
            duration_ms,
            content_type,
            language,
            is_attachment,
            filename,
            is_binary,
            raw_request,
        })
    };

    let result = tokio::select! {
        result = operation => result,
        _ = receiver => Err("Request aborted".to_string()),
    };
    cancel.clear(cancel_id);
    result
}

#[tauri::command(rename = "cancel_rest_request", rename_all = "snake_case")]
pub fn cmd_cancel_rest_request(cancel: tauri::State<'_, RestCancelState>) -> bool {
    cancel.cancel()
}

#[tauri::command(rename = "download_rest_response", rename_all = "snake_case")]
pub async fn cmd_download_rest_response(
    db: tauri::State<'_, AppDb>,
    app: tauri::AppHandle,
    method: String,
    url: String,
    headers: Vec<RestProperty>,
    body: String,
    body_type: String,
    form: Option<Vec<RestProperty>>,
    multipart: Option<Vec<RestProperty>>,
    files: Option<Vec<RestFileRow>>,
    path: String,
) -> Result<u64, String> {
    let form = form.unwrap_or_default();
    let multipart = multipart.unwrap_or_default();
    let files = files.unwrap_or_default();
    let values = load_request_values(&db, &app, &url, &headers, &body, &body_type, &form, &multipart)
        .await?;
    let call = prepare_call(
        &method,
        &url,
        &headers,
        &body,
        &body_type,
        &form,
        &multipart,
        &files,
        &values,
    )?;
    let client = build_client()?;
    let mut builder = client.request(call.method.clone(), call.url.clone());
    for (name, value) in &call.headers {
        builder = builder.header(name, value);
    }
    if !call.body.is_empty() {
        builder = builder.body(call.body.clone());
    }
    let response = builder
        .send()
        .await
        .map_err(|error| format!("Request failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Server returned {} {}",
            response.status().as_u16(),
            response.status().canonical_reason().unwrap_or("")
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("Could not read the response: {error}"))?;
    let size = bytes.len() as u64;
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(parent) = Path::new(&path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
        }
        std::fs::write(path, bytes).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())??;
    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn memory_db() -> Connection {
        let connection = Connection::open_in_memory().expect("in-memory database");
        db::init_schema(&connection).expect("schema");
        connection
    }

    fn property(name: &str, value: &str) -> RestProperty {
        RestProperty {
            name: name.to_string(),
            value: value.to_string(),
        }
    }

    #[test]
    fn normalizes_urls() {
        assert_eq!(
            normalize_rest_url("example.com/test")
                .expect("url")
                .as_str(),
            "http://example.com/test"
        );
        assert_eq!(
            normalize_rest_url("https://example.com/a?b=1")
                .expect("url")
                .as_str(),
            "https://example.com/a?b=1"
        );
        // A `://` inside the query must not be mistaken for a scheme: the bare host
        // still needs its `http://` prefix, exactly like `services/rest.ts`.
        assert_eq!(
            normalize_rest_url("example.com/?next=http://y")
                .expect("url")
                .as_str(),
            "http://example.com/?next=http://y"
        );
        assert_eq!(
            normalize_rest_url("example.com:8080/x")
                .expect("url")
                .as_str(),
            "http://example.com:8080/x"
        );
        assert_eq!(
            normalize_rest_url("HTTP://Example.COM/Path")
                .expect("url")
                .as_str(),
            "http://example.com/Path"
        );
        assert!(normalize_rest_url("").is_err());
        assert!(normalize_rest_url("ftp://example.com").is_err());
    }

    #[test]
    fn detects_languages() {
        assert_eq!(
            language_for_content_type("application/json; charset=utf-8").as_deref(),
            Some("json")
        );
        assert_eq!(
            language_for_content_type("application/xml").as_deref(),
            Some("xml")
        );
        assert_eq!(
            language_for_content_type("text/html").as_deref(),
            Some("html")
        );
        assert_eq!(
            language_for_content_type("text/plain").as_deref(),
            Some("text")
        );
        assert_eq!(language_for_content_type("application/octet-stream"), None);
        assert_eq!(language_for_body("", "<root/>").as_deref(), Some("xml"));
        assert_eq!(language_for_body("", "{\"a\":1}").as_deref(), Some("json"));
    }

    #[test]
    fn reads_html_flavoured_types_and_html_documents() {
        // The HTML language is used for every subtype that says HTML, and XHTML is
        // matched before XML so it never lands on the XML language.
        for content_type in [
            "text/html; charset=utf-8",
            "application/xhtml+xml",
            "application/xhtml",
            "text/x-html",
        ] {
            assert_eq!(
                language_for_content_type(content_type).as_deref(),
                Some("html"),
                "{content_type}"
            );
        }
        assert_eq!(
            language_for_content_type("application/atom+xml").as_deref(),
            Some("xml")
        );
        // A body with no header is HTML when it carries an HTML tag or an HTML doctype.
        assert_eq!(
            language_for_body("", "<!DOCTYPE html><html><body>hi</body></html>").as_deref(),
            Some("html")
        );
        assert_eq!(
            language_for_body("", "<!doctype html>").as_deref(),
            Some("html")
        );
        assert_eq!(
            language_for_body("", "<!DOCTYPE html>").as_deref(),
            Some("html")
        );
        assert_eq!(
            language_for_body("", "<soap:Envelope/>").as_deref(),
            Some("xml")
        );
    }

    #[test]
    fn parses_content_disposition() {
        assert_eq!(
            filename_from_content_disposition("attachment; filename=\"report.csv\""),
            Some("report.csv".to_string())
        );
        assert_eq!(
            filename_from_content_disposition("attachment; filename*=UTF-8''my%20file.txt"),
            Some("my file.txt".to_string())
        );
        // A quoted value keeps its semicolon instead of ending at the first one.
        assert_eq!(
            filename_from_content_disposition("attachment; filename=\"a;b.txt\""),
            Some("a;b.txt".to_string())
        );
        assert_eq!(
            filename_from_content_disposition("attachment; filename*=UTF-8''a%3Bb.txt"),
            Some("a;b.txt".to_string())
        );
        // The extended parameter wins over the plain one, whatever their order.
        assert_eq!(
            filename_from_content_disposition(
                "attachment; filename=\"fallback.txt\"; filename*=UTF-8''my%20file.txt"
            ),
            Some("my file.txt".to_string())
        );
        // A broken extended value keeps the text the server sent, like the browser.
        assert_eq!(
            filename_from_content_disposition("attachment; filename*=UTF-8''caf%E9.txt"),
            Some("caf%E9.txt".to_string())
        );
        assert_eq!(filename_from_content_disposition("inline"), None);
    }

    #[test]
    fn only_the_disposition_type_marks_an_attachment() {
        assert!(is_attachment_disposition("attachment"));
        assert!(is_attachment_disposition(
            "ATTACHMENT; filename=\"report.csv\""
        ));
        // A filename that happens to contain the word is not the disposition type.
        assert!(!is_attachment_disposition(
            "inline; filename=\"attachment.pdf\""
        ));
        assert!(!is_attachment_disposition(""));
    }

    #[test]
    fn builds_raw_request_with_defaults() {
        let url = normalize_rest_url("https://example.com/api?x=1").expect("url");
        let resolved = resolve_body("json", "{\"a\":1}", &[], &[], &[], "unused").expect("body");
        let headers = prepare_headers(&url, &[], &resolved, "json", None);
        let raw = build_raw_request("POST", &url, &headers, "{\"a\":1}");
        assert!(raw.starts_with("POST /api?x=1 HTTP/1.1\r\n"));
        assert!(raw.contains("host: example.com"));
        assert!(raw.contains("content-type: application/json"));
        assert!(raw.contains("content-length: 7"));
        assert!(raw.ends_with("{\"a\":1}"));
    }

    #[test]
    fn status_line_keeps_version_and_reason() {
        assert_eq!(
            build_status_line(Version::HTTP_11, StatusCode::OK),
            "HTTP/1.1 200 OK"
        );
        assert_eq!(
            build_status_line(Version::HTTP_2, StatusCode::NOT_FOUND),
            "HTTP/2.0 404 Not Found"
        );
        // A code the RFCs never named has no reason phrase, so the line carries no
        // trailing space for the Raw response dump to keep.
        assert_eq!(
            build_status_line(Version::HTTP_11, StatusCode::from_u16(599).unwrap()),
            "HTTP/1.1 599"
        );
    }

    #[test]
    fn encodes_form_rows_like_urlsearchparams() {
        let rows = vec![property("name", "John Doe"), property("tag", "a&b")];
        assert_eq!(encode_form_urlencoded(&rows), "name=John+Doe&tag=a%26b");
        assert_eq!(
            encode_form_urlencoded(&[property("q", "привет")]),
            "q=%D0%BF%D1%80%D0%B8%D0%B2%D0%B5%D1%82"
        );
        assert_eq!(encode_form_urlencoded(&[property("  ", "x")]), "");
        assert_eq!(encode_form_urlencoded(&[]), "");
    }

    #[test]
    fn form_table_has_priority_over_text_body() {
        let rows = vec![property("user", "a"), property("pass", "b")];
        let resolved = |body_type: &str, body: &str, form: &[RestProperty]| {
            resolve_body(body_type, body, form, &[], &[], "Boundary").expect("body")
        };
        let form = resolved("form", "{\"a\":1}", &rows);
        assert_eq!(form.dump, "user=a&pass=b");
        assert_eq!(form.wire, form.dump.as_bytes());
        assert_eq!(resolved("json", "{\"a\":1}", &rows).dump, "{\"a\":1}");
        assert_eq!(resolved("text", "plain", &rows).dump, "plain");
        assert_eq!(resolved("form", "{\"a\":1}", &[]).dump, "");
    }

    #[test]
    fn prepare_call_uses_form_rows_and_their_content_type() {
        let call = prepare_call(
            "POST",
            "https://example.com/login",
            &[],
            "{\"ignored\":true}",
            "form",
            &[property("user", "a b"), property("pass", "c&d")],
            &[],
            &[],
            &VarValues::default(),
        )
        .expect("prepared call");
        assert_eq!(call.body, b"user=a+b&pass=c%26d".to_vec());
        assert!(call
            .raw_request
            .contains("content-type: application/x-www-form-urlencoded"));
        assert!(call
            .raw_request
            .contains(&format!("content-length: {}", call.body.len())));
        assert!(call.raw_request.ends_with("user=a+b&pass=c%26d"));
        assert!(!call.raw_request.contains("ignored"));
    }

    #[test]
    fn multipart_body_is_encoded_with_the_boundary_and_a_dump() {
        let directory = std::env::temp_dir().join(format!("rest-multipart-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("temp dir");
        let path = directory.join("report.txt");
        std::fs::write(&path, b"note content").expect("fixture");

        let files = vec![RestFileRow {
            name: "document".to_string(),
            path: path.display().to_string(),
            filename: String::new(),
        }];
        let fields = vec![property("comment", "hello world"), property(" ", "dropped")];
        let resolved = resolve_body("multipart", "{\"ignored\":1}", &[], &fields, &files, "BND")
            .expect("multipart body");

        let wire = String::from_utf8(resolved.wire.clone()).expect("text body");
        assert!(wire.contains("--BND\r\ncontent-disposition: form-data; name=\"comment\""));
        assert!(wire.contains("hello world"));
        assert!(!wire.contains("dropped"));
        assert!(wire.contains("filename=\"report.txt\""));
        assert!(wire.contains("content-type: text/plain"));
        assert!(wire.contains("note content"));
        assert!(wire.ends_with("--BND--\r\n"));

        // The dump keeps the same framing without the payload and reports its length.
        assert!(resolved.dump.contains("<bytes of "));
        assert!(!resolved.dump.contains("note content"));
        assert!(!resolved.dump.contains("ignored"));

        let call = prepare_call(
            "POST",
            "https://example.com/upload",
            &[property("content-type", "application/json")],
            "",
            "multipart",
            &[],
            &fields,
            &files,
            &VarValues::default(),
        )
        .expect("prepared call");
        // `prepare_call` generates its own boundary, so only the framing is comparable.
        let boundary = call
            .headers
            .iter()
            .find(|(name, _)| name.as_str() == "content-type")
            .map(|(_, value)| {
                value
                    .to_str()
                    .unwrap_or_default()
                    .replace("multipart/form-data; boundary=", "")
            })
            .expect("content type");
        assert!(boundary.len() >= 8);
        let body = String::from_utf8(call.body.clone()).expect("utf8 body");
        assert!(body.contains(&format!(
            "--{boundary}\r\ncontent-disposition: form-data; name=\"comment\""
        )));
        assert!(body.contains("note content"));
        assert!(body.ends_with(&format!("--{boundary}--\r\n")));
        assert!(call.raw_request.contains(&format!(
            "content-type: multipart/form-data; boundary={boundary}"
        )));
        assert!(!call.raw_request.contains("application/json"));
        assert!(call
            .raw_request
            .contains(&format!("content-length: {}", call.body.len())));
        assert_eq!(
            call.headers
                .iter()
                .filter(|(name, _)| name.as_str() == "content-type")
                .count(),
            1
        );

        // A file row with no path cannot be sent: nothing is invented for it.
        let missing = resolve_body(
            "multipart",
            "",
            &[],
            &[],
            &[RestFileRow {
                name: "document".to_string(),
                path: String::new(),
                filename: "gone.txt".to_string(),
            }],
            "BND",
        );
        assert!(missing.is_err());

        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn project_and_request_roundtrip() {
        let connection = memory_db();
        let project = create_rest_project(&connection, "Local API").expect("project");
        assert_eq!(
            list_rest_projects(&connection).expect("list"),
            vec![project.clone()]
        );

        let draft = RestRequestDraft {
            project_id: Some(project.id.clone()),
            name: "Login".to_string(),
            method: "POST".to_string(),
            url: "https://example.com/login".to_string(),
            body: "{\"user\":\"a\"}".to_string(),
            body_type: "json".to_string(),
            save_response: true,
            headers: vec![property("Accept", "application/json")],
            form: vec![],
            multipart: vec![],
            files: vec![],
            ..Default::default()
        };
        let created = create_rest_request(&connection, &draft).expect("request");
        assert_eq!(created.name, "Login");
        assert_eq!(created.headers, draft.headers);
        assert_eq!(created.body_type, "json");

        let update = RestRequestUpdate {
            id: created.id.clone(),
            project_id: created.project_id.clone(),
            name: "Login v2".to_string(),
            method: "POST".to_string(),
            url: "https://example.com/login".to_string(),
            body: "{}".to_string(),
            body_type: "json".to_string(),
            save_response: false,
            headers: vec![property("Accept", "text/plain")],
            form: vec![property("a", "1")],
            multipart: vec![],
            files: vec![],
            ..Default::default()
        };
        update_rest_request(&connection, &update).expect("update");
        let stored = read_rest_request(&connection, &created.id)
            .expect("read")
            .expect("present");
        assert_eq!(stored.name, "Login v2");
        assert_eq!(stored.headers, update.headers);
        assert_eq!(stored.form, update.form);

        save_rest_response(
            &connection,
            &created.id,
            "{\"ok\":true}",
            &[property("content-type", "application/json")],
            "POST /login HTTP/1.1",
            "HTTP/1.1 200 OK",
        )
        .expect("save response");
        let with_response = read_rest_request(&connection, &created.id)
            .expect("read")
            .expect("present");
        assert_eq!(
            with_response.response_body.as_deref(),
            Some("{\"ok\":true}")
        );
        assert_eq!(
            with_response.response_headers,
            vec![property("content-type", "application/json")]
        );
        assert_eq!(
            with_response.raw_request.as_deref(),
            Some("POST /login HTTP/1.1")
        );
        assert_eq!(
            with_response.response_status_line.as_deref(),
            Some("HTTP/1.1 200 OK")
        );

        clear_rest_response(&connection, &created.id).expect("clear");
        let cleared = read_rest_request(&connection, &created.id)
            .expect("read")
            .expect("present");
        assert_eq!(cleared.response_body, None);
        assert!(cleared.response_headers.is_empty());
        assert_eq!(cleared.response_status_line, None);
    }

    #[test]
    fn open_tabs_round_trip_and_unknown_codes_fall_back_to_the_first_tab() {
        let connection = memory_db();
        let draft = RestRequestDraft {
            name: "Tabs".to_string(),
            request_tab: "Raw".to_string(),
            multipart_tab: "FILES".to_string(),
            response_tab: "not-a-tab".to_string(),
            ..Default::default()
        };
        let created = create_rest_request(&connection, &draft).expect("request");
        // Codes are stored lowercase, and a code the panel does not have reads as its
        // first tab rather than rendering a tab that does not exist.
        assert_eq!(created.request_tab, "raw");
        assert_eq!(created.multipart_tab, "files");
        assert_eq!(created.response_tab, "response");

        // A draft written before the tabs were stored names none of them.
        let legacy = create_rest_request(&connection, &simple_draft("Legacy")).expect("legacy");
        assert_eq!(legacy.request_tab, "headers");
        assert_eq!(legacy.multipart_tab, "fields");
        assert_eq!(legacy.response_tab, "response");

        let update = RestRequestUpdate {
            id: created.id.clone(),
            name: "Tabs".to_string(),
            request_tab: "cookies".to_string(),
            multipart_tab: "fields".to_string(),
            response_tab: "headers".to_string(),
            ..Default::default()
        };
        update_rest_request(&connection, &update).expect("update");
        let stored = read_rest_request(&connection, &created.id)
            .expect("read")
            .expect("present");
        assert_eq!(stored.request_tab, "cookies");
        assert_eq!(stored.multipart_tab, "fields");
        assert_eq!(stored.response_tab, "headers");

        // The copy of a request opens on the same tabs as its source.
        let duplicate = duplicate_rest_request(&connection, &created.id).expect("duplicate");
        assert_eq!(duplicate.request_tab, "cookies");
        assert_eq!(duplicate.multipart_tab, "fields");
        assert_eq!(duplicate.response_tab, "headers");
    }

    #[test]
    fn form_rows_are_stored_in_their_own_table() {
        let connection = memory_db();
        let body = "{\"q\":\"text buffer\"}".to_string();
        let rows = vec![property("q", "hello world"), property("page", "2")];

        let draft = RestRequestDraft {
            project_id: None,
            name: "Search".to_string(),
            method: "GET".to_string(),
            url: "https://example.com/search".to_string(),
            body: body.clone(),
            body_type: "json".to_string(),
            save_response: false,
            headers: vec![],
            form: rows.clone(),
            multipart: vec![],
            files: vec![],
            ..Default::default()
        };
        let created = create_rest_request(&connection, &draft).expect("request");

        let update = RestRequestUpdate {
            id: created.id.clone(),
            project_id: None,
            name: "Search".to_string(),
            method: "GET".to_string(),
            url: "https://example.com/search".to_string(),
            body: body.clone(),
            body_type: "form".to_string(),
            save_response: false,
            headers: vec![],
            form: rows.clone(),
            multipart: vec![],
            files: vec![],
            ..Default::default()
        };
        update_rest_request(&connection, &update).expect("update");

        let stored = read_rest_request(&connection, &created.id)
            .expect("read")
            .expect("present");
        assert_eq!(stored.body_type, "form");
        assert_eq!(stored.form, rows);
        // Selecting the Form-encode tab must not rewrite the JSON/XML/Text buffer.
        assert_eq!(stored.body, body);

        let form_rows: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM rest_request_form WHERE request_id = ?1",
                params![created.id],
                |row| row.get(0),
            )
            .expect("form row count");
        assert_eq!(form_rows, 2);

        let legacy_rows: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM rest_request_properties WHERE request_id = ?1 AND kind = 'form'",
                params![created.id],
                |row| row.get(0),
            )
            .expect("legacy property count");
        assert_eq!(legacy_rows, 0);
    }

    fn file_row(name: &str, path: &str, filename: &str) -> RestFileRow {
        RestFileRow {
            name: name.to_string(),
            path: path.to_string(),
            filename: filename.to_string(),
        }
    }

    /** (file rows, field rows) still in the tables, to prove deletes clean up. */
    fn row_counts(connection: &Connection) -> (i64, i64) {
        let files: i64 = connection
            .query_row("SELECT COUNT(*) FROM rest_request_files", [], |row| {
                row.get(0)
            })
            .expect("file count");
        let fields: i64 = connection
            .query_row("SELECT COUNT(*) FROM rest_request_multipart", [], |row| {
                row.get(0)
            })
            .expect("multipart count");
        (files, fields)
    }

    #[test]
    fn multipart_rows_survive_a_round_trip_a_duplicate_and_a_delete() {
        let connection = memory_db();
        let draft = RestRequestDraft {
            project_id: None,
            name: "Upload".to_string(),
            method: "POST".to_string(),
            url: "https://example.com/upload".to_string(),
            body: "{\"buffer\":\"untouched\"}".to_string(),
            body_type: "multipart".to_string(),
            save_response: false,
            headers: vec![],
            form: vec![property("encoded", "1")],
            multipart: vec![
                property("comment", "hello world"),
                property(" ", "dropped with a blank name"),
                property("flag", ""),
            ],
            files: vec![
                file_row("document", "/tmp/report.txt", "report.txt"),
                file_row(" ", "/tmp/anonymous.bin", ""),
            ],
            ..Default::default()
        };
        let created = create_rest_request(&connection, &draft).expect("request");

        let stored = read_rest_request(&connection, &created.id)
            .expect("read")
            .expect("present");
        assert_eq!(stored.body_type, "multipart");
        // Only named rows survive, in the order they were typed.
        assert_eq!(
            stored.multipart,
            vec![property("comment", "hello world"), property("flag", "")]
        );
        assert_eq!(
            stored.files,
            vec![file_row("document", "/tmp/report.txt", "report.txt")]
        );
        // Neither the JSON/XML/Text buffer nor the Form-encode table is rewritten.
        assert_eq!(stored.body, "{\"buffer\":\"untouched\"}");
        assert_eq!(stored.form, vec![property("encoded", "1")]);

        let duplicate = duplicate_rest_request(&connection, &created.id).expect("duplicate");
        assert_eq!(duplicate.body_type, "multipart");
        assert_eq!(duplicate.multipart, stored.multipart);
        assert_eq!(duplicate.files, stored.files);

        delete_rest_request(&connection, &created.id).expect("delete request");
        // Rows belong to a request, so deleting the source leaves the duplicate intact.
        assert_eq!(row_counts(&connection), (1, 2));
        delete_rest_request(&connection, &duplicate.id).expect("delete duplicate");
        // Both requests are gone, and their part rows went with them.
        assert_eq!(row_counts(&connection), (0, 0));
    }

    #[test]
    fn updating_an_unknown_request_writes_nothing() {
        let connection = memory_db();
        let update = RestRequestUpdate {
            id: "missing".to_string(),
            project_id: None,
            name: "Ghost".to_string(),
            method: "POST".to_string(),
            url: "https://example.com".to_string(),
            body: String::new(),
            body_type: "multipart".to_string(),
            save_response: false,
            headers: vec![property("Accept", "*/*")],
            form: vec![property("encoded", "1")],
            multipart: vec![property("comment", "hello")],
            files: vec![file_row("document", "/tmp/report.txt", "report.txt")],
            ..Default::default()
        };
        assert!(update_rest_request(&connection, &update).is_err());
        // Child rows belong to a request that exists, so an unknown id leaves nothing
        // behind that no request can read or delete.
        assert_eq!(row_counts(&connection), (0, 0));
        for table in ["rest_request_properties", "rest_request_form"] {
            let count: i64 = connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .expect("row count");
            assert_eq!(count, 0, "{table}");
        }
    }

    #[test]
    fn duplicates_and_deletes_requests() {
        let connection = memory_db();
        let project = create_rest_project(&connection, "P").expect("project");
        let draft = RestRequestDraft {
            project_id: Some(project.id.clone()),
            name: "Request".to_string(),
            method: "GET".to_string(),
            url: "https://example.com".to_string(),
            body: String::new(),
            body_type: "json".to_string(),
            save_response: false,
            headers: vec![property("Accept", "*/*")],
            form: vec![],
            multipart: vec![],
            files: vec![],
            ..Default::default()
        };
        let created = create_rest_request(&connection, &draft).expect("request");
        let duplicate = duplicate_rest_request(&connection, &created.id).expect("duplicate");
        assert_eq!(duplicate.name, "Request copy");
        assert_eq!(duplicate.headers, created.headers);
        assert_eq!(list_rest_requests(&connection).expect("list").len(), 2);

        delete_rest_project(&connection, &project.id).expect("delete project");
        let unassigned = list_rest_requests(&connection).expect("list");
        assert_eq!(unassigned.len(), 2);
        assert!(unassigned
            .iter()
            .all(|request| request.project_id.is_none()));

        delete_rest_request(&connection, &created.id).expect("delete request");
        assert_eq!(list_rest_requests(&connection).expect("list").len(), 1);
    }

    fn simple_draft(name: &str) -> RestRequestDraft {
        RestRequestDraft {
            project_id: None,
            name: name.to_string(),
            method: "GET".to_string(),
            url: "https://example.com".to_string(),
            body: String::new(),
            body_type: "json".to_string(),
            save_response: false,
            headers: vec![],
            form: vec![],
            multipart: vec![],
            files: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn keeps_projects_and_requests_in_creation_order() {
        let connection = memory_db();
        let first = create_rest_project(&connection, "First").expect("project");
        let second = create_rest_project(&connection, "Second").expect("project");
        assert_eq!(
            list_rest_projects(&connection).expect("list"),
            vec![first.clone(), second.clone()]
        );

        let a = create_rest_request(&connection, &simple_draft("Alpha")).expect("alpha");
        let b = create_rest_request(&connection, &simple_draft("Beta")).expect("beta");
        let c = create_rest_request(&connection, &simple_draft("Gamma")).expect("gamma");
        let listed = list_rest_requests(&connection).expect("list");
        assert_eq!(listed, vec![a.clone(), b.clone(), c.clone()]);

        // Editing a request must not move it in the list.
        rename_rest_request(&connection, &a.id, "Alpha v2").expect("rename");
        let renamed = list_rest_requests(&connection).expect("list");
        assert_eq!(renamed[0].name, "Alpha v2");

        // Deleting the middle request keeps the other two in their relative order.
        delete_rest_request(&connection, &b.id).expect("delete beta");
        let after_delete = list_rest_requests(&connection).expect("list");
        assert_eq!(after_delete.len(), 2);
        assert_eq!(after_delete[0].id, a.id);
        assert_eq!(after_delete[1].id, c.id);
    }

    #[tokio::test]
    async fn cancel_state_signals_only_the_running_request() {
        let cancel = RestCancelState::default();
        assert!(
            !cancel.cancel(),
            "nothing runs, so there is nothing to abort"
        );

        let (id, receiver) = cancel.begin();
        assert!(cancel.cancel(), "the running request is aborted");
        receiver
            .await
            .expect("the abort signal reached the awaiting request");

        // The first cancel took the slot, so a second one finds nothing.
        assert!(!cancel.cancel(), "the slot was already taken");
        cancel.clear(id);
        assert!(!cancel.cancel(), "clearing leaves nothing behind");
    }

    #[tokio::test]
    async fn concurrent_requests_do_not_abort_each_other() {
        use tokio::sync::oneshot::error::TryRecvError;

        let cancel = RestCancelState::default();
        let (first_id, mut first) = cancel.begin();
        let (_second_id, mut second) = cancel.begin();

        // Starting a second call keeps the first sender alive, so the first receiver
        // is still waiting rather than answering `Closed`.
        assert!(matches!(first.try_recv(), Err(TryRecvError::Empty)));

        // A call that finished must not abort a call started after it.
        cancel.clear(first_id);
        assert!(matches!(second.try_recv(), Err(TryRecvError::Empty)));

        assert!(cancel.cancel(), "the second call is still cancellable");
        assert!(second.try_recv().is_ok(), "and receives the abort signal");
    }

    fn plain_values(entries: &[(&str, &str)]) -> VarValues {
        let mut values = VarValues::default();
        for (name, value) in entries {
            let key = name.to_ascii_lowercase();
            values.real.insert(key.clone(), value.to_string());
            values.masked.insert(key, value.to_string());
        }
        values
    }

    fn values_with_a_secret(secure: &[&str]) -> VarValues {
        let mut values = VarValues::default();
        for name in secure {
            let key = name.to_ascii_lowercase();
            values.real.insert(key.clone(), "s3cret".to_string());
            values.masked.insert(key, vars::SECRET_MASK.to_string());
        }
        values
    }

    #[test]
    fn variables_are_substituted_in_query_headers_and_body() {
        let values = values_with_a_secret(&["token"]);
        let call = prepare_call(
            "POST",
            "https://example.com/api?a=%7B%7Btoken%7D%7D",
            &[property("Authorization", "Bearer {{ token }}")],
            "{\"token\":\"{{token}}\"}",
            "json",
            &[],
            &[],
            &[],
            &values,
        )
        .expect("prepared call");

        assert_eq!(call.url.to_string(), "https://example.com/api?a=s3cret");
        assert_eq!(
            call.body,
            "{\"token\":\"s3cret\"}".as_bytes().to_vec()
        );
        assert!(call
            .headers
            .iter()
            .any(|(name, value)| name.as_str() == "authorization" && value == "Bearer s3cret"));

        // The stored dump shows the template as a mask, never the real secret.
        assert!(call.raw_request.contains("Bearer ********"));
        assert!(!call.raw_request.contains("s3cret"));
    }

    #[test]
    fn a_query_value_is_reencoded_after_substitution() {
        let values = plain_values(&[("q", "hello world")]);
        let call = prepare_call(
            "GET",
            "https://example.com/search?q={{q}}&keep=a%20b",
            &[],
            "",
            "json",
            &[],
            &[],
            &[],
            &values,
        )
        .expect("prepared call");
        // `query_pairs` decodes, so both the `+` and the `%20` spellings are found, and the
        // rebuilt query re-encodes every value with the form rules: a space becomes `+`.
        assert_eq!(
            call.url.to_string(),
            "https://example.com/search?q=hello+world&keep=a+b"
        );
    }

    #[test]
    fn a_request_without_variables_is_left_exact() {
        let call = prepare_call(
            "GET",
            "https://example.com/p?a=b%20c&d=+x",
            &[],
            "",
            "json",
            &[],
            &[],
            &[],
            &VarValues::default(),
        )
        .expect("prepared call");
        assert_eq!(call.url.to_string(), "https://example.com/p?a=b%20c&d=+x");
    }

    #[test]
    fn an_unknown_variable_blocks_the_request() {
        let error = prepare_call(
            "POST",
            "https://example.com/p",
            &[],
            "{\"a\":\"{{missing}}\"}",
            "json",
            &[],
            &[],
            &[],
            &VarValues::default(),
        )
        .unwrap_err();
        assert_eq!(error, "unknown-var:missing");
    }

    #[test]
    fn a_template_in_an_unsupported_place_is_reported() {
        let values = plain_values(&[("host", "example.com")]);
        let path_error = prepare_call(
            "GET",
            "https://example.com/{{host}}",
            &[],
            "",
            "json",
            &[],
            &[],
            &[],
            &values,
        )
        .unwrap_err();
        assert_eq!(path_error, "unresolved-template:host");

        let name_error = prepare_call(
            "GET",
            "https://example.com/p",
            &[property("{{host}}", "value")],
            "",
            "json",
            &[],
            &[],
            &[],
            &values,
        )
        .unwrap_err();
        assert_eq!(name_error, "unresolved-template:host");
    }

    #[test]
    fn a_template_in_a_query_name_or_a_part_name_is_reported() {
        let values = plain_values(&[("token", "x")]);

        let query_error = prepare_call(
            "GET",
            "https://example.com/p?{{token}}=1",
            &[],
            "",
            "json",
            &[],
            &[],
            &[],
            &values,
        )
        .unwrap_err();
        assert_eq!(query_error, "unresolved-template:token");

        // The Query table writes the name percent-encoded, and `query_pairs` decodes it
        // back, so both spellings are caught.
        let encoded_error = prepare_call(
            "GET",
            "https://example.com/p?%7B%7Btoken%7D%7D=1",
            &[],
            "",
            "json",
            &[],
            &[],
            &[],
            &values,
        )
        .unwrap_err();
        assert_eq!(encoded_error, "unresolved-template:token");

        let part_error = prepare_call(
            "POST",
            "https://example.com/p",
            &[],
            "",
            "multipart",
            &[],
            &[property("{{token}}", "value")],
            &[],
            &values,
        )
        .unwrap_err();
        assert_eq!(part_error, "unresolved-template:token");

        let file_error = prepare_call(
            "POST",
            "https://example.com/p",
            &[],
            "",
            "multipart",
            &[],
            &[],
            &[file_row("part", "{{token}}/report.pdf", "report.pdf")],
            &values,
        )
        .unwrap_err();
        assert_eq!(file_error, "unresolved-template:token");
    }

    #[test]
    fn form_and_multipart_values_are_substituted() {
        let values = plain_values(&[("name", "a b")]);
        let form = prepare_call(
            "POST",
            "https://example.com/p",
            &[],
            "",
            "form",
            &[property("field", "{{name}}")],
            &[],
            &[],
            &values,
        )
        .expect("form call");
        assert_eq!(form.body, b"field=a+b".to_vec());

        let directory = std::env::temp_dir().join(format!("rest-vars-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("temp dir");
        let path = directory.join("note.txt");
        std::fs::write(&path, b"payload").expect("fixture");
        let files = vec![RestFileRow {
            name: "document".to_string(),
            path: path.display().to_string(),
            filename: String::new(),
        }];

        let call = prepare_call(
            "POST",
            "https://example.com/p",
            &[],
            "",
            "multipart",
            &[],
            &[property("field", "{{name}}")],
            &files,
            &values,
        )
        .expect("multipart call");

        // The masked dump keeps the framing and the real length of the wire body.
        assert!(call.raw_request.contains("content-length: "));
        assert!(call
            .raw_request
            .contains(&format!("content-length: {}", call.body.len())));
        assert!(call.raw_request.contains("name=\"field\""));
        assert!(call.raw_request.contains("\r\n\r\na b\r\n"));
        assert!(String::from_utf8_lossy(&call.body).contains("a b"));
        assert!(!call.raw_request.contains("payload"));

        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_substituted_line_break_is_not_allowed_in_a_multipart_field() {
        let values = plain_values(&[("value", "one\r\ntwo")]);
        let error = prepare_call(
            "POST",
            "https://example.com/p",
            &[],
            "",
            "multipart",
            &[],
            &[property("field", "{{value}}")],
            &[],
            &values,
        )
        .unwrap_err();
        assert!(error.contains("cannot contain a line break"));
    }
}
