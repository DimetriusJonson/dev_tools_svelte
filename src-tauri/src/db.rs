use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection, ErrorCode, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorState {
    pub tool_key: String,
    pub content: String,
    pub indent: i64,
    pub opt_format: bool,
    pub opt_unescape: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedUpload {
    pub id: String,
    pub filename: String,
    pub content_type: String,
    pub data: Vec<u8>,
    pub size: i64,
    pub created_at: String,
}

#[derive(Clone)]
pub struct AppDb {
    pub conn: Arc<Mutex<Connection>>,
}

pub fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
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

		CREATE INDEX IF NOT EXISTS idx_rest_requests_project ON rest_requests(project_id);
		CREATE INDEX IF NOT EXISTS idx_rest_properties_request ON rest_request_properties(request_id);
		CREATE INDEX IF NOT EXISTS idx_rest_form_request ON rest_request_form(request_id);
		CREATE INDEX IF NOT EXISTS idx_rest_multipart_request ON rest_request_multipart(request_id);
		CREATE INDEX IF NOT EXISTS idx_rest_files_request ON rest_request_files(request_id);

		INSERT INTO rest_request_form (request_id, name, value, sort_order)
		SELECT p.request_id, p.name, p.value, p.sort_order
		FROM rest_request_properties p
		WHERE p.kind = 'form'
			AND NOT EXISTS (
				SELECT 1 FROM rest_request_form f WHERE f.request_id = p.request_id
			);

		DELETE FROM rest_request_properties WHERE kind = 'form';
		"#,
    )
}

pub fn read_editor_state(
    conn: &Connection,
    tool_key: &str,
) -> rusqlite::Result<Option<EditorState>> {
    let mut statement = conn.prepare(
		"SELECT tool_key, content, indent, opt_format, opt_unescape FROM editor_state WHERE tool_key = ?1",
	)?;
    statement
        .query_row(params![tool_key], |row| {
            Ok(EditorState {
                tool_key: row.get(0)?,
                content: row.get(1)?,
                indent: row.get(2)?,
                opt_format: row.get::<_, i64>(3)? != 0,
                opt_unescape: row.get::<_, i64>(4)? != 0,
            })
        })
        .optional()
}

pub fn write_editor_state(conn: &Connection, state: &EditorState) -> rusqlite::Result<()> {
    conn.execute(
        r#"
		INSERT INTO editor_state (tool_key, content, indent, opt_format, opt_unescape, updated_at)
		VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP)
		ON CONFLICT(tool_key) DO UPDATE SET
			content = excluded.content,
			indent = excluded.indent,
			opt_format = excluded.opt_format,
			opt_unescape = excluded.opt_unescape,
			updated_at = CURRENT_TIMESTAMP
		"#,
        params![
            state.tool_key,
            state.content,
            state.indent,
            i64::from(state.opt_format as u8),
            i64::from(state.opt_unescape as u8),
        ],
    )?;
    Ok(())
}

pub fn read_setting(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    let mut statement = conn.prepare("SELECT value FROM app_settings WHERE key = ?1")?;
    statement
        .query_row(params![key], |row| row.get(0))
        .optional()
}

pub fn write_setting(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
		"INSERT INTO app_settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
		params![key, value],
	)?;
    Ok(())
}

