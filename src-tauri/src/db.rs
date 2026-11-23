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
    /// `None` means "keep forever"; only database rows carry an expiry.
    pub expires_at: Option<String>,
    /// `false` for uploads kept in RAM only, `true` for rows in `shared_uploads`.
    pub stored: bool,
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
			created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
			-- NULL keeps the row forever; a value is the UTC instant it expires at.
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
		-- WSDL client: projects, imported files, the parsed tree and per-operation state.
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

		INSERT INTO rest_request_form (request_id, name, value, sort_order)
		SELECT p.request_id, p.name, p.value, p.sort_order
		FROM rest_request_properties p
		WHERE p.kind = 'form'
			AND NOT EXISTS (
				SELECT 1 FROM rest_request_form f WHERE f.request_id = p.request_id
			);

		DELETE FROM rest_request_properties WHERE kind = 'form';
		"#,
    )?;
    // Databases created before the retention selector have no expiry column.
    ensure_column(conn, "shared_uploads", "expires_at", "DATETIME")
}

/// Adds a column to an existing table. `CREATE TABLE IF NOT EXISTS` never alters a
/// table that is already there, so every schema change needs a guarded `ALTER TABLE`.
fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    declaration: &str,
) -> rusqlite::Result<()> {
    let mut statement = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<_>>()?;
    if columns.iter().any(|name| name == column) {
        return Ok(());
    }
    conn.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {declaration}"),
        [],
    )?;
    Ok(())
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

/// Deletes every upload whose retention has run out. Runs before reads and writes, so
/// expired payloads leave the database even though nobody sweeps on a timer.
pub fn purge_expired_uploads(conn: &Connection) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM shared_uploads WHERE expires_at IS NOT NULL AND expires_at <= datetime('now')",
        [],
    )
}

/// The UTC instant a retention of whole `days` runs out, in the same text format
/// `CURRENT_TIMESTAMP` writes, so the purge can compare the two directly.
pub fn expiry_timestamp(conn: &Connection, days: u32) -> rusqlite::Result<String> {
    conn.query_row(
        "SELECT datetime('now', ?1)",
        params![format!("+{days} days")],
        |row| row.get(0),
    )
}

/// The UTC instant right now, for payloads that are never written to the database.
pub fn now_timestamp(conn: &Connection) -> rusqlite::Result<String> {
    conn.query_row("SELECT datetime('now')", [], |row| row.get(0))
}

pub fn insert_shared_upload(
    conn: &Connection,
    filename: &str,
    content_type: &str,
    data: &[u8],
    expires_at: Option<&str>,
) -> Result<String, String> {
    purge_expired_uploads(conn).map_err(|error| error.to_string())?;
    let size = data.len() as i64;
    for _ in 0..8 {
        let id = Uuid::new_v4().to_string();
        match conn.execute(
            "INSERT INTO shared_uploads (id, filename, content_type, data, size, expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, filename, content_type, data, size, expires_at],
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
        "SELECT id, filename, content_type, data, size, created_at, expires_at FROM shared_uploads WHERE id = ?1 AND (expires_at IS NULL OR expires_at > datetime('now'))",
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
                expires_at: row.get(6)?,
                stored: true,
            })
        })
        .optional()
}

