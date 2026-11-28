use std::collections::HashMap;

use reqwest::Url;
use rusqlite::ffi;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One namespace for every secret the app keeps in the OS credential store, so a
/// renamed or deleted variable never collides with another program's entry.
pub const KEYRING_SERVICE: &str = "app.webdev.usefultools.global-vars";
/// The text a safe variable shows in a saved raw-request dump: the real value and its
/// length never reach the database or the frontend.
pub const SECRET_MASK: &str = "********";
/// Windows Credential Manager refuses anything longer than this. Counted in characters,
/// which is what the localized limit says; a Cyrillic secret has more bytes than chars.
pub const MAX_SECRET_CHARS: usize = 2048;
pub const MAX_NAME_CHARS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalVar {
    pub id: String,
    pub name: String,
    pub secure: bool,
    /// `None` for a secure variable: its value never leaves the keyring without an
    /// explicit reveal.
    pub value: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GlobalVarInput {
    pub name: String,
    #[serde(default)]
    pub secure: bool,
    /// `None` keeps the current value, `Some("")` clears it.
    #[serde(default)]
    pub value: Option<String>,
}

/// Values a single request needs. `real` goes on the wire, `masked` goes into the
/// raw-request dump, where a secret is replaced by `SECRET_MASK`.
#[derive(Clone, Debug, Default)]
pub struct VarValues {
    pub real: HashMap<String, String>,
    pub masked: HashMap<String, String>,
}

/// The secret side of a variable. Kept behind a trait so the tests never touch the OS
/// credential store.
pub trait SecretStore: Send + Sync {
    fn get(&self, id: &str) -> Result<Option<String>, String>;
    fn set(&self, id: &str, value: &str) -> Result<(), String>;
    fn delete(&self, id: &str) -> Result<(), String>;
}

pub struct KeyringSecrets {
    app: tauri::AppHandle,
}

impl KeyringSecrets {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl SecretStore for KeyringSecrets {
    fn get(&self, id: &str) -> Result<Option<String>, String> {
        // `get_password` answers `Ok(None)` for both a missing entry and an
        // unreachable store, so "no secret" is the only reading here.
        use tauri_plugin_keyring::KeyringExt;
        self.app
            .keyring()
            .get_password(KEYRING_SERVICE, id)
            .map_err(|error| format!("secret-store:{error}"))
    }

    fn set(&self, id: &str, value: &str) -> Result<(), String> {
        use tauri_plugin_keyring::KeyringExt;
        self.app
            .keyring()
            .set_password(KEYRING_SERVICE, id, value)
            .map_err(|error| format!("secret-store:{error}"))
    }

    fn delete(&self, id: &str) -> Result<(), String> {
        use tauri_plugin_keyring::KeyringExt;
        self.app
            .keyring()
            .delete_password(KEYRING_SERVICE, id)
            .map_err(|error| format!("secret-store:{error}"))
    }
}

fn is_valid_name(name: &str) -> bool {
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
        return false;
    }
    let mut chars = name.chars();
    let first = match chars.next() {
        Some(char) => char,
        None => return false,
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|char| char.is_ascii_alphanumeric() || char == '_')
}

/// The template starting exactly at `start`: its lowercase name and the index just
/// after the closing braces. Anything that is not a valid name (`{{ a b }}`, `{{a}}}`
/// nesting) is not a template.
fn template_at(chars: &[char], start: usize) -> Option<(String, usize)> {
    if start + 1 >= chars.len() || chars[start] != '{' || chars[start + 1] != '{' {
        return None;
    }
    let mut index = start + 2;
    while index + 1 < chars.len() {
        if chars[index] == '}' && chars[index + 1] == '}' {
            let inner: String = chars[start + 2..index].iter().collect();
            let name = inner.trim();
            if is_valid_name(name) {
                return Some((name.to_ascii_lowercase(), index + 2));
            }
            return None;
        }
        index += 1;
    }
    None
}

/// Every distinct variable name referenced by `text`, in the order it appears.
pub fn template_names(text: &str) -> Vec<String> {
    if !text.contains("{{") {
        return Vec::new();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut names: Vec<String> = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if let Some((name, next)) = template_at(&chars, index) {
            if !names.contains(&name) {
                names.push(name);
            }
            index = next;
        } else {
            index += 1;
        }
    }
    names
}

pub fn has_template(text: &str) -> bool {
    !template_names(text).is_empty()
}

/// One pass over `text`: a `{{ name }}` is replaced by its value, and a value is never
/// scanned again, so a value containing `{{other}}` stays literal.
pub fn substitute(text: &str, values: &HashMap<String, String>) -> Result<String, String> {
    if !text.contains("{{") {
        return Ok(text.to_string());
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < chars.len() {
        if let Some((name, next)) = template_at(&chars, index) {
            match values.get(&name) {
                Some(value) => {
                    out.push_str(value);
                    index = next;
                }
                None => return Err(format!("unknown-var:{name}")),
            }
        } else {
            out.push(chars[index]);
            index += 1;
        }
    }
    Ok(out)
}

/// `Url::parse()` keeps `{` and `}` percent-encoded, so a template typed in the path
/// reaches any scanner as `%7B%7Bname%7D%7D`. Only the braces are decoded here: the path
/// is never a template position, this is the check that says so.
pub fn decoded_braces(text: &str) -> String {
    text.replace("%7B", "{")
        .replace("%7b", "{")
        .replace("%7D", "}")
        .replace("%7d", "}")
}

/// Substitutes inside the query values only. Host, path and parameter names are not
/// template positions, so a template there is reported instead of being sent literally.
/// The URL is rebuilt only when a value actually changed, which keeps an untouched URL
/// byte-identical.
pub fn substitute_url(url: &Url, values: &HashMap<String, String>) -> Result<Url, String> {
    if has_template(url.host_str().unwrap_or_default()) {
        return Err("unresolved-template:host".to_string());
    }
    if let Some(name) = first_template_name(&decoded_braces(url.path())) {
        return Err(format!("unresolved-template:{name}"));
    }
    let Some(raw_query) = url.query() else {
        return Ok(url.clone());
    };

    // The raw segments are what gets written back, so a pair whose value did not change
    // keeps the spelling the user typed: a bare flag gains no `=`, an empty pair survives
    // and an existing `%20` is not rewritten as `+`. `form_decode` is the decoding
    // `query_pairs` would apply, which is the form a template typed into the Query table
    // arrives in.
    let segments: Vec<&str> = raw_query.split('&').collect();

    let mut rebuilt: Vec<String> = Vec::with_capacity(segments.len());
    let mut changed = false;
    for segment in segments {
        let (raw_name, raw_value) = match segment.split_once('=') {
            Some((name, value)) => (name, value),
            None => (segment, ""),
        };
        if let Some(inner) = first_template_name(&form_decode(raw_name)) {
            return Err(format!("unresolved-template:{inner}"));
        }
        let value = form_decode(raw_value);
        let replaced = substitute(&value, values)?;
        if replaced == value {
            rebuilt.push(segment.to_string());
            continue;
        }
        changed = true;
        rebuilt.push(format!("{raw_name}={}", form_encode(&replaced)));
    }

    if !changed {
        return Ok(url.clone());
    }
    let mut result = url.clone();
    result.set_query(Some(&rebuilt.join("&")));
    Ok(result)
}

pub fn first_template_name(text: &str) -> Option<String> {
    template_names(text).first().cloned()
}

/// Reads the values of the variables a request mentions. A name with no row, or a
/// secure row whose keyring entry is gone, is an error rather than an empty string: a
/// request must never silently send `{{ token }}` or an empty token.
pub fn load_values(
    conn: &Connection,
    secrets: &dyn SecretStore,
    referenced: &[String],
) -> Result<VarValues, String> {
    let mut values = VarValues::default();
    for name in referenced {
        let key = name.to_ascii_lowercase();
        if values.real.contains_key(&key) {
            continue;
        }
        let row = conn
            .query_row(
                "SELECT id, secure, value FROM global_vars WHERE name = ?1 COLLATE NOCASE",
                params![name],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0, row.get::<_, String>(2)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;

        match row {
            None => return Err(format!("unknown-var:{key}")),
            Some((id, secure, plain)) => {
                if secure {
                    let secret = secrets
                        .get(&id)?
                        .filter(|secret| !secret.is_empty())
                        .ok_or_else(|| format!("secret-missing:{key}"))?;
                    values.real.insert(key.clone(), secret);
                    values.masked.insert(key, SECRET_MASK.to_string());
                } else {
                    values.real.insert(key.clone(), plain.clone());
                    values.masked.insert(key, plain);
                }
            }
        }
    }
    Ok(values)
}

fn find_row(conn: &Connection, id: &str) -> Result<Option<(String, bool, String)>, String> {
    conn.query_row(
        "SELECT name, secure, value FROM global_vars WHERE id = ?1",
        params![id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? != 0, row.get::<_, String>(2)?)),
    )
    .optional()
    .map_err(|error| error.to_string())
}

/// A name already taken by another row, ignoring case.
fn duplicate_name(conn: &Connection, name: &str, except_id: &str) -> Result<Option<String>, String> {
    let taken = conn
        .query_row(
            "SELECT name FROM global_vars WHERE name = ?1 COLLATE NOCASE AND id != ?2",
            params![name, except_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    Ok(taken)
}

/// The name check above is only a friendly early answer: two writes can pass it at the
/// same time, and then the unique index is what refuses the row. Its error text names
/// SQLite internals, so a constraint violation becomes the same code the check emits.
fn duplicate_error(name: &str, error: rusqlite::Error) -> String {
    if let rusqlite::Error::SqliteFailure(ffi::Error { code, .. }, _) = &error {
        if *code == ffi::ErrorCode::ConstraintViolation {
            return format!("duplicate-name:{name}");
        }
    }
    error.to_string()
}

fn check_secret_size(value: &str) -> Result<(), String> {
    if value.chars().count() > MAX_SECRET_CHARS {
        return Err("secret-too-large".to_string());
    }
    Ok(())
}

/// Form encoding of a substituted query value, the same rules `query_pairs_mut` applies:
/// alphanumerics and `*`, `-`, `.`, `_` stay, a space becomes `+`, everything else is
/// percent-escaped with uppercase hex.
fn form_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push('+'),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

/// The decoding `query_pairs` applies: `+` is a space and a complete `%XX` escape becomes
/// its byte, so a template typed into the Query table arrives in the form `substitute`
/// reads. An incomplete escape stays literal.
fn form_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
                    .ok()
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok());
                match hex {
                    Some(byte) => {
                        decoded.push(byte);
                        index += 3;
                    }
                    None => {
                        decoded.push(b'%');
                        index += 1;
                    }
                }
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn validate_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if !is_valid_name(trimmed) {
        return Err(format!("invalid-name:{trimmed}"));
    }
    Ok(trimmed.to_string())
}

pub fn list(conn: &Connection) -> Result<Vec<GlobalVar>, String> {
    let mut statement = conn
        .prepare(
            "SELECT id, name, secure, value, created_at, updated_at FROM global_vars ORDER BY name COLLATE NOCASE",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(GlobalVar {
                id: row.get::<_, String>(0)?,
                name: row.get::<_, String>(1)?,
                secure: row.get::<_, i64>(2)? != 0,
                // A secure row stores an empty column: the value lives in the keyring.
                value: if row.get::<_, i64>(2)? != 0 {
                    None
                } else {
                    Some(row.get::<_, String>(3)?)
                },
                created_at: row.get::<_, String>(4)?,
                updated_at: row.get::<_, String>(5)?,
            })
        })
        .map_err(|error| error.to_string())?;

    let mut vars = Vec::new();
    for row in rows {
        vars.push(row.map_err(|error| error.to_string())?);
    }
    Ok(vars)
}

pub fn create(
    conn: &Connection,
    secrets: &dyn SecretStore,
    input: &GlobalVarInput,
) -> Result<GlobalVar, String> {
    let name = validate_name(&input.name)?;
    if duplicate_name(conn, &name, "")?.is_some() {
        return Err(format!("duplicate-name:{name}"));
    }

    let id = Uuid::new_v4().to_string();
    let value = input.value.clone().unwrap_or_default();

    if input.secure {
        check_secret_size(&value)?;
        // An empty secret is not written to the store: there is nothing to keep, and a
        // send that later reads it must fail loudly instead of sending an empty token.
        if !value.is_empty() {
            secrets.set(&id, &value)?;
        }
        if let Err(error) = conn.execute(
            "INSERT INTO global_vars (id, name, secure, value, sort_order) VALUES (?1, ?2, 1, '', ?3)",
            params![id, name, count_rows(conn)?],
        ) {
            // The keyring entry would otherwise outlive the row that names it.
            secrets.delete(&id).ok();
            return Err(duplicate_error(&name, error));
        }
    } else {
        conn.execute(
            "INSERT INTO global_vars (id, name, secure, value, sort_order) VALUES (?1, ?2, 0, ?3, ?4)",
            params![id, name, value, count_rows(conn)?],
        )
        .map_err(|error| duplicate_error(&name, error))?;
    }

    read_one(conn, &id)
}

fn count_rows(conn: &Connection) -> Result<i64, String> {
    conn.prepare("SELECT COUNT(*) FROM global_vars")
        .map_err(|error| error.to_string())?
        .query_row([], |row| row.get::<_, i64>(0))
        .map_err(|error| error.to_string())
}

fn read_one(conn: &Connection, id: &str) -> Result<GlobalVar, String> {
    conn.query_row(
        "SELECT id, name, secure, value, created_at, updated_at FROM global_vars WHERE id = ?1",
        params![id],
        |row| {
            Ok(GlobalVar {
                id: row.get::<_, String>(0)?,
                name: row.get::<_, String>(1)?,
                secure: row.get::<_, i64>(2)? != 0,
                value: if row.get::<_, i64>(2)? != 0 {
                    None
                } else {
                    Some(row.get::<_, String>(3)?)
                },
                created_at: row.get::<_, String>(4)?,
                updated_at: row.get::<_, String>(5)?,
            })
        }
    )
    .optional()
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "Global variable not found".to_string())
}