pub fn insert_shared_upload(
    conn: &Connection,
    filename: &str,
    content_type: &str,
    data: &[u8],
) -> Result<String, String> {
    let size = data.len() as i64;
    for _ in 0..8 {
        let id = Uuid::new_v4().to_string();
        match conn.execute(
            "INSERT INTO shared_uploads (id, filename, content_type, data, size) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, filename, content_type, data, size],
        ) {
            Ok(_) => return Ok(id),
            Err(rusqlite::Error::SqliteFailure(error, _))
                if error.code == ErrorCode::ConstraintViolation => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("Could not allocate a unique upload id".to_string())
}

pub fn read_shared_upload(conn: &Connection, id: &str) -> rusqlite::Result<Option<SharedUpload>> {
    let mut statement = conn.prepare(
        "SELECT id, filename, content_type, data, size, created_at FROM shared_uploads WHERE id = ?1",
    )?;
    statement
        .query_row(params![id], |row| {
            Ok(SharedUpload {
                id: row.get(0)?,
                filename: row.get(1)?,
                content_type: row.get(2)?,
                data: row.get(3)?,
                size: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .optional()
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_editor_state(
    db: tauri::State<'_, AppDb>,
    tool_key: String,
) -> Result<Option<EditorState>, String> {
    let connection = db.conn.lock().map_err(|error| error.to_string())?;
    read_editor_state(&connection, &tool_key).map_err(|error| error.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn save_editor_state(
    db: tauri::State<'_, AppDb>,
    tool_key: String,
    content: String,
    indent: i64,
    opt_format: bool,
    opt_unescape: bool,
) -> Result<(), String> {
    let state = EditorState {
        tool_key,
        content,
        indent,
        opt_format,
        opt_unescape,
    };
    let connection = db.conn.lock().map_err(|error| error.to_string())?;
    write_editor_state(&connection, &state).map_err(|error| error.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_setting(db: tauri::State<'_, AppDb>, key: String) -> Result<Option<String>, String> {
    let connection = db.conn.lock().map_err(|error| error.to_string())?;
    read_setting(&connection, &key).map_err(|error| error.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn set_setting(db: tauri::State<'_, AppDb>, key: String, value: String) -> Result<(), String> {
    let connection = db.conn.lock().map_err(|error| error.to_string())?;
    write_setting(&connection, &key, &value).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_db() -> Connection {
        let connection = Connection::open_in_memory().expect("in-memory database");
        init_schema(&connection).expect("schema");
        connection
    }

    #[test]
    fn creates_expected_tables() {
        let connection = memory_db();
        let mut statement = connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .expect("prepare");
        let tables: Vec<String> = statement
            .query_map([], |row| row.get(0))
            .expect("query")
            .map(|row| row.expect("row"))
            .collect();
        assert!(tables.contains(&"editor_state".to_string()));
        assert!(tables.contains(&"app_settings".to_string()));
        assert!(tables.contains(&"shared_uploads".to_string()));
        assert!(tables.contains(&"rest_projects".to_string()));
        assert!(tables.contains(&"rest_requests".to_string()));
        assert!(tables.contains(&"rest_request_properties".to_string()));
        assert!(tables.contains(&"rest_request_form".to_string()));
        assert!(tables.contains(&"rest_request_multipart".to_string()));
        assert!(tables.contains(&"rest_request_files".to_string()));
    }

    #[test]
    fn migrates_legacy_form_rows_out_of_the_properties_table() {
        let connection = memory_db();
        let mut seed = connection.prepare(
            "INSERT INTO rest_request_properties (request_id, kind, name, value, sort_order) VALUES (?1, ?2, ?3, ?4, ?5)",
        ).expect("prepare seed");
        // An old build had stashed the Form-encode tab under kind = 'form'.
        seed.execute(params!["legacy", "form", "user", "a b", 0])
            .expect("seed form row");
        seed.execute(params!["legacy", "form", "page", "2", 1])
            .expect("seed form row");
        seed.execute(params!["legacy", "header", "Accept", "text/plain", 0])
            .expect("seed header row");
        seed.execute(params!["moved", "form", "stale", "1", 0])
            .expect("seed duplicate");

        let mut fresh = connection.prepare(
            "INSERT INTO rest_request_form (request_id, name, value, sort_order) VALUES (?1, ?2, ?3, ?4)",
        ).expect("prepare fresh");
        // A newer request already migrated, so its legacy twin must not be copied.
        fresh
            .execute(params!["moved", "kept", "1", 0])
            .expect("fresh form row");

        init_schema(&connection).expect("migrate");

        // No ORDER BY on the SELECT, so match the migrated pair by content.
        let migrated = form_row_values(&connection, "legacy");
        assert_eq!(migrated.len(), 2);
        assert!(migrated.contains(&("user".to_string(), "a b".to_string())));
        assert!(migrated.contains(&("page".to_string(), "2".to_string())));
        // The request that already had a row keeps it and drops the legacy copy.
        assert_eq!(
            form_row_values(&connection, "moved"),
            vec![("kept".to_string(), "1".to_string())]
        );
        // No kind = 'form' survives in the properties table at all.
        assert_eq!(count_property_rows(&connection, "legacy", "form"), 0);
        assert_eq!(count_property_rows(&connection, "moved", "form"), 0);
        // Headers are unrelated to the migration and stay put.
        assert_eq!(count_property_rows(&connection, "legacy", "header"), 1);

        // Migration runs on every startup, so a second pass must not change anything.
        init_schema(&connection).expect("migrate again");
        assert_eq!(form_row_values(&connection, "legacy").len(), 2);
        assert_eq!(form_row_values(&connection, "moved").len(), 1);
    }

    fn form_row_values(connection: &Connection, request_id: &str) -> Vec<(String, String)> {
        let mut statement = connection
            .prepare("SELECT name, value FROM rest_request_form WHERE request_id = ?1")
            .expect("prepare");
        statement
            .query_map(params![request_id], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("query")
            .map(|row| row.expect("row"))
            .collect()
    }

    fn count_property_rows(connection: &Connection, request_id: &str, kind: &str) -> i64 {
        connection
            .query_row(
                "SELECT COUNT(*) FROM rest_request_properties WHERE request_id = ?1 AND kind = ?2",
                params![request_id, kind],
                |row| row.get(0),
            )
            .expect("count")
    }

    #[test]
    fn shared_upload_roundtrip_and_uuid_id() {
        let connection = memory_db();
        let id =
            insert_shared_upload(&connection, "notes.txt", "text/plain", b"hello").expect("insert");
        let parsed = Uuid::parse_str(&id).expect("uuid");
        assert_eq!(parsed.get_version_num(), 4);

        let upload = read_shared_upload(&connection, &id)
            .expect("read")
            .expect("present");
        assert_eq!(upload.id, id);
        assert_eq!(upload.filename, "notes.txt");
        assert_eq!(upload.content_type, "text/plain");
        assert_eq!(upload.data, b"hello");
        assert_eq!(upload.size, 5);
        assert!(!upload.created_at.is_empty());
        assert_eq!(
            read_shared_upload(&connection, "missing").expect("read"),
            None
        );
    }

    #[test]
    fn editor_state_roundtrip_and_update() {
        let connection = memory_db();
        let state = EditorState {
            tool_key: "xml".to_string(),
            content: "<root/>".to_string(),
            indent: 3,
            opt_format: true,
            opt_unescape: false,
        };
        write_editor_state(&connection, &state).expect("insert");
        assert_eq!(
            read_editor_state(&connection, "xml").expect("read"),
            Some(state.clone())
        );

        let updated = EditorState {
            content: "<root><child/></root>".to_string(),
            indent: 4,
            opt_unescape: true,
            ..state
        };
        write_editor_state(&connection, &updated).expect("update");
        let stored = read_editor_state(&connection, "xml")
            .expect("read")
            .expect("present");
        assert_eq!(stored.content, "<root><child/></root>");
        assert_eq!(stored.indent, 4);
        assert!(stored.opt_format);
        assert!(stored.opt_unescape);
    }

    #[test]
    fn missing_editor_state_returns_none() {
        let connection = memory_db();
        assert_eq!(read_editor_state(&connection, "json").expect("read"), None);
    }

    #[test]
    fn settings_roundtrip() {
        let connection = memory_db();
        write_setting(&connection, "theme", "dark").expect("insert");
        assert_eq!(
            read_setting(&connection, "theme").expect("read"),
            Some("dark".to_string())
        );
        write_setting(&connection, "theme", "light").expect("update");
        assert_eq!(
            read_setting(&connection, "theme").expect("read"),
            Some("light".to_string())
        );
    }
}