/// Removes one upload. The share link is the only credential: whoever holds it may
/// delete the payload, so there is nothing to check beyond the id itself.
pub fn delete_shared_upload(conn: &Connection, id: &str) -> rusqlite::Result<bool> {
    Ok(conn.execute("DELETE FROM shared_uploads WHERE id = ?1", params![id])? > 0)
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
        assert!(tables.contains(&"wsdl_projects".to_string()));
        assert!(tables.contains(&"wsdl_files".to_string()));
        assert!(tables.contains(&"wsdl_services".to_string()));
        assert!(tables.contains(&"wsdl_operations".to_string()));
        assert!(tables.contains(&"wsdl_endpoints".to_string()));
        assert!(tables.contains(&"wsdl_operation_state".to_string()));
    }

    #[test]
    fn schema_is_idempotent_for_the_wsdl_tables() {
        let connection = memory_db();
        // A second startup must not re-create (and therefore must not clear) anything.
        init_schema(&connection).expect("schema again");
        let mut statement = connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .expect("prepare");
        let after: Vec<String> = statement
            .query_map([], |row| row.get(0))
            .expect("query")
            .map(|row| row.expect("row"))
            .collect();
        let wsdl_tables = after
            .iter()
            .filter(|name| name.starts_with("wsdl_"))
            .count();
        assert_eq!(wsdl_tables, 6);
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
        let id = insert_shared_upload(&connection, "notes.txt", "text/plain", b"hello", None)
            .expect("insert");
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
        assert_eq!(upload.expires_at, None);
        assert!(upload.stored);
        assert_eq!(
            read_shared_upload(&connection, "missing").expect("read"),
            None
        );
    }

    #[test]
    fn adds_the_expiry_column_to_an_older_database() {
        let connection = Connection::open_in_memory().expect("database");
        // The first release created this table without `expires_at`; the rows survive and
        // keep their payload.
        connection
            .execute_batch(
                "CREATE TABLE shared_uploads (id TEXT PRIMARY KEY, filename TEXT NOT NULL, content_type TEXT NOT NULL, data BLOB NOT NULL, size INTEGER NOT NULL, created_at DATETIME DEFAULT CURRENT_TIMESTAMP);
                 INSERT INTO shared_uploads (id, filename, content_type, data, size) VALUES ('old', 'a.txt', 'text/plain', CAST('x' AS BLOB), 1);",
            )
            .expect("legacy table");

        init_schema(&connection).expect("migrate");
        init_schema(&connection).expect("migrate again");

        let upload = read_shared_upload(&connection, "old")
            .expect("read")
            .expect("legacy row");
        assert_eq!(upload.expires_at, None);
        assert_eq!(upload.size, 1);
        assert_eq!(upload.data, b"x", "the payload survived the migration");
        assert!(upload.stored);
        assert!(delete_shared_upload(&connection, "old").expect("delete the legacy row"));
    }

    #[test]
    fn keeps_retention_and_purges_expired_rows() {
        let connection = memory_db();
        let living = insert_shared_upload(
            &connection,
            "keep.txt",
            "text/plain",
            b"keep",
            Some(&expiry_timestamp(&connection, 3).expect("expiry")),
        )
        .expect("insert");
        let dead = insert_shared_upload(
            &connection,
            "gone.txt",
            "text/plain",
            b"gone",
            Some("2001-01-01 00:00:00"),
        )
        .expect("insert");
        let forever = insert_shared_upload(&connection, "stay.txt", "text/plain", b"stay", None)
            .expect("insert");

        // The expired row is already gone: an insert purges before writing.
        assert_eq!(read_shared_upload(&connection, &dead).expect("read"), None);
        assert!(read_shared_upload(&connection, &living)
            .expect("read")
            .is_some());
        assert!(read_shared_upload(&connection, &forever)
            .expect("read")
            .is_some());
        assert_eq!(purge_expired_uploads(&connection).expect("purge"), 0);

        // Wiping the retention of a stored row makes it live forever.
        connection
            .execute(
                "UPDATE shared_uploads SET expires_at = NULL WHERE id = ?1",
                params![living],
            )
            .expect("clear expiry");
        assert_eq!(
            read_shared_upload(&connection, &living)
                .expect("read")
                .expect("present")
                .expires_at,
            None
        );
    }

    #[test]
    fn reading_expired_rows_does_not_delete_them_and_purge_removes_them() {
        let connection = memory_db();
        let id =
            insert_shared_upload(&connection, "a.txt", "text/plain", b"a", None).expect("insert");
        connection
            .execute(
                "UPDATE shared_uploads SET expires_at = '2001-01-01 00:00:00' WHERE id = ?1",
                params![id],
            )
            .expect("expire it");

        assert_eq!(
            read_shared_upload(&connection, &id).expect("read"),
            None,
            "an expired row must not answer a read even before the purge"
        );
        assert_eq!(purge_expired_uploads(&connection).expect("purge"), 1);
    }

    #[test]
    fn deletes_one_upload_and_reports_nothing_for_unknown_ids() {
        let connection = memory_db();
        let id =
            insert_shared_upload(&connection, "a.txt", "text/plain", b"a", None).expect("insert");
        assert!(delete_shared_upload(&connection, &id).expect("delete"));
        assert_eq!(read_shared_upload(&connection, &id).expect("read"), None);
        assert!(!delete_shared_upload(&connection, &id).expect("delete again"));
        assert!(!delete_shared_upload(&connection, "missing").expect("delete"));
    }

    #[test]
    fn deletes_a_row_the_read_already_hides() {
        let connection = memory_db();
        let id =
            insert_shared_upload(&connection, "a.txt", "text/plain", b"a", None).expect("insert");
        connection
            .execute(
                "UPDATE shared_uploads SET expires_at = '2001-01-01 00:00:00' WHERE id = ?1",
                params![id],
            )
            .expect("expire it");

        // A delete is not gated by the expiry filter: it wipes the payload the viewer can
        // no longer read, so the row leaves the database instead of waiting for a purge.
        assert!(delete_shared_upload(&connection, &id).expect("delete an expired row"));
        assert_eq!(purge_expired_uploads(&connection).expect("purge"), 0);
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
