mod parser;
mod store;
mod xsd;

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use tokio::sync::oneshot;

use crate::db::AppDb;
use crate::rest::RestProperty;

/// A WSDL file never grows past 5 MB, so the whole document fits in memory
/// and the parser can index it in one pass.
pub const MAX_WSDL_BYTES: usize = 5 * 1024 * 1024;

/// Fetching a WSDL takes no longer than this. The REST client's own timeout is
/// longer on purpose, so a slow server shows up as a WSDL import error.
pub const FETCH_TIMEOUT_SECONDS: u32 = 30;

/// A request template nests this many element levels before it is refused.
pub const MAX_TEMPLATE_DEPTH: u8 = 32;

/// A request template stops after this many elements; the rest becomes a comment.
pub const MAX_TEMPLATE_ELEMENTS: usize = 2000;

/// The WSDL document itself is refused above this many elements.
pub const MAX_WSDL_ELEMENTS: usize = 20000;

/// The SOAP version an endpoint and an operation carry through to execution.
pub const SOAP_VERSION_11: &str = "1.1";
pub const SOAP_VERSION_12: &str = "1.2";

/// The namespace a WSDL 1.1 document declares on its root element. Element
/// matching is done by this resolved namespace and never by the prefix the
/// author happened to pick, which is what makes a `<wsdl:…>` and a plain
/// `<…>` inside the default declaration the same thing to the parser.
pub const WSDL_NAMESPACE: &str = "http://schemas.xmlsoap.org/wsdl/";

/// The namespace a WSDL 2.0 document declares. The app refuses it with a
/// localized reason instead of half-parsing a tree of wrong names.
pub const WSDL2_NAMESPACE: &str = "http://www.w3.org/2002/wsdl/";

/// The namespace `<soap:binding>` and `<soap:address>` live in for SOAP 1.1.
pub const SOAP_NAMESPACE: &str = "http://schemas.xmlsoap.org/wsdl/soap/";

/// The namespace `<soap12:binding>` lives in. The binding element's own
/// namespace is what decides the SOAP version of an operation.
pub const SOAP12_NAMESPACE: &str = "http://schemas.xmlsoap.org/wsdl/soap12/";

