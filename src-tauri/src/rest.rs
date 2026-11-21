use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::header::{HeaderName, HeaderValue};
use reqwest::{Client, Method, Url};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::db::AppDb;

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
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RestResponse {
    pub status: u16,
    pub status_text: String,
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
    sender: Arc<Mutex<Option<oneshot::Sender<()>>>>,
}

impl RestCancelState {
    fn begin(&self) -> oneshot::Receiver<()> {
        let (sender, receiver) = oneshot::channel();
        if let Ok(mut slot) = self.sender.lock() {
            *slot = Some(sender);
        }
        receiver
    }

    fn clear(&self) {
        if let Ok(mut slot) = self.sender.lock() {
            *slot = None;
        }
    }

    fn cancel(&self) -> bool {
        self.sender
            .lock()
            .ok()
            .and_then(|mut slot| slot.take())
            .map(|sender| sender.send(()).is_ok())
            .unwrap_or(false)
    }
}

fn next_sort_order(conn: &Connection, table: &str) -> rusqlite::Result<i64> {
    let sql = format!("SELECT COALESCE(MAX(sort_order), -1) + 1 FROM {table}");
    conn.query_row(&sql, [], |row| row.get(0))
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
		       response_body, response_headers, raw_request, created_at, updated_at
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
    conn.execute(
        "UPDATE rest_requests SET project_id = NULL, updated_at = CURRENT_TIMESTAMP WHERE project_id = ?1",
        params![id],
    )
    .map_err(|error| error.to_string())?;
    conn.execute("DELETE FROM rest_projects WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
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
    conn.execute(
        r#"
		INSERT INTO rest_requests
			(id, project_id, name, method, url, body, body_type, save_response, sort_order)
		VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
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
            sort_order
        ],
    )
    .map_err(|error| error.to_string())?;
    write_properties(conn, &id, "header", &draft.headers).map_err(|error| error.to_string())?;
    write_form(conn, &id, &draft.form).map_err(|error| error.to_string())?;
    write_multipart(conn, &id, &draft.multipart).map_err(|error| error.to_string())?;
    write_files(conn, &id, &draft.files).map_err(|error| error.to_string())?;

    read_rest_request(conn, &id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Could not read the created request".to_string())
}

pub fn update_rest_request(conn: &Connection, update: &RestRequestUpdate) -> Result<(), String> {
    conn.execute(
        r#"
		UPDATE rest_requests SET
			project_id = ?2,
			name = ?3,
			method = ?4,
			url = ?5,
			body = ?6,
			body_type = ?7,
			save_response = ?8,
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
            i64::from(update.save_response)
        ],
    )
    .map_err(|error| error.to_string())?;
    write_properties(conn, &update.id, "header", &update.headers)
        .map_err(|error| error.to_string())?;
    write_form(conn, &update.id, &update.form).map_err(|error| error.to_string())?;
    write_multipart(conn, &update.id, &update.multipart).map_err(|error| error.to_string())?;
    write_files(conn, &update.id, &update.files).map_err(|error| error.to_string())?;
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
    conn.execute(
        "DELETE FROM rest_request_properties WHERE request_id = ?1",
        params![id],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "DELETE FROM rest_request_form WHERE request_id = ?1",
        params![id],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "DELETE FROM rest_request_multipart WHERE request_id = ?1",
        params![id],
    )
    .map_err(|error| error.to_string())?;
    conn.execute(
        "DELETE FROM rest_request_files WHERE request_id = ?1",
        params![id],
    )
    .map_err(|error| error.to_string())?;
    conn.execute("DELETE FROM rest_requests WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
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
    };
    create_rest_request(conn, &draft)
}

