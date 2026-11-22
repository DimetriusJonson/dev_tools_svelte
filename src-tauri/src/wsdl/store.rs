//! Persistence for imported WSDL files and the per-operation request state.
//!
//! Every function here takes the mutex-guarded connection through `AppDb` and
//! delegates the actual SQL to a `&Connection` helper, so the same code runs
//! against an in-memory database in tests.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::db::AppDb;
use crate::rest::{normalize_rest_url, RestProperty};
use crate::wsdl::parser::{fetch_wsdl, parse_wsdl, ParsedFile, XmlDom};
use crate::wsdl::xsd::{body_template, XsdIndex};
use crate::wsdl::{
    WsdlEndpoint, WsdlFetchState, WsdlFileRow, WsdlFileSummary, WsdlImportResult, WsdlOperation,
    WsdlOperationDetail, WsdlOperationInput, WsdlProject, WsdlService, WsdlState, SOAP_VERSION_11,
    SOAP_VERSION_12,
};

/// Turns a poisoned lock into the message the frontend shows.
fn lock_error<T>(error: std::sync::PoisonError<T>) -> String {
    error.to_string()
}

/// An id part is a WSDL name; XML names never contain `:`, so this separator
/// cannot be confused with a name and the parts stay readable.
const ID_SEPARATOR: &str = "::";

/// The service id of a file, deterministic in its name so a re-import of the
/// same URL produces the same ids and the saved operation state survives.
fn service_id(file_id: &str, name: &str) -> String {
    format!("{file_id}{ID_SEPARATOR}{name}")
}

/// The operation id of a service, deterministic for the same reason.
fn operation_id(service: &str, name: &str) -> String {
    format!("{service}{ID_SEPARATOR}{name}")
}

/// Header rows are stored as JSON, like the REST response headers.
fn encode_headers(rows: &[RestProperty]) -> Result<String, String> {
    serde_json::to_string(rows).map_err(|error| error.to_string())
}

fn decode_headers(text: &str) -> Vec<RestProperty> {
    serde_json::from_str(text).unwrap_or_default()
}

/// A `SoapAction` value is quoted exactly once, never doubled, and an empty
/// action stays empty. Mirrors `utils/wsdl.ts::soapActionForCall`.
fn soap_action_for_call(action: &str) -> String {
    if action.is_empty() || action.starts_with('"') {
        return action.to_string();
    }
    format!("\"{action}\"")
}

/// The rows a binding mandates: 1.1 pins `Content-Type` and `SOAPAction`,
/// 1.2 pins one `Content-Type` carrying the action as an attribute. Mirrors
/// `utils/wsdl.ts::lockedHeadersFor`.
fn locked_headers(version: &str, action: &str) -> Vec<RestProperty> {
    let quoted = soap_action_for_call(action);

    if version == SOAP_VERSION_12 {
        let value = if action.is_empty() {
            "application/soap+xml; charset=utf-8".to_string()
        } else {
            format!("application/soap+xml; charset=utf-8; action={quoted}")
        };
        return vec![RestProperty {
            name: "Content-Type".to_string(),
            value,
        }];
    }

    vec![
        RestProperty {
            name: "Content-Type".to_string(),
            value: "text/xml; charset=utf-8".to_string(),
        },
        RestProperty {
            name: "SOAPAction".to_string(),
            value: quoted,
        },
    ]
}

/// Names of the mandatory rows, lowercased for the case-insensitive clash check.
fn locked_header_names(version: &str, action: &str) -> Vec<String> {
    locked_headers(version, action)
        .iter()
        .map(|row| row.name.to_lowercase())
        .collect()
}

/// The rows worth persisting: a blank name never reaches the wire, and a row
/// repeating a mandatory name is dropped rather than duplicated, because the
/// locked rows are recomputed from the file on every load. Mirrors
/// `utils/wsdl.ts::editableForSave`.
fn editable_for_save(version: &str, action: &str, rows: &[RestProperty]) -> Vec<RestProperty> {
    let names = locked_header_names(version, action);
    rows.iter()
        .filter(|row| {
            !row.name.trim().is_empty() && !names.contains(&row.name.trim().to_lowercase())
        })
        .map(|row| RestProperty {
            name: row.name.trim().to_string(),
            value: row.value.clone(),
        })
        .collect()
}

/// Mandatory rows first, then the user rows in the order they were typed.
/// Mirrors `utils/wsdl.ts::mergeLockedHeaders`.
fn merge_locked_headers(mandatory: &[RestProperty], user: &[RestProperty]) -> Vec<RestProperty> {
    let mut rows: Vec<RestProperty> = mandatory.to_vec();
    rows.extend(
        user.iter()
            .filter(|row| !row.name.trim().is_empty())
            .map(|row| RestProperty {
                name: row.name.trim().to_string(),
                value: row.value.clone(),
            }),
    );
    rows
}

pub fn list_projects(db: tauri::State<'_, AppDb>) -> Result<Vec<WsdlProject>, String> {
    let connection = db.conn.lock().map_err(lock_error)?;
    select_projects(&connection)
}

pub fn create_project(db: tauri::State<'_, AppDb>, name: String) -> Result<WsdlProject, String> {
    let connection = db.conn.lock().map_err(lock_error)?;
    insert_project(&connection, name.trim())
}

pub fn rename_project(db: tauri::State<'_, AppDb>, id: String, name: String) -> Result<(), String> {
    let connection = db.conn.lock().map_err(lock_error)?;
    rename_project_row(&connection, &id, name.trim())
}

pub fn delete_project(db: tauri::State<'_, AppDb>, id: String) -> Result<(), String> {
    let connection = db.conn.lock().map_err(lock_error)?;
    remove_project(&connection, &id)
}

pub fn list_files(db: tauri::State<'_, AppDb>) -> Result<Vec<WsdlFileSummary>, String> {
    let connection = db.conn.lock().map_err(lock_error)?;
    select_files(&connection)
}

/// Fetches a WSDL and writes it, its services, operations and endpoints.
///
/// The network work happens before the connection is locked, so a slow server
/// does not block every other command. Nothing is written when the fetch or the
/// parse fails: a broken document leaves the previously imported file intact.
pub async fn import_file(
    db: tauri::State<'_, AppDb>,
    cancel: tauri::State<'_, WsdlFetchState>,
    project_id: String,
    url: String,
) -> Result<WsdlImportResult, String> {
    let waiter = cancel.begin();
    let queued = cancel.take_pending();

    let endpoint = match normalize_rest_url(&url) {
        Ok(endpoint) => endpoint,
        Err(error) => {
            cancel.clear();
            return Err(format!("not-url: {error}"));
        }
    };

    let source = match fetch_wsdl(queued, waiter, endpoint.as_str()).await {
        Ok(source) => source,
        Err(error) => {
            cancel.clear();
            return Err(error);
        }
    };
    cancel.clear();

    let parsed = parse_wsdl(&source)?;

    let mut connection = db.conn.lock().map_err(lock_error)?;
    persist_file(&mut connection, &project_id, &endpoint, &source, &parsed)
}