/// The XML Schema namespace. Type definitions live here even when the WSDL
/// itself is in the WSDL namespace, which is why element matching is done by
/// resolved namespace URI and never by the prefix the author happened to pick.
pub const XSD_NAMESPACE: &str = "http://www.w3.org/2001/XMLSchema";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WsdlProject {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WsdlFileSummary {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub source_url: String,
    pub warnings: Vec<String>,
    pub services: Vec<WsdlService>,
    pub imported_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WsdlService {
    pub id: String,
    pub name: String,
    pub operations: Vec<WsdlOperation>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WsdlOperation {
    pub id: String,
    pub name: String,
    pub soap_version: String,
    pub soap_action: String,
    pub endpoint_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WsdlEndpoint {
    pub id: String,
    pub url: String,
    pub soap_version: String,
    pub soap_action: String,
    pub port_name: String,
    pub binding_name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WsdlState {
    pub endpoint_id: String,
    pub request_body: String,
    pub headers: Vec<RestProperty>,
    pub save_response: bool,
    pub response_body: String,
    pub response_headers: Vec<RestProperty>,
    pub raw_request: String,
    /// The status line the server answered with (`HTTP/1.1 200 OK`). The Raw response
    /// tab rebuilds the whole exchange from it; a row written before this column or an
    /// answer with no status known holds the empty string.
    #[serde(default)]
    pub response_status_line: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WsdlOperationDetail {
    pub id: String,
    pub name: String,
    pub service_id: String,
    pub service_name: String,
    pub soap_version: String,
    pub soap_action: String,
    pub input_element: String,
    pub output_element: String,
    pub input_template: String,
    pub endpoints: Vec<WsdlEndpoint>,
    pub state: WsdlState,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WsdlImportResult {
    pub summary: WsdlFileSummary,
    pub reimported: bool,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WsdlOperationInput {
    pub operation_id: String,
    pub endpoint_id: String,
    pub request_body: String,
    pub headers: Vec<RestProperty>,
    pub save_response: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WsdlFileRow {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub source_url: String,
    pub warnings: Vec<String>,
    pub imported_at: String,
}

/// A second WSDL import may be cancelled from the frontend before or while it runs.
/// The running request holds a oneshot receiver; a cancel that arrives earlier only
/// raises the flag, so the next import refuses to fetch the URL again.
#[derive(Clone, Default)]
pub struct WsdlFetchState {
    cancel_requested: Arc<Mutex<bool>>,
    sender: Arc<Mutex<Option<oneshot::Sender<()>>>>,
}

impl WsdlFetchState {
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

    fn queue_cancellation(&self) {
        if let Ok(mut slot) = self.cancel_requested.lock() {
            *slot = true;
        }
    }

    fn take_pending(&self) -> bool {
        self.cancel_requested
            .lock()
            .ok()
            .map(|mut slot| {
                let held = *slot;
                *slot = false;
                held
            })
            .unwrap_or(false)
    }
}

#[tauri::command(rename = "list_wsdl_projects", rename_all = "snake_case")]
pub fn cmd_list_wsdl_projects(db: tauri::State<'_, AppDb>) -> Result<Vec<WsdlProject>, String> {
    store::list_projects(db)
}

#[tauri::command(rename = "create_wsdl_project", rename_all = "snake_case")]
pub fn cmd_create_wsdl_project(
    db: tauri::State<'_, AppDb>,
    name: String,
) -> Result<WsdlProject, String> {
    store::create_project(db, name)
}

#[tauri::command(rename = "rename_wsdl_project", rename_all = "snake_case")]
pub fn cmd_rename_wsdl_project(
    db: tauri::State<'_, AppDb>,
    id: String,
    name: String,
) -> Result<(), String> {
    store::rename_project(db, id, name)
}

#[tauri::command(rename = "delete_wsdl_project", rename_all = "snake_case")]
pub fn cmd_delete_wsdl_project(db: tauri::State<'_, AppDb>, id: String) -> Result<(), String> {
    store::delete_project(db, id)
}

#[tauri::command(rename = "list_wsdl_files", rename_all = "snake_case")]
pub fn cmd_list_wsdl_files(db: tauri::State<'_, AppDb>) -> Result<Vec<WsdlFileSummary>, String> {
    store::list_files(db)
}

#[tauri::command(rename = "import_wsdl", rename_all = "snake_case")]
pub async fn cmd_import_wsdl(
    db: tauri::State<'_, AppDb>,
    cancel: tauri::State<'_, WsdlFetchState>,
    project_id: String,
    url: String,
) -> Result<WsdlImportResult, String> {
    store::import_file(db, cancel, project_id, url).await
}

#[tauri::command(rename = "delete_wsdl_file", rename_all = "snake_case")]
pub fn cmd_delete_wsdl_file(db: tauri::State<'_, AppDb>, id: String) -> Result<(), String> {
    store::delete_file(db, id)
}

#[tauri::command(rename = "cancel_wsdl_fetch", rename_all = "snake_case")]
pub fn cmd_cancel_wsdl_fetch(cancel: tauri::State<'_, WsdlFetchState>) -> bool {
    if cancel.cancel() {
        return true;
    }
    cancel.queue_cancellation();
    true
}

#[tauri::command(rename = "get_wsdl_operation", rename_all = "snake_case")]
pub fn cmd_get_wsdl_operation(
    db: tauri::State<'_, AppDb>,
    operation_id: String,
) -> Result<WsdlOperationDetail, String> {
    store::get_operation(db, operation_id)
}

#[tauri::command(rename = "save_wsdl_operation_state", rename_all = "snake_case")]
pub fn cmd_save_wsdl_operation_state(
    db: tauri::State<'_, AppDb>,
    operation_id: String,
    endpoint_id: String,
    request_body: String,
    headers: Vec<RestProperty>,
    save_response: bool,
) -> Result<(), String> {
    store::save_operation_state(
        db,
        WsdlOperationInput {
            operation_id,
            endpoint_id,
            request_body,
            headers,
            save_response,
        },
    )
}

#[tauri::command(rename = "save_wsdl_response", rename_all = "snake_case")]
pub fn cmd_save_wsdl_response(
    db: tauri::State<'_, AppDb>,
    operation_id: String,
    body: String,
    headers: Vec<RestProperty>,
    raw_request: String,
    status_line: Option<String>,
) -> Result<(), String> {
    store::save_response(db, operation_id, body, headers, raw_request, status_line)
}

#[tauri::command(rename = "clear_wsdl_response", rename_all = "snake_case")]
pub fn cmd_clear_wsdl_response(
    db: tauri::State<'_, AppDb>,
    operation_id: String,
) -> Result<(), String> {
    store::clear_response(db, operation_id)
}