pub fn save_rest_response(
    conn: &Connection,
    id: &str,
    body: &str,
    headers: &[RestProperty],
    raw_request: &str,
) -> Result<(), String> {
    let encoded = serde_json::to_string(headers).map_err(|error| error.to_string())?;
    conn.execute(
        r#"
		UPDATE rest_requests SET
			response_body = ?2,
			response_headers = ?3,
			raw_request = ?4,
			updated_at = CURRENT_TIMESTAMP
		WHERE id = ?1
		"#,
        params![id, body, encoded, raw_request],
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
			updated_at = CURRENT_TIMESTAMP
		WHERE id = ?1
		"#,
        params![id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn normalize_rest_url(raw: &str) -> Result<Url, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Enter a request URL".to_string());
    }
    let candidate = if trimmed.contains("://") {
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
        if trimmed.to_ascii_lowercase().contains("<html") {
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

pub fn filename_from_content_disposition(value: &str) -> Option<String> {
    let lower = value.to_ascii_lowercase();
    if let Some(index) = lower.find("filename*=") {
        let raw = value[index + "filename*=".len()..].trim();
        let raw = raw
            .split(';')
            .next()
            .unwrap_or(raw)
            .trim()
            .trim_matches('"');
        let encoded = raw.split("''").nth(1).unwrap_or(raw);
        if let Ok(decoded) = percent_decode(encoded) {
            if !decoded.is_empty() {
                return Some(decoded);
            }
        }
    }
    if let Some(index) = lower.find("filename=") {
        let raw = value[index + "filename=".len()..].trim();
        let raw = raw.split(';').next().unwrap_or(raw).trim();
        let name = raw.trim_matches('"').trim_matches('\'').trim();
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    None
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
) -> Result<PreparedCall, String> {
    let parsed = normalize_rest_url(url)?;
    let method = Method::from_bytes(method.trim().to_ascii_uppercase().as_bytes())
        .map_err(|_| format!("Invalid HTTP method: {method}"))?;
    let boundary = match body_type {
        "multipart" => Some(multipart_boundary()),
        _ => None,
    };
    let resolved = resolve_body(
        body_type,
        body,
        form,
        multipart,
        files,
        boundary
            .as_deref()
            .unwrap_or(MULTIPART_BOUNDARY_PLACEHOLDER),
    )?;
    let prepared = prepare_headers(&parsed, headers, &resolved, body_type, boundary.as_deref());
    let wire_headers = prepared
        .iter()
        .filter(|header| {
            !header.name.eq_ignore_ascii_case("host")
                && !header.name.eq_ignore_ascii_case("content-length")
        })
        .cloned()
        .collect::<Vec<_>>();
    let header_map = build_headers_map(&wire_headers)?;
    let raw_request = build_raw_request(method.as_str(), &parsed, &prepared, &resolved.dump);
    Ok(PreparedCall {
        method,
        url: parsed,
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
) -> Result<(), String> {
    let connection = db.conn.lock().map_err(|error| error.to_string())?;
    save_rest_response(&connection, &id, &body, &headers, &raw_request)
}

#[tauri::command(rename = "clear_rest_response", rename_all = "snake_case")]
pub fn cmd_clear_rest_response(db: tauri::State<'_, AppDb>, id: String) -> Result<(), String> {
    let connection = db.conn.lock().map_err(|error| error.to_string())?;
    clear_rest_response(&connection, &id)
}

#[tauri::command(rename = "execute_rest_request", rename_all = "snake_case")]
pub async fn cmd_execute_rest_request(
    cancel: tauri::State<'_, RestCancelState>,
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
        &multipart.unwrap_or_default(),
        &files.unwrap_or_default(),
    )?;
    let client = build_client()?;
    let receiver = cancel.begin();

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
        let headers = response_headers(&response);
        let content_type = header_value(&headers, "content-type");
        let disposition = header_value(&headers, "content-disposition");
        let is_attachment = disposition.to_ascii_lowercase().contains("attachment");
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
    cancel.clear();
    result
}

#[tauri::command(rename = "cancel_rest_request", rename_all = "snake_case")]
pub fn cmd_cancel_rest_request(cancel: tauri::State<'_, RestCancelState>) -> bool {
    cancel.cancel()
}

#[tauri::command(rename = "download_rest_response", rename_all = "snake_case")]
pub async fn cmd_download_rest_response(
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
    let call = prepare_call(
        &method,
        &url,
        &headers,
        &body,
        &body_type,
        &form,
        &multipart.unwrap_or_default(),
        &files.unwrap_or_default(),
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
    fn parses_content_disposition() {
        assert_eq!(
            filename_from_content_disposition("attachment; filename=\"report.csv\""),
            Some("report.csv".to_string())
        );
        assert_eq!(
            filename_from_content_disposition("attachment; filename*=UTF-8''my%20file.txt"),
            Some("my file.txt".to_string())
        );
        assert_eq!(filename_from_content_disposition("inline"), None);
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

        clear_rest_response(&connection, &created.id).expect("clear");
        let cleared = read_rest_request(&connection, &created.id)
            .expect("read")
            .expect("present");
        assert_eq!(cleared.response_body, None);
        assert!(cleared.response_headers.is_empty());
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

        let receiver = cancel.begin();
        assert!(cancel.cancel(), "the running request is aborted");
        receiver
            .await
            .expect("the abort signal reached the awaiting request");

        // The first cancel took the slot, so a second one finds nothing.
        assert!(!cancel.cancel(), "the slot was already taken");
        cancel.clear();
        assert!(!cancel.cancel(), "clearing leaves nothing behind");
    }
}