pub fn delete_file(db: tauri::State<'_, AppDb>, id: String) -> Result<(), String> {
    let connection = db.conn.lock().map_err(lock_error)?;
    remove_file(&connection, &id)
}

/// Reads one operation with its endpoints and its saved state, creating the
/// state row on first open and re-applying the headers the binding mandates.
pub fn get_operation(
    db: tauri::State<'_, AppDb>,
    operation_id: String,
) -> Result<WsdlOperationDetail, String> {
    let connection = db.conn.lock().map_err(lock_error)?;
    select_operation_detail(&connection, &operation_id)
}

/// Stores what the user typed for one operation: the selected endpoint, the
/// body, the editable headers and the save flag. The response columns keep
/// whatever they already held, and headers that clash with the binding's
/// mandatory rows are dropped here rather than sent twice.
pub fn save_operation_state(
    db: tauri::State<'_, AppDb>,
    input: WsdlOperationInput,
) -> Result<(), String> {
    let connection = db.conn.lock().map_err(lock_error)?;
    persist_state(&connection, &input)
}

/// Keeps the answer of a finished call so the operation shows it again later.
pub fn save_response(
    db: tauri::State<'_, AppDb>,
    operation_id: String,
    body: String,
    headers: Vec<RestProperty>,
    raw_request: String,
) -> Result<(), String> {
    let connection = db.conn.lock().map_err(lock_error)?;
    store_response(&connection, &operation_id, &body, &headers, &raw_request)
}

/// Empties the stored answer without touching the request the user wrote.
pub fn clear_response(db: tauri::State<'_, AppDb>, operation_id: String) -> Result<(), String> {
    let connection = db.conn.lock().map_err(lock_error)?;
    drop_response(&connection, &operation_id)
}

/// Where the next row of a table sits in the order the explorer shows it.
fn next_sort_order(conn: &Connection, table: &str) -> rusqlite::Result<i64> {
    let sql = format!("SELECT COALESCE(MAX(sort_order), -1) + 1 FROM {table}");
    conn.query_row(&sql, [], |row| row.get(0))
}

fn select_projects(conn: &Connection) -> Result<Vec<WsdlProject>, String> {
    let mut statement = conn
        .prepare(
            "SELECT id, name, created_at FROM wsdl_projects ORDER BY sort_order, created_at, id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(WsdlProject {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn select_project(conn: &Connection, id: &str) -> Option<WsdlProject> {
    conn.query_row(
        "SELECT id, name, created_at FROM wsdl_projects WHERE id = ?1",
        params![id],
        |row| {
            Ok(WsdlProject {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
            })
        },
    )
    .optional()
    .ok()
    .flatten()
}

fn insert_project(conn: &Connection, name: &str) -> Result<WsdlProject, String> {
    if name.is_empty() {
        return Err("name-required: a project needs a name".to_string());
    }
    let id = Uuid::new_v4().to_string();
    let sort_order = next_sort_order(conn, "wsdl_projects").map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT INTO wsdl_projects (id, name, sort_order) VALUES (?1, ?2, ?3)",
        params![id, name, sort_order],
    )
    .map_err(|error| error.to_string())?;
    select_project(conn, &id).ok_or_else(|| "not-found: the project was not stored".to_string())
}