pub fn update(
    conn: &Connection,
    secrets: &dyn SecretStore,
    id: &str,
    input: &GlobalVarInput,
) -> Result<GlobalVar, String> {
    let existing = match find_row(conn, id)? {
        Some(row) => row,
        None => return Err("Global variable not found".to_string()),
    };
    let (current_name, was_secure, current_value) = existing;
    let name = validate_name(&input.name)?;
    if duplicate_name(conn, &name, id)?.is_some() {
        return Err(format!("duplicate-name:{name}"));
    }

    // Renaming never touches the keyring: the account is the id, not the name.
    if was_secure && input.secure {
        if let Some(value) = &input.value {
            check_secret_size(value)?;
            if value.is_empty() {
                // Clearing a revealed secret erases the entry: the row now has no secret
                // and refuses to be sent until one is typed again.
                secrets.delete(id).ok();
            } else {
                secrets.set(id, value)?;
            }
        }
        conn.execute(
            "UPDATE global_vars SET name = ?1, secure = 1, value = '', updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![name, id],
        )
        .map_err(|error| duplicate_error(&name, error))?;
        return read_one(conn, id);
    }

    if was_secure && !input.secure {
        // secure -> plain: the secret is pulled into SQLite on purpose, then erased.
        // The column has no size limit, so the keyring's limit no longer applies here.
        let value = match &input.value {
            Some(value) => value.clone(),
            None => secrets
                .get(id)?
                .ok_or_else(|| format!("secret-missing:{current_name}"))?,
        };
        conn.execute(
            "UPDATE global_vars SET name = ?1, secure = 0, value = ?2, updated_at = CURRENT_TIMESTAMP WHERE id = ?3",
            params![name, value, id],
        )
        .map_err(|error| duplicate_error(&name, error))?;
        // The row is already plain, so a failed erase is not reported: a repeated
        // delete of the same account is idempotent.
        secrets.delete(id).ok();
        return read_one(conn, id);
    }

    if !was_secure && input.secure {
        // plain -> secure: the value moves into the keyring before the column is emptied.
        let value = input.value.clone().unwrap_or_else(|| current_value.clone());
        check_secret_size(&value)?;
        if !value.is_empty() {
            secrets.set(id, &value)?;
        }
        if let Err(error) = conn.execute(
            "UPDATE global_vars SET name = ?1, secure = 1, value = '', updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
            params![name, id],
        ) {
            secrets.delete(id).ok();
            return Err(duplicate_error(&name, error));
        }
        return read_one(conn, id);
    }

    let value = input.value.clone().unwrap_or_else(|| current_value.clone());
    conn.execute(
        "UPDATE global_vars SET name = ?1, secure = 0, value = ?2, updated_at = CURRENT_TIMESTAMP WHERE id = ?3",
        params![name, value, id],
    )
    .map_err(|error| duplicate_error(&name, error))?;
    read_one(conn, id)
}