fn rename_project_row(conn: &Connection, id: &str, name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("name-required: a project needs a name".to_string());
    }
    conn.execute(
        "UPDATE wsdl_projects SET name = ?2 WHERE id = ?1",
        params![id, name],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// Deletes a project together with everything it owns. The schema declares no
/// foreign keys, so the cascade is written out here.
fn remove_project(conn: &Connection, id: &str) -> Result<(), String> {
    let mut statement = conn
        .prepare("SELECT id FROM wsdl_files WHERE project_id = ?1")
        .map_err(|error| error.to_string())?;
    let files = statement
        .query_map(params![id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;

    for file in files {
        remove_file(conn, &file)?;
    }
    conn.execute("DELETE FROM wsdl_projects WHERE id = ?1", params![id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// The `wsdl_files` rows without the document text the explorer never shows.
fn select_file_rows(conn: &Connection) -> Result<Vec<WsdlFileRow>, String> {
    let mut statement = conn
        .prepare(
            "SELECT id, COALESCE(project_id, ''), name, source_url, parse_warnings, imported_at
             FROM wsdl_files ORDER BY sort_order, imported_at, id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            let warnings: String = row.get(4)?;
            Ok(WsdlFileRow {
                id: row.get(0)?,
                project_id: row.get(1)?,
                name: row.get(2)?,
                source_url: row.get(3)?,
                warnings: serde_json::from_str(&warnings).unwrap_or_default(),
                imported_at: row.get(5)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn select_files(conn: &Connection) -> Result<Vec<WsdlFileSummary>, String> {
    let mut summaries = Vec::new();
    for row in select_file_rows(conn)? {
        let services = select_tree(conn, &row.id)?;
        summaries.push(WsdlFileSummary {
            id: row.id,
            project_id: row.project_id,
            name: row.name,
            source_url: row.source_url,
            warnings: row.warnings,
            services,
            imported_at: row.imported_at,
        });
    }
    Ok(summaries)
}

/// The service/operation tree of one file. An operation reports the SOAP
/// version of its first endpoint because every endpoint of one operation is
/// merged into that single row.
fn select_tree(conn: &Connection, file_id: &str) -> Result<Vec<WsdlService>, String> {
    let services = select_services(conn, file_id)?;

    let mut tree = Vec::with_capacity(services.len());
    for (id, name) in services {
        let operations = select_operations(conn, &id)?;

        let mut entries = Vec::with_capacity(operations.len());
        for (operation_id, name, soap_action) in operations {
            let endpoints = select_endpoints(conn, &operation_id)?;
            entries.push(WsdlOperation {
                soap_version: endpoints
                    .first()
                    .map(|endpoint| endpoint.soap_version.clone())
                    .unwrap_or_else(|| SOAP_VERSION_11.to_string()),
                endpoint_count: endpoints.len(),
                id: operation_id,
                name,
                soap_action,
            });
        }

        tree.push(WsdlService {
            id,
            name,
            operations: entries,
        });
    }
    Ok(tree)
}

fn select_services(conn: &Connection, file_id: &str) -> Result<Vec<(String, String)>, String> {
    let mut statement = conn
        .prepare(
            "SELECT id, name FROM wsdl_services WHERE file_id = ?1 ORDER BY sort_order, name, id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![file_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn select_operations(
    conn: &Connection,
    service_id: &str,
) -> Result<Vec<(String, String, String)>, String> {
    let mut statement = conn
        .prepare(
            "SELECT id, name, soap_action FROM wsdl_operations WHERE service_id = ?1 ORDER BY sort_order, name, id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![service_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}

fn select_endpoints(conn: &Connection, operation_id: &str) -> Result<Vec<WsdlEndpoint>, String> {
    let mut statement = conn
        .prepare(
            "SELECT id, url, soap_version, binding, port_name FROM wsdl_endpoints WHERE operation_id = ?1 ORDER BY sort_order, id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![operation_id], |row| {
            Ok(WsdlEndpoint {
                id: row.get(0)?,
                url: row.get(1)?,
                soap_version: row.get(2)?,
                soap_action: String::new(),
                port_name: row.get(4)?,
                binding_name: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(rows)
}
/// Deletes a file and everything it owns. The schema declares no foreign keys,
/// so the cascade is written out here; the saved states go with the file because
/// their operation ids can no longer be resolved.
fn remove_file(conn: &Connection, id: &str) -> Result<(), String> {
    let tree = "operation_id IN (SELECT o.id FROM wsdl_operations o JOIN wsdl_services s ON s.id = o.service_id WHERE s.file_id = ?1)";
    // The states go first: every statement below finds its rows through the tree,
    // so the tree has to still exist while the states are looked up.
    for sql in [
        &format!("DELETE FROM wsdl_operation_state WHERE {tree}"),
        &format!("DELETE FROM wsdl_endpoints WHERE {tree}"),
        "DELETE FROM wsdl_operations WHERE service_id IN (SELECT id FROM wsdl_services WHERE file_id = ?1)",
        "DELETE FROM wsdl_services WHERE file_id = ?1",
        "DELETE FROM wsdl_files WHERE id = ?1",
    ] {
        conn.execute(sql, params![id]).map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Deletes the tree of a file but keeps its `wsdl_files` row and its saved
/// operation states, which is what a re-import needs.
fn remove_tree(conn: &Connection, file_id: &str) -> Result<(), String> {
    for sql in [
        "DELETE FROM wsdl_endpoints WHERE operation_id IN (SELECT o.id FROM wsdl_operations o JOIN wsdl_services s ON s.id = o.service_id WHERE s.file_id = ?1)",
        "DELETE FROM wsdl_operations WHERE service_id IN (SELECT id FROM wsdl_services WHERE file_id = ?1)",
        "DELETE FROM wsdl_services WHERE file_id = ?1",
    ] {
        conn.execute(sql, params![file_id])
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// The name the explorer shows for a file: the last path segment of the URL,
/// falling back to the host when the URL ends with a slash.
fn display_name(url: &reqwest::Url) -> String {
    let last = url
        .path_segments()
        .and_then(|segments| segments.last())
        .unwrap_or_default()
        .trim();
    if last.is_empty() {
        return url.host_str().unwrap_or(url.as_str()).to_string();
    }
    last.to_string()
}

/// Reads the file row of a URL already imported into the same project. An empty
/// project id is stored as NULL, and `IS` is what compares NULLs as equal.
fn find_file(
    conn: &Connection,
    project_id: &str,
    url: &str,
) -> Result<Option<WsdlFileRow>, String> {
    let stored = optional_project(project_id);
    conn.query_row(
        "SELECT id, COALESCE(project_id, ''), name, source_url, parse_warnings, imported_at FROM wsdl_files WHERE project_id IS ?1 AND source_url = ?2",
        params![stored, url],
        |row| {
            let warnings: String = row.get(4)?;
            Ok(WsdlFileRow {
                id: row.get(0)?,
                project_id: row.get(1)?,
                name: row.get(2)?,
                source_url: row.get(3)?,
                warnings: serde_json::from_str(&warnings).unwrap_or_default(),
                imported_at: row.get(5)?,
            })
        },
    )
    .optional()
    .map_err(|error| error.to_string())
}

/// "" means "no project", which the schema keeps as NULL.
fn optional_project(project_id: &str) -> Option<&str> {
    if project_id.is_empty() {
        None
    } else {
        Some(project_id)
    }
}

/// The endpoint ids already in use for this file, keyed by the operation and the
/// address they point at, so a re-import of the same document keeps them and the
/// state that names one of them stays valid.
fn kept_endpoint_ids(
    conn: &Connection,
    file_id: &str,
) -> Result<HashMap<(String, String, String), String>, String> {
    let mut statement = conn
        .prepare(
            "SELECT o.id, e.url, e.soap_version, e.id FROM wsdl_endpoints e JOIN wsdl_operations o ON o.id = e.operation_id JOIN wsdl_services s ON s.id = o.service_id WHERE s.file_id = ?1 ORDER BY e.sort_order, e.id",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![file_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;

    let mut kept = HashMap::new();
    for (operation, address, version, id) in rows {
        kept.entry((operation, address, version)).or_insert(id);
    }
    Ok(kept)
}

/// Reports a warning once: the same code found in two parts is one line in the UI.
fn push_warning(warnings: &mut Vec<String>, text: &str) {
    if !text.is_empty() && !warnings.iter().any(|known| known == text) {
        warnings.push(text.to_string());
    }
}

/// The request skeleton of an operation's input part. A part whose element cannot
/// be resolved produces an empty template plus a warning rather than a failed
/// import, so the operation stays openable and editable.
fn template_for(
    dom: &XmlDom,
    index: &XsdIndex,
    reference: Option<&(String, String)>,
) -> (String, Vec<String>) {
    let Some((uri, local)) = reference else {
        return (String::new(), Vec::new());
    };

    let fragment = body_template(dom, index, dom.root(), uri, local);
    (fragment.xml, fragment.warnings)
}

/// Reads back one file with its tree, the shape every command returns.
fn summary_of(conn: &Connection, id: &str) -> Result<WsdlFileSummary, String> {
    let row = conn
        .query_row(
            "SELECT id, COALESCE(project_id, ''), name, source_url, parse_warnings, imported_at FROM wsdl_files WHERE id = ?1",
            params![id],
            |row| {
                let warnings: String = row.get(4)?;
                Ok(WsdlFileRow {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    name: row.get(2)?,
                    source_url: row.get(3)?,
                    warnings: serde_json::from_str(&warnings).unwrap_or_default(),
                    imported_at: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("not-found: no WSDL file with id {id}"))?;

    Ok(WsdlFileSummary {
        id: row.id.clone(),
        project_id: row.project_id,
        name: row.name,
        source_url: row.source_url,
        warnings: row.warnings,
        services: select_tree(conn, &row.id)?,
        imported_at: row.imported_at,
    })
}

/// Inserts the row of a new import, or refreshes the row of an already imported
/// URL in place so its id — and every operation id derived from it — survives.
#[allow(clippy::too_many_arguments)]
fn write_file_row(
    conn: &Connection,
    file_id: &str,
    project_id: &str,
    endpoint: &reqwest::Url,
    source: &str,
    warnings: &[String],
    existing: Option<&WsdlFileRow>,
) -> Result<(), String> {
    let name = display_name(endpoint);
    let warnings = serde_json::to_string(warnings).map_err(|error| error.to_string())?;

    if existing.is_some() {
        conn.execute(
            "UPDATE wsdl_files SET name = ?2, raw_xml = ?3, parse_warnings = ?4, imported_at = CURRENT_TIMESTAMP WHERE id = ?1",
            params![file_id, name, source, warnings],
        )
        .map_err(|error| error.to_string())?;
        return Ok(());
    }

    let sort_order = next_sort_order(conn, "wsdl_files").map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT INTO wsdl_files (id, project_id, name, source_url, raw_xml, parse_warnings, sort_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            file_id,
            optional_project(project_id),
            name,
            endpoint.as_str(),
            source,
            warnings,
            sort_order
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// Writes the service/operation/endpoint tree with the deterministic ids derived
/// from the names, reusing an endpoint id whenever the same operation still
/// points at the same address with the same SOAP version.
fn write_tree(
    conn: &Connection,
    file_id: &str,
    parsed: &ParsedFile,
    kept: &HashMap<(String, String, String), String>,
    index: &XsdIndex,
) -> Result<Vec<String>, String> {
    let mut warnings: Vec<String> = Vec::new();

    for (position, service) in parsed.services.iter().enumerate() {
        let sid = service_id(file_id, &service.name);
        conn.execute(
            "INSERT INTO wsdl_services (id, file_id, name, sort_order) VALUES (?1, ?2, ?3, ?4)",
            params![sid, file_id, service.name, position as i64],
        )
        .map_err(|error| error.to_string())?;

        for (position, operation) in service.operations.iter().enumerate() {
            let operation_id = operation_id(&sid, &operation.name);
            let (template, found) = template_for(&parsed.dom, index, operation.input_ref.as_ref());
            for warning in found {
                push_warning(&mut warnings, &warning);
            }

            conn.execute(
                "INSERT INTO wsdl_operations (id, service_id, name, soap_action, input_element, output_element, input_template, sort_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    operation_id,
                    sid,
                    operation.name,
                    operation.soap_action,
                    operation.input_element,
                    operation.output_element,
                    template,
                    position as i64
                ],
            )
            .map_err(|error| error.to_string())?;

            let mut seen: Vec<(String, String)> = Vec::new();
            let mut order: i64 = 0;
            for endpoint in &operation.endpoints {
                let address = (endpoint.url.clone(), endpoint.soap_version.clone());
                if seen.iter().any(|known| *known == address) {
                    continue;
                }
                seen.push(address.clone());

                let id = kept
                    .get(&(operation_id.clone(), address.0.clone(), address.1.clone()))
                    .cloned()
                    .unwrap_or_else(|| Uuid::new_v4().to_string());

                conn.execute(
                    "INSERT INTO wsdl_endpoints (id, operation_id, url, soap_version, binding, port_name, sort_order) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        id,
                        operation_id,
                        address.0,
                        address.1,
                        endpoint.binding_name,
                        endpoint.port_name,
                        order
                    ],
                )
                .map_err(|error| error.to_string())?;
                order += 1;
            }
        }
    }

    Ok(warnings)
}

/// Stores a parsed document in one transaction. A file whose URL was imported
/// into the same project before keeps its id, so the operation ids derived from
/// it — and the request state stored under them — survive a re-import.
fn persist_file(
    conn: &mut Connection,
    project_id: &str,
    endpoint: &reqwest::Url,
    source: &str,
    parsed: &ParsedFile,
) -> Result<WsdlImportResult, String> {
    let transaction = conn.transaction().map_err(|error| error.to_string())?;

    let existing = find_file(&transaction, project_id, endpoint.as_str())?;
    let reimported = existing.is_some();
    let file_id = existing
        .as_ref()
        .map(|row| row.id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let kept = kept_endpoint_ids(&transaction, &file_id)?;
    remove_tree(&transaction, &file_id)?;

    let index = XsdIndex::build(&parsed.dom);
    let extra = write_tree(&transaction, &file_id, parsed, &kept, &index)?;

    let mut warnings = parsed.warnings.clone();
    for text in extra {
        push_warning(&mut warnings, &text);
    }

    write_file_row(
        &transaction,
        &file_id,
        project_id,
        endpoint,
        source,
        &warnings,
        existing.as_ref(),
    )?;
    transaction.commit().map_err(|error| error.to_string())?;

    Ok(WsdlImportResult {
        summary: summary_of(conn, &file_id)?,
        reimported,
        warnings,
    })
}

/// The two columns the request needs from an operation row, or the message the
/// frontend shows when the operation is no longer in the database.
fn operation_keys(conn: &Connection, operation_id: &str) -> Result<(String, String), String> {
    conn.query_row(
        "SELECT soap_action, input_template FROM wsdl_operations WHERE id = ?1",
        params![operation_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )
    .optional()
    .map_err(|error| error.to_string())?
    .ok_or_else(|| format!("not-found: no WSDL operation with id {operation_id}"))
}

/// The SOAP version a call runs under: the version of the endpoint the state has
/// selected, else of the first endpoint, else the WSDL default.
fn version_of(conn: &Connection, operation_id: &str, endpoint_id: &str) -> Result<String, String> {
    if !endpoint_id.is_empty() {
        if let Some(version) = conn
            .query_row(
                "SELECT soap_version FROM wsdl_endpoints WHERE id = ?1",
                params![endpoint_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
        {
            return Ok(version);
        }
    }

    Ok(conn
        .query_row(
            "SELECT soap_version FROM wsdl_endpoints WHERE operation_id = ?1 ORDER BY sort_order, id",
            params![operation_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| SOAP_VERSION_11.to_string()))
}

fn endpoint_ids(conn: &Connection, operation_id: &str) -> Result<Vec<String>, String> {
    Ok(select_endpoints(conn, operation_id)?
        .into_iter()
        .map(|endpoint| endpoint.id)
        .collect())
}

fn read_state(conn: &Connection, operation_id: &str) -> Result<Option<WsdlState>, String> {
    conn.query_row(
        "SELECT COALESCE(endpoint_id, ''), request_body, headers, save_response, COALESCE(response_body, ''), response_headers, COALESCE(raw_request, '') FROM wsdl_operation_state WHERE operation_id = ?1",
        params![operation_id],
        |row| {
            let headers: String = row.get(2)?;
            let response_headers: String = row.get(5)?;
            Ok(WsdlState {
                endpoint_id: row.get(0)?,
                request_body: row.get(1)?,
                headers: decode_headers(&headers),
                save_response: row.get(3)?,
                response_body: row.get(4)?,
                response_headers: decode_headers(&response_headers),
                raw_request: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(|error| error.to_string())
}

/// The state row of an operation, written when it is missing: an operation is
/// opened with the generated template, the first endpoint and the headers the
/// binding mandates.
fn write_state(conn: &Connection, operation_id: &str, state: &WsdlState) -> Result<(), String> {
    let headers = encode_headers(&state.headers)?;
    let response_headers = encode_headers(&state.response_headers)?;

    conn.execute(
        "INSERT INTO wsdl_operation_state (operation_id, endpoint_id, request_body, headers, save_response, response_body, response_headers, raw_request, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, CURRENT_TIMESTAMP) ON CONFLICT(operation_id) DO UPDATE SET endpoint_id = excluded.endpoint_id, request_body = excluded.request_body, headers = excluded.headers, save_response = excluded.save_response, response_body = excluded.response_body, response_headers = excluded.response_headers, raw_request = excluded.raw_request, updated_at = CURRENT_TIMESTAMP",
        params![
            operation_id,
            state.endpoint_id,
            state.request_body,
            headers,
            state.save_response,
            state.response_body,
            response_headers,
            state.raw_request
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

/// A state row shape for an operation that has never been opened.
fn empty_state() -> WsdlState {
    WsdlState {
        endpoint_id: String::new(),
        request_body: String::new(),
        headers: Vec::new(),
        save_response: false,
        response_body: String::new(),
        response_headers: Vec::new(),
        raw_request: String::new(),
    }
}

/// Reads an operation with its endpoints and its state. The first open creates
/// the state row, and the mandatory headers are always recomputed from the
/// operation's action and the selected endpoint's version.
fn select_operation_detail(
    conn: &Connection,
    operation_id: &str,
) -> Result<WsdlOperationDetail, String> {
    let row = conn
        .query_row(
            "SELECT o.name, o.service_id, s.name, o.soap_action, o.input_element, o.output_element, o.input_template FROM wsdl_operations o JOIN wsdl_services s ON s.id = o.service_id WHERE o.id = ?1",
            params![operation_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("not-found: no WSDL operation with id {operation_id}"))?;

    let (
        name,
        service_id,
        service_name,
        soap_action,
        input_element,
        output_element,
        input_template,
    ) = row;

    let mut endpoints = select_endpoints(conn, operation_id)?;
    let first = endpoints
        .first()
        .map(|endpoint| endpoint.id.clone())
        .unwrap_or_default();

    let mut state = match read_state(conn, operation_id)? {
        Some(state) => state,
        // The first open of an operation writes the generated template and the
        // first endpoint; the headers are rebuilt below from the real version.
        None => WsdlState {
            endpoint_id: first.clone(),
            request_body: input_template.clone(),
            ..empty_state()
        },
    };

    // An endpoint id the operation does not own — one a re-import replaced — is
    // not remembered: the selection falls back to the first endpoint, and the
    // version the headers are built from follows the value written here.
    if !endpoints
        .iter()
        .any(|endpoint| endpoint.id == state.endpoint_id)
    {
        state.endpoint_id = first;
    }

    let version = version_of(conn, operation_id, &state.endpoint_id)?;
    let locked = locked_headers(&version, &soap_action);

    state.headers = merge_locked_headers(
        &locked,
        &editable_for_save(&version, &soap_action, &state.headers),
    );

    // Reading an operation is also what canonicalizes its row, so a row written
    // by an older build or by a re-import ends up with the headers the file
    // mandates and a selection that exists.
    write_state(conn, operation_id, &state)?;

    for endpoint in endpoints.iter_mut() {
        endpoint.soap_action = soap_action.clone();
    }

    Ok(WsdlOperationDetail {
        id: operation_id.to_string(),
        name,
        service_id,
        service_name,
        soap_version: version,
        soap_action,
        input_element,
        output_element,
        input_template,
        endpoints,
        state,
    })
}

/// Stores what the user typed, keeping the stored answer untouched. A selected
/// endpoint that is not part of the operation is cleared rather than remembered,
/// because a re-import can replace it.
fn persist_state(conn: &Connection, input: &WsdlOperationInput) -> Result<(), String> {
    let (soap_action, _) = operation_keys(conn, &input.operation_id)?;

    let ids = endpoint_ids(conn, &input.operation_id)?;
    let endpoint_id = if ids.contains(&input.endpoint_id) {
        input.endpoint_id.clone()
    } else {
        String::new()
    };

    let version = version_of(conn, &input.operation_id, &endpoint_id)?;
    let locked = locked_headers(&version, &soap_action);
    let stored = read_state(conn, &input.operation_id)?.unwrap_or_else(empty_state);

    let state = WsdlState {
        endpoint_id,
        request_body: input.request_body.clone(),
        headers: merge_locked_headers(
            &locked,
            &editable_for_save(&version, &soap_action, &input.headers),
        ),
        save_response: input.save_response,
        ..stored
    };

    write_state(conn, &input.operation_id, &state)
}

/// Keeps the answer of a finished call so the operation shows it again later.
fn store_response(
    conn: &Connection,
    operation_id: &str,
    body: &str,
    headers: &[RestProperty],
    raw_request: &str,
) -> Result<(), String> {
    operation_keys(conn, operation_id)?;
    let stored = read_state(conn, operation_id)?.unwrap_or_else(empty_state);

    let state = WsdlState {
        response_body: body.to_string(),
        response_headers: headers.to_vec(),
        raw_request: raw_request.to_string(),
        ..stored
    };
    write_state(conn, operation_id, &state)
}

/// Empties the stored answer without touching the request the user wrote.
fn drop_response(conn: &Connection, operation_id: &str) -> Result<(), String> {
    operation_keys(conn, operation_id)?;
    let stored = read_state(conn, operation_id)?.unwrap_or_else(empty_state);

    let state = WsdlState {
        response_body: String::new(),
        response_headers: Vec::new(),
        raw_request: String::new(),
        ..stored
    };
    write_state(conn, operation_id, &state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_schema;

    const SAMPLE: &str = r#"<wsdl:definitions xmlns:wsdl="http://schemas.xmlsoap.org/wsdl/"
      xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap/"
      xmlns:xsd="http://www.w3.org/2001/XMLSchema"
      xmlns:tns="http://example.com/people"
      targetNamespace="http://example.com/people">
  <wsdl:types>
    <xsd:schema targetNamespace="http://example.com/people">
      <xsd:element name="GetPerson" type="tns:GetPersonRequest"/>
      <xsd:element name="GetPersonResponse" type="tns:GetPersonResponse"/>
      <xsd:complexType name="GetPersonRequest">
        <xsd:sequence>
          <xsd:element name="id" type="xsd:int"/>
          <xsd:element name="comment" type="xsd:string" minOccurs="0"/>
        </xsd:sequence>
      </xsd:complexType>
      <xsd:complexType name="GetPersonResponse">
        <xsd:sequence>
          <xsd:element name="name" type="xsd:string"/>
        </xsd:sequence>
      </xsd:complexType>
    </xsd:schema>
  </wsdl:types>
  <wsdl:message name="GetPersonInput">
    <wsdl:part name="body" element="tns:GetPerson"/>
  </wsdl:message>
  <wsdl:message name="GetPersonOutput">
    <wsdl:part name="body" element="tns:GetPersonResponse"/>
  </wsdl:message>
  <wsdl:portType name="PeoplePortType">
    <wsdl:operation name="GetPerson">
      <wsdl:input message="tns:GetPersonInput"/>
      <wsdl:output message="tns:GetPersonOutput"/>
    </wsdl:operation>
  </wsdl:portType>
  <wsdl:binding name="PeopleBinding" type="tns:PeoplePortType">
    <soap:binding style="document" transport="http://schemas.xmlsoap.org/soap/http"/>
    <wsdl:operation name="GetPerson">
      <soap:operation soapAction="urn:GetPerson" style="document"/>
      <wsdl:input><soap:body use="literal"/></wsdl:input>
      <wsdl:output><soap:body use="literal"/></wsdl:output>
    </wsdl:operation>
  </wsdl:binding>
  <wsdl:service name="PeopleService">
    <wsdl:port name="PeoplePort" binding="tns:PeopleBinding">
      <soap:address location="http://127.0.0.1:9000/soap"/>
    </wsdl:port>
  </wsdl:service>
</wsdl:definitions>"#;

    /// A second service carrying the same `name` attribute: WSDL allows it, and
    /// it makes the deterministic service id collide — the case the transaction
    /// has to survive.
    const SAME_SERVICE_TWICE: &str = r#"  <wsdl:service name="PeopleService">
    <wsdl:port name="OtherPort" binding="tns:PeopleBinding">
      <soap:address location="http://127.0.0.1:9001/soap"/>
    </wsdl:port>
  </wsdl:service>
"#;

    fn memory_db() -> Connection {
        let connection = Connection::open_in_memory().expect("in-memory database");
        init_schema(&connection).expect("schema");
        connection
    }

    fn address(text: &str) -> reqwest::Url {
        reqwest::Url::parse(text).expect("url")
    }

    fn persist(
        conn: &mut Connection,
        project_id: &str,
        location: &str,
        source: &str,
    ) -> WsdlImportResult {
        let parsed = parse_wsdl(source).expect("parse");
        persist_file(conn, project_id, &address(location), source, &parsed).expect("persist")
    }

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get::<_, i64>(0)
        })
        .expect("count")
    }

    fn row(name: &str, value: &str) -> RestProperty {
        RestProperty {
            name: name.to_string(),
            value: value.to_string(),
        }
    }

    #[test]
    fn quotes_the_action_exactly_once() {
        assert_eq!(soap_action_for_call("urn:GetPerson"), "\"urn:GetPerson\"");
        assert_eq!(
            soap_action_for_call("\"urn:GetPerson\""),
            "\"urn:GetPerson\""
        );
        assert_eq!(soap_action_for_call(""), "");
    }

    #[test]
    fn locks_the_mandatory_headers_of_both_versions() {
        let one_one = locked_headers(SOAP_VERSION_11, "urn:GetPerson");
        assert_eq!(
            one_one,
            vec![
                row("Content-Type", "text/xml; charset=utf-8"),
                row("SOAPAction", "\"urn:GetPerson\"")
            ]
        );
        // The action row of a 1.1 binding is always there, even with no action.
        assert_eq!(
            locked_headers(SOAP_VERSION_11, "")[1],
            row("SOAPAction", "")
        );

        let one_two = locked_headers(SOAP_VERSION_12, "urn:GetPerson");
        assert_eq!(
            one_two,
            vec![row(
                "Content-Type",
                "application/soap+xml; charset=utf-8; action=\"urn:GetPerson\""
            )]
        );
        assert_eq!(
            locked_headers(SOAP_VERSION_12, ""),
            vec![row("Content-Type", "application/soap+xml; charset=utf-8")]
        );

        assert_eq!(
            locked_header_names(SOAP_VERSION_11, "a"),
            vec!["content-type", "soapaction"]
        );
        assert_eq!(
            locked_header_names(SOAP_VERSION_12, "a"),
            vec!["content-type"]
        );
    }

    #[test]
    fn drops_rows_that_clash_with_a_locked_name() {
        let editable = editable_for_save(
            SOAP_VERSION_11,
            "urn:GetPerson",
            &[
                row("  ", "discarded"),
                row("soapaction", "\"dup\""),
                row(" Content-Type ", "text/xml"),
                row("Authorization", "Basic x"),
            ],
        );
        assert_eq!(editable, vec![row("Authorization", "Basic x")]);

        // Mandatory rows come first and keep the order the user typed.
        let merged = merge_locked_headers(
            &locked_headers(SOAP_VERSION_12, "urn:GetPerson"),
            &[row("Accept", "text/xml"), row("", "noise")],
        );
        assert_eq!(
            merged,
            vec![
                row(
                    "Content-Type",
                    "application/soap+xml; charset=utf-8; action=\"urn:GetPerson\""
                ),
                row("Accept", "text/xml")
            ]
        );
    }

    #[test]
    fn writes_the_tree_and_the_request_template() {
        let conn = memory_db();
        let mut conn = conn;
        let result = persist(&mut conn, "", "http://example.com/people?wsdl", SAMPLE);

        assert!(!result.reimported);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert_eq!(result.summary.name, "people");
        assert_eq!(result.summary.source_url, "http://example.com/people?wsdl");
        assert_eq!(result.summary.services.len(), 1);

        let service = &result.summary.services[0];
        assert_eq!(service.name, "PeopleService");
        assert_eq!(service.operations.len(), 1);
        let operation = &service.operations[0];
        assert_eq!(operation.name, "GetPerson");
        assert_eq!(operation.soap_action, "urn:GetPerson");
        assert_eq!(operation.soap_version, SOAP_VERSION_11);
        assert_eq!(operation.endpoint_count, 1);

        let detail = select_operation_detail(&conn, &operation.id).expect("detail");
        assert_eq!(detail.input_element, "tns:GetPerson");
        assert_eq!(detail.output_element, "tns:GetPersonResponse");
        assert_eq!(detail.soap_version, SOAP_VERSION_11);
        assert!(detail.input_template.contains("<tns:GetPerson"));
        assert!(detail.input_template.contains("<id>"));
        // `minOccurs="0"` is a comment, not a forced element.
        assert!(detail.input_template.contains("comment"));
        // A note wrapped by its optional parent must stay one comment.
        assert!(
            !detail.input_template.contains("<!- -"),
            "{}",
            detail.input_template
        );
        assert_eq!(detail.endpoints[0].url, "http://127.0.0.1:9000/soap");
        assert_eq!(detail.endpoints[0].port_name, "PeoplePort");
        assert_eq!(detail.endpoints[0].soap_action, "urn:GetPerson");

        assert_eq!(count(&conn, "wsdl_files"), 1);
        assert_eq!(count(&conn, "wsdl_services"), 1);
        assert_eq!(count(&conn, "wsdl_operations"), 1);
        assert_eq!(count(&conn, "wsdl_endpoints"), 1);
    }

    #[test]
    fn keeps_ids_and_saved_state_through_a_reimport() {
        let conn = memory_db();
        let mut conn = conn;
        let first = persist(&mut conn, "", "http://example.com/people?wsdl", SAMPLE);
        let operation_id = first.summary.services[0].operations[0].id.clone();
        let endpoint_id = select_operation_detail(&conn, &operation_id)
            .expect("detail")
            .endpoints[0]
            .id
            .clone();

        // The user edits the body and adds a header; the ids must not move.
        persist_state(
            &conn,
            &WsdlOperationInput {
                operation_id: operation_id.clone(),
                endpoint_id: endpoint_id.clone(),
                request_body: "<edited/>".to_string(),
                headers: vec![row("Accept", "text/xml")],
                save_response: true,
            },
        )
        .expect("state");

        let again = persist(&mut conn, "", "http://example.com/people?wsdl", SAMPLE);
        assert!(again.reimported);
        assert_eq!(again.summary.id, first.summary.id);
        assert_eq!(again.summary.services[0].id, first.summary.services[0].id);
        assert_eq!(again.summary.services[0].operations[0].id, operation_id);

        let detail = select_operation_detail(&conn, &operation_id).expect("detail");
        assert_eq!(detail.state.request_body, "<edited/>");
        assert_eq!(detail.state.endpoint_id, endpoint_id);
        assert_eq!(detail.endpoints[0].id, endpoint_id);
        assert!(detail.state.save_response);
        // The mandatory rows are recomputed, the user row is kept behind them.
        assert_eq!(
            detail.state.headers,
            vec![
                row("Content-Type", "text/xml; charset=utf-8"),
                row("SOAPAction", "\"urn:GetPerson\""),
                row("Accept", "text/xml")
            ]
        );
        // The re-import replaced the tree but never stacked a second state row.
        assert_eq!(count(&conn, "wsdl_operation_state"), 1);
        assert_eq!(count(&conn, "wsdl_endpoints"), 1);
    }

    #[test]
    fn keeps_a_different_url_as_its_own_file() {
        let conn = memory_db();
        let mut conn = conn;
        persist(&mut conn, "", "http://example.com/a?wsdl", SAMPLE);
        let second = persist(&mut conn, "", "http://example.com/b?wsdl", SAMPLE);
        assert!(!second.reimported);
        assert_eq!(count(&conn, "wsdl_files"), 2);

        // The same URL inside a project is a re-import, in another project it is
        // a second file: the dedupe key is the pair, not the URL alone.
        let project = insert_project(&conn, "Demo").expect("project");
        let moved = persist(&mut conn, &project.id, "http://example.com/a?wsdl", SAMPLE);
        assert!(!moved.reimported);
        assert_eq!(moved.summary.project_id, project.id);
        assert_eq!(count(&conn, "wsdl_files"), 3);

        let back = persist(&mut conn, &project.id, "http://example.com/a?wsdl", SAMPLE);
        assert!(back.reimported);
        assert_eq!(count(&conn, "wsdl_files"), 3);
        assert_eq!(select_files(&conn).expect("files").len(), 3);
    }

    #[test]
    fn deletes_a_file_with_everything_below_it() {
        let conn = memory_db();
        let mut conn = conn;
        let kept = persist(&mut conn, "", "http://example.com/keep?wsdl", SAMPLE);
        let gone = persist(&mut conn, "", "http://example.com/gone?wsdl", SAMPLE);

        let gone_operation = gone.summary.services[0].operations[0].id.clone();
        let kept_operation = kept.summary.services[0].operations[0].id.clone();
        select_operation_detail(&conn, &gone_operation).expect("detail");
        select_operation_detail(&conn, &kept_operation).expect("detail");
        assert_eq!(count(&conn, "wsdl_operation_state"), 2);

        remove_file(&conn, &gone.summary.id).expect("remove");

        assert_eq!(count(&conn, "wsdl_files"), 1);
        assert_eq!(count(&conn, "wsdl_services"), 1);
        assert_eq!(count(&conn, "wsdl_operations"), 1);
        assert_eq!(count(&conn, "wsdl_endpoints"), 1);
        // The state of a file whose ids can no longer be resolved goes with it.
        assert_eq!(count(&conn, "wsdl_operation_state"), 1);
        assert!(select_operation_detail(&conn, &gone_operation).is_err());
        assert_eq!(
            select_operation_detail(&conn, &kept_operation)
                .expect("kept detail")
                .id,
            kept_operation
        );
    }

    #[test]
    fn deletes_a_project_and_the_files_it_holds() {
        let conn = memory_db();
        let mut conn = conn;
        let project = insert_project(&conn, "Demo").expect("project");
        persist(&mut conn, &project.id, "http://example.com/a?wsdl", SAMPLE);
        persist(&mut conn, &project.id, "http://example.com/b?wsdl", SAMPLE);
        persist(&mut conn, "", "http://example.com/loose?wsdl", SAMPLE);

        remove_project(&conn, &project.id).expect("remove project");

        assert_eq!(count(&conn, "wsdl_projects"), 0);
        assert_eq!(count(&conn, "wsdl_files"), 1);
        assert_eq!(count(&conn, "wsdl_services"), 1);
        assert_eq!(count(&conn, "wsdl_operations"), 1);
        // A file with no project is NULL, not an orphan of the deleted one.
        assert_eq!(select_files(&conn).expect("files")[0].project_id, "");
    }

    #[test]
    fn writes_nothing_when_the_document_cannot_be_stored() {
        let conn = memory_db();
        let mut conn = conn;
        // Two services with the same name make the deterministic id collide.
        let source = SAMPLE.replace(
            "</wsdl:definitions>",
            &format!("{SAME_SERVICE_TWICE}</wsdl:definitions>"),
        );
        let parsed = parse_wsdl(&source).expect("parse");
        assert_eq!(parsed.services.len(), 2);

        let error = persist_file(
            &mut conn,
            "",
            &address("http://example.com/twice?wsdl"),
            &source,
            &parsed,
        )
        .expect_err("collision");
        assert!(
            error.contains("UNIQUE") || error.contains("PRIMARY KEY"),
            "{error}"
        );

        // The transaction rolled back: no half-written tree, no file row.
        assert_eq!(count(&conn, "wsdl_files"), 0);
        assert_eq!(count(&conn, "wsdl_services"), 0);
        assert_eq!(count(&conn, "wsdl_operations"), 0);
        assert_eq!(count(&conn, "wsdl_endpoints"), 0);
        assert_eq!(select_files(&conn).expect("files").len(), 0);
    }

    #[test]
    fn opens_an_operation_with_the_template_and_the_locked_headers() {
        let conn = memory_db();
        let mut conn = conn;
        let result = persist(&mut conn, "", "http://example.com/people?wsdl", SAMPLE);
        let operation_id = result.summary.services[0].operations[0].id.clone();

        // The state row does not exist until the operation is opened.
        assert_eq!(count(&conn, "wsdl_operation_state"), 0);
        let first = select_operation_detail(&conn, &operation_id).expect("detail");
        assert_eq!(count(&conn, "wsdl_operation_state"), 1);

        assert_eq!(first.state.request_body, first.input_template);
        assert_eq!(first.state.endpoint_id, first.endpoints[0].id);
        assert_eq!(
            first.state.headers,
            locked_headers(SOAP_VERSION_11, "urn:GetPerson")
        );
        assert!(!first.state.save_response);
        assert_eq!(first.soap_version, SOAP_VERSION_11);

        // Reading it twice does not create a second row or duplicate headers.
        let again = select_operation_detail(&conn, &operation_id).expect("detail");
        assert_eq!(again.state, first.state);
        assert_eq!(count(&conn, "wsdl_operation_state"), 1);
    }

    #[test]
    fn clears_an_endpoint_of_another_operation_and_its_locked_row() {
        let conn = memory_db();
        let mut conn = conn;
        let first = persist(&mut conn, "", "http://example.com/a?wsdl", SAMPLE);
        let second = persist(&mut conn, "", "http://example.com/b?wsdl", SAMPLE);
        let operation_id = first.summary.services[0].operations[0].id.clone();
        let foreign = second.summary.services[0].operations[0].id.clone();

        persist_state(
            &conn,
            &WsdlOperationInput {
                operation_id: operation_id.clone(),
                endpoint_id: foreign,
                request_body: "<a/>".to_string(),
                headers: vec![row("SOAPAction", "\"stale\"")],
                save_response: false,
            },
        )
        .expect("state");

        let detail = select_operation_detail(&conn, &operation_id).expect("detail");
        // A selected endpoint the operation does not own is dropped, so the
        // version falls back to the first endpoint of this operation.
        assert_eq!(detail.state.endpoint_id, detail.endpoints[0].id);
        assert_eq!(detail.state.request_body, "<a/>");
        // The stale action row is gone: the stored one is rebuilt from the file.
        assert_eq!(
            detail.state.headers,
            locked_headers(SOAP_VERSION_11, "urn:GetPerson")
        );
    }

    #[test]
    fn response_and_request_do_not_overwrite_each_other() {
        let conn = memory_db();
        let mut conn = conn;
        let result = persist(&mut conn, "", "http://example.com/people?wsdl", SAMPLE);
        let operation_id = result.summary.services[0].operations[0].id.clone();

        persist_state(
            &conn,
            &WsdlOperationInput {
                operation_id: operation_id.clone(),
                endpoint_id: String::new(),
                request_body: "<request/>".to_string(),
                headers: vec![row("Accept", "text/xml")],
                save_response: true,
            },
        )
        .expect("state");

        store_response(
            &conn,
            &operation_id,
            "<fault/>",
            &[row("Content-Type", "text/xml; charset=utf-8")],
            "POST /soap HTTP/1.1",
        )
        .expect("response");

        let stored = read_state(&conn, &operation_id)
            .expect("state")
            .expect("row");
        assert_eq!(stored.request_body, "<request/>");
        assert_eq!(stored.response_body, "<fault/>");
        assert_eq!(stored.raw_request, "POST /soap HTTP/1.1");
        assert_eq!(stored.response_headers.len(), 1);

        // The request stays when the response is cleared, and the other way round.
        drop_response(&conn, &operation_id).expect("clear");
        let cleared = read_state(&conn, &operation_id)
            .expect("state")
            .expect("row");
        assert_eq!(cleared.response_body, "");
        assert!(cleared.response_headers.is_empty());
        assert_eq!(cleared.raw_request, "");
        assert_eq!(cleared.request_body, "<request/>");

        persist_state(
            &conn,
            &WsdlOperationInput {
                operation_id: operation_id.clone(),
                endpoint_id: String::new(),
                request_body: "<second/>".to_string(),
                headers: Vec::new(),
                save_response: false,
            },
        )
        .expect("state again");
        let kept = read_state(&conn, &operation_id)
            .expect("state")
            .expect("row");
        assert_eq!(kept.request_body, "<second/>");
        assert_eq!(kept.response_body, "");
        assert!(!kept.save_response);
    }
}