pub fn delete(conn: &Connection, secrets: &dyn SecretStore, id: &str) -> Result<(), String> {
    let existing = match find_row(conn, id)? {
        Some(row) => row,
        None => return Err("Global variable not found".to_string()),
    };
    // `delete_credential` answers "not found" for a secure row that never got a secret,
    // which is not an error. A store that cannot be reached still loses the row: the
    // account is keyed by an id nothing references any more.
    if existing.1 {
        secrets.delete(id).ok();
    }
    conn.execute("DELETE FROM global_vars WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn reveal(conn: &Connection, secrets: &dyn SecretStore, id: &str) -> Result<String, String> {
    let (_name, secure, value) = match find_row(conn, id)? {
        Some(row) => row,
        None => return Err("Global variable not found".to_string()),
    };
    if !secure {
        return Ok(value);
    }
    // A row whose secret was cleared has no entry; answering with an empty string lets the
    // page show an empty field to type it into. `load_values` is where a missing secret
    // stops a request.
    Ok(secrets.get(id)?.unwrap_or_default())
}

#[tauri::command(rename = "list_global_vars", rename_all = "snake_case")]
pub fn cmd_list_global_vars(db: tauri::State<'_, crate::db::AppDb>) -> Result<Vec<GlobalVar>, String> {
    let connection = db.conn.lock().map_err(|error| error.to_string())?;
    list(&connection)
}

#[tauri::command(rename = "create_global_var", rename_all = "snake_case")]
pub fn cmd_create_global_var(
    db: tauri::State<'_, crate::db::AppDb>,
    app: tauri::AppHandle,
    name: String,
    secure: bool,
    value: Option<String>,
) -> Result<GlobalVar, String> {
    let connection = db.conn.lock().map_err(|error| error.to_string())?;
    create(
        &connection,
        &KeyringSecrets { app },
        &GlobalVarInput { name, secure, value },
    )
}

#[tauri::command(rename = "update_global_var", rename_all = "snake_case")]
pub fn cmd_update_global_var(
    db: tauri::State<'_, crate::db::AppDb>,
    app: tauri::AppHandle,
    id: String,
    name: String,
    secure: bool,
    value: Option<String>,
) -> Result<GlobalVar, String> {
    let connection = db.conn.lock().map_err(|error| error.to_string())?;
    update(
        &connection,
        &KeyringSecrets { app },
        &id,
        &GlobalVarInput { name, secure, value },
    )
}

#[tauri::command(rename = "delete_global_var", rename_all = "snake_case")]
pub fn cmd_delete_global_var(
    db: tauri::State<'_, crate::db::AppDb>,
    app: tauri::AppHandle,
    id: String,
) -> Result<(), String> {
    let connection = db.conn.lock().map_err(|error| error.to_string())?;
    delete(&connection, &KeyringSecrets { app }, &id)
}

#[tauri::command(rename = "reveal_global_var", rename_all = "snake_case")]
pub fn cmd_reveal_global_var(
    db: tauri::State<'_, crate::db::AppDb>,
    app: tauri::AppHandle,
    id: String,
) -> Result<String, String> {
    let connection = db.conn.lock().map_err(|error| error.to_string())?;
    reveal(&connection, &KeyringSecrets { app }, &id)
}

#[cfg(test)]
pub struct MemorySecrets {
    entries: std::sync::Mutex<HashMap<String, String>>,
}

#[cfg(test)]
impl MemorySecrets {
    pub fn new() -> Self {
        MemorySecrets {
            entries: std::sync::Mutex::new(HashMap::new()),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }
}

#[cfg(test)]
impl SecretStore for MemorySecrets {
    fn get(&self, id: &str) -> Result<Option<String>, String> {
        Ok(self.entries.lock().unwrap().get(id).cloned())
    }

    fn set(&self, id: &str, value: &str) -> Result<(), String> {
        self.entries
            .lock()
            .unwrap()
            .insert(id.to_string(), value.to_string());
        Ok(())
    }

    fn delete(&self, id: &str) -> Result<(), String> {
        self.entries.lock().unwrap().remove(id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use rusqlite::Connection;

    fn memory_db() -> Connection {
        let connection = Connection::open_in_memory().expect("in-memory database");
        db::init_schema(&connection).expect("schema");
        connection
    }

    fn input(name: &str, secure: bool, value: Option<&str>) -> GlobalVarInput {
        GlobalVarInput {
            name: name.to_string(),
            secure,
            value: value.map(|value| value.to_string()),
        }
    }

    #[test]
    fn creates_and_lists_a_plain_variable() {
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        let created = create(&conn, &secrets, &input("base", false, Some("https://example.com")))
            .expect("created");
        assert_eq!(created.name, "base");
        assert!(!created.secure);
        assert_eq!(created.value.as_deref(), Some("https://example.com"));
        assert_eq!(list(&conn).expect("list").len(), 1);
    }

    #[test]
    fn a_secure_variable_keeps_no_value_in_sqlite() {
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        let created = create(&conn, &secrets, &input("token", true, Some("s3cret")))
            .expect("created");
        assert!(created.secure);
        assert_eq!(created.value, None);

        let stored: String = conn
            .query_row("SELECT value FROM global_vars WHERE id = ?1", params![created.id], |row| row.get(0))
            .expect("stored value");
        assert_eq!(stored, "");
        assert_eq!(secrets.len(), 1);
        assert_eq!(secrets.get(&created.id).expect("secret").as_deref(), Some("s3cret"));
    }

    #[test]
    fn updating_a_secure_variable_writes_the_keyring_only() {
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        let created = create(&conn, &secrets, &input("token", true, Some("first"))).expect("created");
        let updated = update(&conn, &secrets, &created.id, &input("token", true, Some("second")))
            .expect("updated");
        assert_eq!(updated.value, None);
        assert_eq!(secrets.get(&created.id).expect("secret").as_deref(), Some("second"));
    }

    #[test]
    fn plain_to_secure_moves_the_value_and_secure_to_plain_moves_it_back() {
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        let created = create(&conn, &secrets, &input("key", false, Some("value"))).expect("created");

        let secured = update(&conn, &secrets, &created.id, &input("key", true, None)).expect("secure");
        assert!(secured.secure);
        assert_eq!(secured.value, None);
        assert_eq!(secrets.get(&created.id).expect("secret").as_deref(), Some("value"));

        let unsecured = update(&conn, &secrets, &created.id, &input("key", false, None)).expect("plain");
        assert!(!unsecured.secure);
        assert_eq!(unsecured.value.as_deref(), Some("value"));
        assert_eq!(secrets.len(), 0);
    }

    #[test]
    fn renaming_does_not_touch_the_keyring_account() {
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        let created = create(&conn, &secrets, &input("token", true, Some("s3cret"))).expect("created");
        let renamed = update(&conn, &secrets, &created.id, &input("Token", true, None)).expect("renamed");
        assert_eq!(renamed.name, "Token");
        assert_eq!(secrets.len(), 1);
        assert_eq!(secrets.get(&created.id).expect("secret").as_deref(), Some("s3cret"));
    }

    #[test]
    fn deleting_a_secure_variable_removes_the_secret() {
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        let created = create(&conn, &secrets, &input("token", true, Some("s3cret"))).expect("created");
        delete(&conn, &secrets, &created.id).expect("deleted");
        assert_eq!(secrets.len(), 0);
        assert!(list(&conn).expect("list").is_empty());
    }

    #[test]
    fn deleting_a_secure_variable_without_an_entry_still_removes_the_row() {
        // A row whose secret is already gone (no credential store, or an entry deleted
        // outside the app) must still disappear rather than stay undeletable.
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        let created = create(&conn, &secrets, &input("token", true, None)).expect("secure row");
        assert_eq!(secrets.len(), 0);
        delete(&conn, &secrets, &created.id).expect("deleted");
        assert!(list(&conn).expect("list").is_empty());
    }

    #[test]
    fn secure_to_plain_with_a_revealed_value_writes_that_value() {
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        let created = create(&conn, &secrets, &input("token", true, Some("s3cret"))).expect("created");
        let moved = update(&conn, &secrets, &created.id, &input("token", false, Some("typed")))
            .expect("moved");
        assert_eq!(moved.value.as_deref(), Some("typed"));
        assert_eq!(secrets.len(), 0);
    }

    #[test]
    fn secure_to_plain_with_an_empty_value_leaves_the_column_empty() {
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        let created = create(&conn, &secrets, &input("token", true, Some("s3cret"))).expect("created");
        let moved = update(&conn, &secrets, &created.id, &input("token", false, Some("")))
            .expect("moved");
        assert_eq!(moved.value.as_deref(), Some(""));
        assert_eq!(secrets.len(), 0);
    }

    #[test]
    fn a_duplicate_name_in_another_case_is_refused() {
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        create(&conn, &secrets, &input("Token", false, Some("a"))).expect("first");
        let error = create(&conn, &secrets, &input("token", false, Some("b"))).unwrap_err();
        assert_eq!(error, "duplicate-name:token");
    }

    #[test]
    fn an_invalid_name_is_refused() {
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        assert_eq!(create(&conn, &secrets, &input("a b", false, None)).unwrap_err(), "invalid-name:a b");
        assert_eq!(create(&conn, &secrets, &input("", false, None)).unwrap_err(), "invalid-name:");
        assert_eq!(create(&conn, &secrets, &input("1x", false, None)).unwrap_err(), "invalid-name:1x");
    }

    #[test]
    fn a_secure_value_over_the_limit_is_refused() {
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        let long = "a".repeat(MAX_SECRET_CHARS + 1);
        assert_eq!(
            create(&conn, &secrets, &input("token", true, Some(&long))).unwrap_err(),
            "secret-too-large"
        );
        assert_eq!(secrets.len(), 0);

        // The limit counts characters, as the localized message promises, so a multibyte
        // secret of the same length is refused too — and one that fits is accepted even
        // though it is twice as many bytes.
        let cyrillic = "т".repeat(MAX_SECRET_CHARS + 1);
        assert_eq!(
            create(&conn, &secrets, &input("token", true, Some(&cyrillic))).unwrap_err(),
            "secret-too-large"
        );
        let fits = "т".repeat(MAX_SECRET_CHARS);
        assert_eq!(fits.len(), MAX_SECRET_CHARS * 2);
        create(&conn, &secrets, &input("token", true, Some(&fits))).expect("fits");
    }

    #[test]
    fn the_limit_applies_to_every_write_into_the_secret_store() {
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        let long = "a".repeat(MAX_SECRET_CHARS + 1);

        let secure = create(&conn, &secrets, &input("token", true, Some("first"))).expect("secure");
        assert_eq!(
            update(&conn, &secrets, &secure.id, &input("token", true, Some(&long))).unwrap_err(),
            "secret-too-large"
        );
        assert_eq!(secrets.get(&secure.id).expect("secret").as_deref(), Some("first"));

        let plain = create(&conn, &secrets, &input("base", false, Some("x"))).expect("plain");
        assert_eq!(
            update(&conn, &secrets, &plain.id, &input("base", true, Some(&long))).unwrap_err(),
            "secret-too-large"
        );
        assert_eq!(secrets.len(), 1);
        assert_eq!(
            conn
                .query_row("SELECT value FROM global_vars WHERE id = ?1", params![plain.id], |row| row.get::<_, String>(0))
                .expect("stored"),
            "x"
        );
    }

    #[test]
    fn moving_a_long_secret_out_of_the_store_is_not_limited() {
        // SQLite has no size limit, so the keyring's cap must not block secure -> plain.
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        let long = "a".repeat(MAX_SECRET_CHARS + 1);
        let created = create(&conn, &secrets, &input("token", true, Some("short"))).expect("secure");
        secrets.set(&created.id, &long).expect("secret");

        let moved = update(&conn, &secrets, &created.id, &input("token", false, None)).expect("moved");
        assert!(!moved.secure);
        assert_eq!(moved.value.as_deref(), Some(long.as_str()));
        assert_eq!(secrets.len(), 0);
    }

    #[test]
    fn list_never_answers_a_secure_value_but_reveal_does() {
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        let created = create(&conn, &secrets, &input("token", true, Some("s3cret"))).expect("created");
        assert_eq!(list(&conn).expect("list")[0].value, None);
        assert_eq!(reveal(&conn, &secrets, &created.id).expect("reveal"), "s3cret");
    }

    #[test]
    fn list_is_ordered_by_name_ignoring_case() {
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        create(&conn, &secrets, &input("zeta", false, Some("1"))).expect("zeta");
        create(&conn, &secrets, &input("Alpha", false, Some("2"))).expect("alpha");
        create(&conn, &secrets, &input("beta", false, Some("3"))).expect("beta");
        let names: Vec<String> = list(&conn).expect("list").into_iter().map(|var| var.name).collect();
        assert_eq!(names, vec!["Alpha".to_string(), "beta".to_string(), "zeta".to_string()]);
    }

    #[test]
    fn load_values_reports_a_missing_secret_and_an_unknown_name() {
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        let created = create(&conn, &secrets, &input("token", true, Some("s3cret"))).expect("created");
        conn.execute("DELETE FROM global_vars WHERE id = ?1", params![created.id])
            .expect("delete");
        assert_eq!(
            load_values(&conn, &secrets, &["token".to_string()]).unwrap_err(),
            "unknown-var:token"
        );

        create(&conn, &secrets, &input("token", true, None)).expect("secure row without a secret");
        assert_eq!(
            load_values(&conn, &secrets, &["TOKEN".to_string()]).unwrap_err(),
            "secret-missing:token"
        );
    }

    #[test]
    fn load_values_masks_secure_values_for_the_dump() {
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        create(&conn, &secrets, &input("base", false, Some("https://a"))).expect("plain");
        create(&conn, &secrets, &input("token", true, Some("s3cret"))).expect("secure");
        let values = load_values(
            &conn,
            &secrets,
            &["base".to_string(), "token".to_string()],
        )
        .expect("values");
        assert_eq!(values.real.get("base").unwrap(), "https://a");
        assert_eq!(values.masked.get("base").unwrap(), "https://a");
        assert_eq!(values.real.get("token").unwrap(), "s3cret");
        assert_eq!(values.masked.get("token").unwrap(), SECRET_MASK);
    }

    #[test]
    fn a_name_mentioned_twice_is_read_once() {
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        create(&conn, &secrets, &input("token", true, Some("s3cret"))).expect("secure");
        let values = load_values(
            &conn,
            &secrets,
            &["token".to_string(), "TOKEN".to_string(), "Token".to_string()],
        )
        .expect("values");
        assert_eq!(values.real.len(), 1);
        assert_eq!(values.masked.len(), 1);
    }

    #[test]
    fn substitution_is_case_insensitive_and_single_pass() {
        let mut values = HashMap::new();
        values.insert("token".to_string(), "s3cret".to_string());
        values.insert("other".to_string(), "{{token}}".to_string());

        assert_eq!(substitute("{{ token }}", &values).unwrap(), "s3cret");
        assert_eq!(substitute("Bearer {{TOKEN}} and {{token}}", &values).unwrap(), "Bearer s3cret and s3cret");
        // A value is never rescanned, so `other` stays literal.
        assert_eq!(substitute("{{other}}", &values).unwrap(), "{{token}}");
    }

    #[test]
    fn a_bad_template_stays_literal() {
        let mut values = HashMap::new();
        values.insert("b".to_string(), "v".to_string());
        assert_eq!(substitute("{{ a b }}", &values).unwrap(), "{{ a b }}");
        assert_eq!(substitute("{{a{{b}}}}", &values).unwrap(), "{{av}}");
        assert_eq!(substitute("{{", &values).unwrap(), "{{");
        assert_eq!(substitute("{{token", &values).unwrap(), "{{token");
    }

    #[test]
    fn an_unknown_template_is_an_error() {
        let values = HashMap::new();
        assert_eq!(substitute("{{missing}}", &values).unwrap_err(), "unknown-var:missing");
    }

    #[test]
    fn names_are_collected_without_repeating() {
        assert_eq!(
            template_names("{{a}} {{ a }} {{b}} {{ a b }} {{1}}"),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn url_substitution_touches_only_query_values() {
        let mut values = HashMap::new();
        values.insert("q".to_string(), "hello world".to_string());
        let url = Url::parse("https://example.com/path?x=%7B%7Bq%7D%7D&y=1").expect("url");
        let replaced = substitute_url(&url, &values).expect("substituted");
        assert_eq!(replaced.query_pairs().count(), 2);
        assert_eq!(replaced.query_pairs().nth(0).unwrap().1, "hello world");
        assert_eq!(replaced.path(), "/path");
        // The rebuilt query uses the form encoding, so a space in a value becomes `+`.
        assert_eq!(replaced.to_string(), "https://example.com/path?x=hello+world&y=1");
    }

    #[test]
    fn an_url_without_a_template_is_returned_unchanged() {
        let url = Url::parse("https://example.com/path?a=b%20c").expect("url");
        assert_eq!(substitute_url(&url, &HashMap::new()).expect("url").to_string(), url.to_string());
    }

    #[test]
    fn a_template_in_the_path_or_a_parameter_name_is_rejected() {
        let values = HashMap::new();
        let url = Url::parse("https://example.com/{{segment}}?a=1").expect("url");
        assert_eq!(substitute_url(&url, &values).unwrap_err(), "unresolved-template:segment");
        let url = Url::parse("https://example.com/p?{{name}}=1").expect("url");
        assert_eq!(substitute_url(&url, &values).unwrap_err(), "unresolved-template:name");
    }

    #[test]
    fn a_template_in_the_host_is_rejected() {
        // `Url::parse` keeps a typed brace in the host literally and decodes an
        // already-encoded one, so both spellings reach the same check.
        let values = HashMap::new();
        let url = Url::parse("https://{{token}}/x").expect("url");
        assert_eq!(url.host_str(), Some("{{token}}"));
        assert_eq!(substitute_url(&url, &values).unwrap_err(), "unresolved-template:host");

        let url = Url::parse("https://%7b%7btoken%7d%7d/x").expect("url");
        assert_eq!(url.host_str(), Some("{{token}}"));
        assert_eq!(substitute_url(&url, &values).unwrap_err(), "unresolved-template:host");
    }

    #[test]
    fn a_substituted_value_does_not_re_spell_the_pairs_around_it() {
        // The rebuild only touches the pair it changed: a bare flag keeps no `=`, an
        // empty pair survives, and an existing escape stays as the user wrote it.
        let mut values = HashMap::new();
        values.insert("q".to_string(), "v".to_string());
        let url = Url::parse("https://example.com/p?debug&a=1&&keep=a%20b&x=%7B%7Bq%7D%7D&y=%5E")
            .expect("url");
        let replaced = substitute_url(&url, &values).expect("substituted");
        assert_eq!(
            replaced.query().unwrap(),
            "debug&a=1&&keep=a%20b&x=v&y=%5E"
        );
    }

    #[test]
    fn a_substituted_value_is_form_encoded() {
        let mut values = HashMap::new();
        values.insert("q".to_string(), "hello world/ä".to_string());
        let url = Url::parse("https://example.com/p?x=%7B%7Bq%7D%7D").expect("url");
        let replaced = substitute_url(&url, &values).expect("substituted");
        // The same rules `query_pairs_mut` used before: a space becomes `+`, everything
        // else outside the unreserved set is percent-escaped with uppercase hex.
        assert_eq!(replaced.query().unwrap(), "x=hello+world%2F%C3%A4");
        assert_eq!(replaced.query_pairs().next().unwrap().1, "hello world/ä");
    }

    #[test]
    fn a_path_template_is_detected_in_its_percent_encoded_form() {
        // `Url::parse` writes `{` and `}` as `%7B`/`%7D`, so the raw path never contains
        // the `{{` a template is built from.
        let values = HashMap::new();
        let url = Url::parse("https://example.com/%7b%7btoken%7d%7d").expect("url");
        assert_eq!(substitute_url(&url, &values).unwrap_err(), "unresolved-template:token");
    }

    #[test]
    fn decoded_braces_covers_every_spelling_the_encoder_picks() {
        // Whether the encoder wrote uppercase or lowercase escapes depends on how the user
        // typed the URL, so all four spellings have to become a brace.
        assert_eq!(decoded_braces("%7B%7b%7D%7d"), "{{}}");
        assert_eq!(decoded_braces("%7b%7B%7d%7D"), "{{}}");
        assert_eq!(decoded_braces("a%7B%7Btoken%7D%7Db"), "a{{token}}b");
    }

    #[test]
    fn updating_a_plain_variable_without_a_value_keeps_it() {
        // A rename arrives with `value: null`, which means "do not touch the value".
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        let created = create(&conn, &secrets, &input("base", false, Some("first"))).expect("created");
        let renamed = update(&conn, &secrets, &created.id, &input("baseUrl", false, None)).expect("renamed");
        assert_eq!(renamed.name, "baseUrl");
        assert_eq!(renamed.value.as_deref(), Some("first"));
    }

    #[test]
    fn updating_onto_a_name_another_row_owns_is_refused() {
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        create(&conn, &secrets, &input("Token", false, Some("a"))).expect("first");
        let second = create(&conn, &secrets, &input("other", false, Some("b"))).expect("second");
        // The name in the code is what the user typed, so the message names their row.
        assert_eq!(
            update(&conn, &secrets, &second.id, &input("TOKEN", false, None)).unwrap_err(),
            "duplicate-name:TOKEN"
        );
    }

    #[test]
    fn a_constraint_violation_is_reported_as_a_duplicate_name() {
        // The friendly check can be beaten by a concurrent write, so the unique index is
        // the authority — and its text must still reach `vars.nameDuplicate`.
        let error = rusqlite::Error::SqliteFailure(
            ffi::Error {
                code: ffi::ErrorCode::ConstraintViolation,
                extended_code: 19,
            },
            Some("UNIQUE constraint failed: global_vars.name".to_string()),
        );
        assert_eq!(duplicate_error("Token", error), "duplicate-name:Token");

        let other = rusqlite::Error::SqliteFailure(
            ffi::Error {
                code: ffi::ErrorCode::NotFound,
                extended_code: 12,
            },
            Some("no such table".to_string()),
        );
        assert_eq!(duplicate_error("Token", other), "no such table");
    }

    #[test]
    fn a_secure_variable_created_without_a_value_has_no_entry() {
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        let created = create(&conn, &secrets, &input("token", true, Some(""))).expect("created");
        assert_eq!(secrets.len(), 0);
        // Revealing an empty secret answers an empty field, which is what lets the user
        // type one into it; a send is where the missing secret stops the request.
        assert_eq!(reveal(&conn, &secrets, &created.id).expect("reveal"), "");
        assert_eq!(
            load_values(&conn, &secrets, &["token".to_string()]).unwrap_err(),
            "secret-missing:token"
        );
    }

    #[test]
    fn clearing_a_revealed_secret_erases_the_entry() {
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        let created = create(&conn, &secrets, &input("token", true, Some("s3cret"))).expect("created");
        let cleared = update(&conn, &secrets, &created.id, &input("token", true, Some(""))).expect("cleared");
        assert!(cleared.secure);
        assert_eq!(secrets.len(), 0);
        assert_eq!(reveal(&conn, &secrets, &created.id).expect("reveal"), "");
    }

    #[test]
    fn plain_to_secure_with_an_empty_value_writes_no_secret() {
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        let created = create(&conn, &secrets, &input("token", false, Some(""))).expect("created");
        let secured = update(&conn, &secrets, &created.id, &input("token", true, None)).expect("secured");
        assert!(secured.secure);
        assert_eq!(secrets.len(), 0);
    }

    #[test]
    fn unsecuring_a_row_whose_secret_is_gone_reports_the_missing_secret() {
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        let created = create(&conn, &secrets, &input("token", true, Some("s3cret"))).expect("created");
        secrets.delete(&created.id).expect("entry lost");
        assert_eq!(
            update(&conn, &secrets, &created.id, &input("token", false, None)).unwrap_err(),
            "secret-missing:token"
        );
        // The row stays secure rather than becoming a plain row with an empty value.
        assert!(list(&conn).expect("list")[0].secure);
    }

    #[test]
    fn reveal_answers_the_column_of_a_plain_row() {
        let conn = memory_db();
        let secrets = MemorySecrets::new();
        let created = create(&conn, &secrets, &input("base", false, Some("https://a"))).expect("created");
        assert_eq!(reveal(&conn, &secrets, &created.id).expect("reveal"), "https://a");
        assert_eq!(secrets.len(), 0);
    }
}
