use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::Url;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_deep_link::DeepLinkExt;
use tokio::sync::watch;
use uuid::Uuid;

use crate::db::{self, AppDb};

pub const SHARE_SCHEME: &str = "webdev-useful-tools";
pub const SHARE_PORT: u16 = 3030;
pub const MAX_UPLOAD_BYTES: usize = 50 * 1024 * 1024;
pub const DEEP_LINK_EVENT: &str = "share:deep-link";
/// Memory-only uploads are kept until the app closes; the cap stops a long session from
/// holding an unbounded pile of payloads.
pub const MAX_MEMORY_UPLOADS: usize = 24;

/// How long an upload survives. `Memory` never reaches SQLite: the payload lives in RAM
/// only, so its link dies with the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retention {
    Memory,
    Days(u32),
    Always,
}

/// The codes the frontend sends; an unknown one is a bug rather than a user typo.
pub fn parse_retention(raw: &str) -> Option<Retention> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "none" => Some(Retention::Memory),
        "1d" => Some(Retention::Days(1)),
        "3d" => Some(Retention::Days(3)),
        "7d" => Some(Retention::Days(7)),
        "always" => Some(Retention::Always),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeepLinkTarget {
    pub server: String,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NetworkInterface {
    pub name: String,
    pub ip: String,
    pub url: String,
    pub is_loopback: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ShareServerStatus {
    pub running: bool,
    pub port: u16,
    pub error: Option<String>,
}

impl Default for ShareServerStatus {
    fn default() -> Self {
        Self {
            running: false,
            port: SHARE_PORT,
            error: None,
        }
    }
}

#[derive(Clone, Default)]
pub struct ShareServerState {
    status: Arc<Mutex<ShareServerStatus>>,
    shutdown: Arc<Mutex<Option<watch::Sender<bool>>>>,
}

impl ShareServerState {
    pub fn snapshot(&self) -> ShareServerStatus {
        self.status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_default()
    }

    pub fn mark_started(&self, shutdown: watch::Sender<bool>) {
        if let Ok(mut status) = self.status.lock() {
            *status = ShareServerStatus {
                running: true,
                port: SHARE_PORT,
                error: None,
            };
        }
        if let Ok(mut sender) = self.shutdown.lock() {
            *sender = Some(shutdown);
        }
    }

    pub fn mark_error(&self, message: impl Into<String>) {
        if let Ok(mut status) = self.status.lock() {
            status.running = false;
            status.error = Some(message.into());
        }
    }

    pub fn stop(&self) {
        if let Ok(mut sender) = self.shutdown.lock() {
            if let Some(sender) = sender.take() {
                let _ = sender.send(true);
            }
        }
    }
}

/// Uploads the user chose not to store. They answer the HTTP endpoints exactly like
/// database rows, but only for as long as this process runs.
#[derive(Clone, Default)]
pub struct MemoryUploads {
    entries: Arc<Mutex<MemoryEntries>>,
}

#[derive(Default)]
struct MemoryEntries {
    uploads: HashMap<String, (db::SharedUpload, u64)>,
    sequence: u64,
}

impl MemoryUploads {
    pub fn insert(&self, upload: db::SharedUpload) {
        {
            let Ok(mut entries) = self.entries.lock() else {
                return;
            };
            entries.sequence += 1;
            let sequence = entries.sequence;
            entries
                .uploads
                .insert(upload.id.clone(), (upload, sequence));
        }
        // The cap is re-read through `len` after every eviction, so an insert that raced
        // with this one cannot leave the map over the limit.
        while self.len() > MAX_MEMORY_UPLOADS {
            let Ok(mut entries) = self.entries.lock() else {
                return;
            };
            let oldest = entries
                .uploads
                .values()
                .min_by_key(|(_, sequence)| *sequence)
                .map(|(upload, _)| upload.id.clone());
            match oldest {
                Some(id) => {
                    entries.uploads.remove(&id);
                }
                None => break,
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<db::SharedUpload> {
        self.entries
            .lock()
            .ok()?
            .uploads
            .get(id)
            .map(|(upload, _)| upload.clone())
    }

    pub fn remove(&self, id: &str) -> bool {
        self.entries
            .lock()
            .map(|mut entries| entries.uploads.remove(id).is_some())
            .unwrap_or(false)
    }

    pub fn len(&self) -> usize {
        self.entries
            .lock()
            .map(|entries| entries.uploads.len())
            .unwrap_or(0)
    }
}

/// Everything the share server reads from: expired rows are filtered out on the way.
#[derive(Clone)]
pub struct ShareStore {
    pub db: AppDb,
    pub memory: MemoryUploads,
}

impl ShareStore {
    pub fn new(db: AppDb) -> Self {
        Self {
            db,
            memory: MemoryUploads::default(),
        }
    }

    /// Drops uploads whose retention ran out. Reads and writes call it, so no timer is
    /// needed to keep the database from holding expired payloads.
    pub fn purge_expired(&self) {
        if let Ok(connection) = self.db.conn.lock() {
            let _ = db::purge_expired_uploads(&connection);
        }
    }

    pub fn find(&self, id: &str) -> Result<Option<db::SharedUpload>, String> {
        if let Some(upload) = self.memory.get(id) {
            return Ok(Some(upload));
        }
        self.purge_expired();
        let connection = self.db.conn.lock().map_err(|error| error.to_string())?;
        db::read_shared_upload(&connection, id).map_err(|error| error.to_string())
    }

    /// Forgets an upload, in memory or in the database. The share link is the whole
    /// credential: anybody holding it can drop the payload it points at.
    pub fn delete(&self, id: &str) -> Result<bool, String> {
        if self.memory.remove(id) {
            return Ok(true);
        }
        let connection = self.db.conn.lock().map_err(|error| error.to_string())?;
        db::delete_shared_upload(&connection, id).map_err(|error| error.to_string())
    }
}

#[derive(Clone, Default)]
pub struct PendingDeepLink {
    target: Arc<Mutex<Option<DeepLinkTarget>>>,
}

impl PendingDeepLink {
    pub fn set(&self, target: DeepLinkTarget) {
        if let Ok(mut slot) = self.target.lock() {
            *slot = Some(target);
        }
    }

    pub fn take(&self) -> Option<DeepLinkTarget> {
        self.target.lock().ok().and_then(|mut slot| slot.take())
    }

    pub fn clear(&self) {
        if let Ok(mut slot) = self.target.lock() {
            *slot = None;
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SharedUploadResponse {
    pub id: String,
    pub filename: String,
    pub content_type: String,
    pub size: i64,
    pub created_at: String,
    pub viewer: String,
    pub language: Option<String>,
    pub content: Option<String>,
    pub raw_url: String,
    /// `false` when the payload is only in the sharer's memory.
    pub stored: bool,
    /// `None` means the upload does not expire.
    pub expires_at: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ShareUploadResult {
    pub id: String,
    pub link: String,
    pub server: String,
    pub filename: String,
    pub size: u64,
    /// `false` when the payload lives in the sharer's RAM only and dies with the app.
    pub stored: bool,
    /// `None` for a memory-only upload or one kept forever.
    pub expires_at: Option<String>,
}

pub fn is_lan_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
}

pub fn normalize_server(raw: &str) -> Result<String, String> {
    let url = Url::parse(raw.trim()).map_err(|error| format!("Invalid server URL: {error}"))?;
    if url.scheme() != "http" {
        return Err("Share server must use http".to_string());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Share server must not contain credentials".to_string());
    }
    if url.port() != Some(SHARE_PORT) {
        return Err(format!("Share server must use port {SHARE_PORT}"));
    }
    if !matches!(url.path(), "" | "/") || url.query().is_some() || url.fragment().is_some() {
        return Err("Share server must be a bare origin".to_string());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "Share server has no host".to_string())?;
    let ip: Ipv4Addr = host
        .parse()
        .map_err(|_| "Share server must be an IPv4 address".to_string())?;
    if !is_lan_ipv4(ip) {
        return Err("Share server must be on the local network".to_string());
    }
    Ok(format!("http://{ip}:{SHARE_PORT}"))
}

pub fn normalize_id(raw: &str) -> Result<String, String> {
    let parsed = Uuid::parse_str(raw.trim()).map_err(|_| "Invalid upload id".to_string())?;
    if parsed.get_version_num() != 4 {
        return Err("Invalid upload id".to_string());
    }
    Ok(parsed.to_string())
}

pub fn build_deep_link(server: &str, id: &str) -> Result<String, String> {
    let mut url = Url::parse(&format!("{SHARE_SCHEME}://view"))
        .map_err(|error| format!("Could not build share link: {error}"))?;
    url.query_pairs_mut()
        .append_pair("server", server)
        .append_pair("id", id);
    Ok(url.to_string())
}

pub fn parse_deep_link(raw: &str) -> Result<DeepLinkTarget, String> {
    let url = Url::parse(raw).map_err(|error| format!("Invalid deep link: {error}"))?;
    if url.scheme() != SHARE_SCHEME || url.host_str() != Some("view") {
        return Err("Unexpected deep link target".to_string());
    }
    if !matches!(url.path(), "" | "/") {
        return Err("Unexpected deep link path".to_string());
    }

    let mut server = None;
    let mut id = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "server" => server = Some(value.into_owned()),
            "id" => id = Some(value.into_owned()),
            _ => {}
        }
    }

    let server = normalize_server(&server.ok_or_else(|| "Deep link has no server".to_string())?)?;
    let id = normalize_id(&id.ok_or_else(|| "Deep link has no upload id".to_string())?)?;
    Ok(DeepLinkTarget { server, id })
}

pub fn language_for(content_type: &str, filename: &str) -> Option<&'static str> {
    let content_type = content_type.to_ascii_lowercase();
    let filename = filename.to_ascii_lowercase();

    if content_type.contains("json") || filename.ends_with(".json") {
        return Some("json");
    }
    if content_type.contains("xml") || filename.ends_with(".xml") {
        return Some("xml");
    }
    if content_type.starts_with("text/")
        || content_type.contains("javascript")
        || content_type.contains("yaml")
        || content_type.contains("toml")
        || [
            ".txt", ".md", ".log", ".csv", ".yaml", ".yml", ".toml", ".ini", ".cfg", ".conf",
            ".js", ".ts", ".css", ".html", ".sql",
        ]
        .iter()
        .any(|extension| filename.ends_with(extension))
    {
        return Some("text");
    }
    None
}

pub fn viewer_response(upload: &db::SharedUpload) -> SharedUploadResponse {
    let language = language_for(&upload.content_type, &upload.filename);
    let content = language.and_then(|_| String::from_utf8(upload.data.clone()).ok());

    let (viewer, language, content) = match (language, content) {
        (Some(language), Some(content)) => (
            "code".to_string(),
            Some(language.to_string()),
            Some(content),
        ),
        _ => ("binary".to_string(), None, None),
    };

    SharedUploadResponse {
        id: upload.id.clone(),
        filename: upload.filename.clone(),
        content_type: upload.content_type.clone(),
        size: upload.size,
        created_at: upload.created_at.clone(),
        viewer,
        language,
        content,
        raw_url: format!("/api/uploads/{}/raw", upload.id),
        stored: upload.stored,
        expires_at: upload.expires_at.clone(),
    }
}

pub fn list_network_interfaces() -> Vec<NetworkInterface> {
    let mut interfaces = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|interface| match interface.addr {
            if_addrs::IfAddr::V4(address) if is_lan_ipv4(address.ip) => Some(NetworkInterface {
                name: interface.name,
                ip: address.ip.to_string(),
                url: format!("http://{}:{SHARE_PORT}", address.ip),
                is_loopback: address.ip.is_loopback(),
            }),
            _ => None,
        })
        .collect::<Vec<_>>();

    interfaces.sort_by(|left, right| {
        left.is_loopback
            .cmp(&right.is_loopback)
            .then_with(|| left.ip.cmp(&right.ip))
    });
    interfaces.dedup_by(|left, right| left.ip == right.ip);

    if interfaces.is_empty() {
        interfaces.push(NetworkInterface {
            name: "loopback".to_string(),
            ip: "127.0.0.1".to_string(),
            url: format!("http://127.0.0.1:{SHARE_PORT}"),
            is_loopback: true,
        });
    }

    interfaces
}

fn sanitize_filename(filename: &str) -> String {
    let name = filename
        .replace(['/', '\\', '\r', '\n', '"'], "_")
        .trim()
        .to_string();
    if name.is_empty() {
        "upload.bin".to_string()
    } else {
        name
    }
}

fn filename_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .map(|name| sanitize_filename(&name))
        .unwrap_or_else(|| "upload.bin".to_string())
}

fn content_type_from_path(path: &str) -> String {
    mime_guess::from_path(path)
        .first_or_octet_stream()
        .essence_str()
        .to_string()
}

fn validate_payload_size(size: usize) -> Result<(), String> {
    if size > MAX_UPLOAD_BYTES {
        Err(format!(
            "File is too large. Maximum size is {} MB",
            MAX_UPLOAD_BYTES / (1024 * 1024)
        ))
    } else {
        Ok(())
    }
}

fn upload_result(
    id: String,
    server: String,
    filename: String,
    size: u64,
    stored: bool,
    expires_at: Option<String>,
) -> Result<ShareUploadResult, String> {
    let link = build_deep_link(&server, &id)?;
    Ok(ShareUploadResult {
        id,
        link,
        server,
        filename,
        size,
        stored,
        expires_at,
    })
}

/// Stores the payload according to the retention the user picked. `Memory` never reaches
/// SQLite, so the entry disappears when the app closes and cannot be deleted by hand.
fn store_upload(
    store: &ShareStore,
    filename: String,
    content_type: String,
    data: &[u8],
    server: String,
    raw_retention: &str,
) -> Result<ShareUploadResult, String> {
    validate_payload_size(data.len())?;
    let retention =
        parse_retention(raw_retention).ok_or_else(|| "Unknown retention option".to_string())?;
    let size = data.len() as u64;

    if retention == Retention::Memory {
        let upload = {
            let connection = store.db.conn.lock().map_err(|error| error.to_string())?;
            db::SharedUpload {
                id: Uuid::new_v4().to_string(),
                filename: filename.clone(),
                content_type,
                data: data.to_vec(),
                size: size as i64,
                created_at: db::now_timestamp(&connection).map_err(|error| error.to_string())?,
                expires_at: None,
                stored: false,
            }
        };
        let id = upload.id.clone();
        store.memory.insert(upload);
        return upload_result(id, server, filename, size, false, None);
    }

    let (id, expires_at) = {
        let connection = store.db.conn.lock().map_err(|error| error.to_string())?;
        let expires_at = match retention {
            Retention::Days(days) => {
                Some(db::expiry_timestamp(&connection, days).map_err(|error| error.to_string())?)
            }
            Retention::Always | Retention::Memory => None,
        };
        let written = db::insert_shared_upload(
            &connection,
            &filename,
            &content_type,
            data,
            expires_at.as_deref(),
        )?;
        (written, expires_at)
    };
    upload_result(id, server, filename, size, true, expires_at)
}

fn focus_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn handle_deep_link_urls(app: &AppHandle, urls: Vec<Url>) {
    for url in urls {
        if let Ok(target) = parse_deep_link(url.as_str()) {
            if let Some(pending) = app.try_state::<PendingDeepLink>() {
                pending.set(target.clone());
            }
            focus_main_window(app);
            let _ = app.emit(DEEP_LINK_EVENT, target);
            break;
        }
    }
}

pub fn install_deep_link_handler(app: &AppHandle) {
    #[cfg(all(debug_assertions, any(target_os = "windows", target_os = "linux")))]
    if let Err(error) = app.deep_link().register_all() {
        eprintln!("Could not register deep link scheme: {error}");
    }

    if let Ok(Some(urls)) = app.deep_link().get_current() {
        handle_deep_link_urls(app, urls);
    }

    let app_handle = app.clone();
    app.deep_link().on_open_url(move |event| {
        handle_deep_link_urls(&app_handle, event.urls());
    });
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_share_interfaces() -> Vec<NetworkInterface> {
    list_network_interfaces()
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_share_server_status(status: State<'_, ShareServerState>) -> ShareServerStatus {
    status.snapshot()
}

#[tauri::command(rename_all = "snake_case")]
pub fn restart_share_server(
    store: State<'_, ShareStore>,
    status: State<'_, ShareServerState>,
) -> ShareServerStatus {
    crate::http_server::restart(&status, store.inner().clone())
}

#[tauri::command(rename_all = "snake_case")]
pub fn upload_share_content(
    store: State<'_, ShareStore>,
    content: String,
    filename: String,
    content_type: String,
    server: String,
    retention: String,
) -> Result<ShareUploadResult, String> {
    let server = normalize_server(&server)?;
    store_upload(
        store.inner(),
        sanitize_filename(&filename),
        content_type,
        content.as_bytes(),
        server,
        &retention,
    )
}

#[tauri::command(rename_all = "snake_case")]
pub async fn upload_share_file(
    store: State<'_, ShareStore>,
    path: String,
    server: String,
    retention: String,
) -> Result<ShareUploadResult, String> {
    let server = normalize_server(&server)?;
    let store = store.inner().clone();

    tauri::async_runtime::spawn_blocking(move || {
        let metadata = std::fs::metadata(&path).map_err(|error| error.to_string())?;
        if metadata.len() > MAX_UPLOAD_BYTES as u64 {
            return Err(format!(
                "File is too large. Maximum size is {} MB",
                MAX_UPLOAD_BYTES / (1024 * 1024)
            ));
        }
        let data = std::fs::read(&path).map_err(|error| error.to_string())?;
        let filename = filename_from_path(&path);
        let content_type = content_type_from_path(&path);
        store_upload(&store, filename, content_type, &data, server, &retention)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn load_shared_upload(
    server: String,
    id: String,
) -> Result<SharedUploadResponse, String> {
    let server = normalize_server(&server)?;
    let id = normalize_id(&id)?;
    let url = format!("{server}/api/uploads/{id}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("Share server is unavailable: {error}"))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("Upload not found".to_string());
    }
    if !response.status().is_success() {
        return Err(format!("Share server returned {}", response.status()));
    }

    response
        .json::<SharedUploadResponse>()
        .await
        .map_err(|error| format!("Invalid share response: {error}"))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn download_shared_upload(
    server: String,
    id: String,
    path: String,
) -> Result<(), String> {
    let server = normalize_server(&server)?;
    let id = normalize_id(&id)?;
    let url = format!("{server}/api/uploads/{id}/raw");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("Share server is unavailable: {error}"))?;

    if !response.status().is_success() {
        return Err(format!("Share server returned {}", response.status()));
    }

    let bytes = response.bytes().await.map_err(|error| error.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(parent) = Path::new(&path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
        }
        std::fs::write(path, bytes).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command(rename_all = "snake_case")]
pub async fn delete_shared_upload(server: String, id: String) -> Result<(), String> {
    let server = normalize_server(&server)?;
    let id = normalize_id(&id)?;
    let url = format!("{server}/api/uploads/{id}");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .delete(url)
        .send()
        .await
        .map_err(|error| format!("Share server is unavailable: {error}"))?;

    match response.status() {
        reqwest::StatusCode::NO_CONTENT => Ok(()),
        reqwest::StatusCode::NOT_FOUND => Err("Upload not found".to_string()),
        status => Err(format!("Share server returned {status}")),
    }
}

#[tauri::command(rename_all = "snake_case")]
pub fn consume_pending_deep_link(pending: State<'_, PendingDeepLink>) -> Option<DeepLinkTarget> {
    pending.take()
}
#[tauri::command(rename_all = "snake_case")]
pub fn clear_pending_deep_link(pending: State<'_, PendingDeepLink>) {
    pending.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "550e8400-e29b-41d4-a716-446655440000";

    #[test]
    fn builds_and_parses_deep_link() {
        let link = build_deep_link("http://192.168.1.10:3030", ID).expect("link");
        let parsed = parse_deep_link(&link).expect("parse");
        assert_eq!(
            parsed,
            DeepLinkTarget {
                server: "http://192.168.1.10:3030".to_string(),
                id: ID.to_string(),
            }
        );
    }

    #[test]
    fn rejects_unsafe_links() {
        assert!(parse_deep_link(
            "https://view?server=http://192.168.1.10:3030&id=550e8400-e29b-41d4-a716-446655440000"
        )
        .is_err());
        assert!(parse_deep_link("webdev-useful-tools://view?server=http://8.8.8.8:3030&id=550e8400-e29b-41d4-a716-446655440000").is_err());
        assert!(parse_deep_link(
            "webdev-useful-tools://view?server=file://tmp&id=550e8400-e29b-41d4-a716-446655440000"
        )
        .is_err());
        assert!(parse_deep_link("webdev-useful-tools://view?server=http://192.168.1.10:4000&id=550e8400-e29b-41d4-a716-446655440000").is_err());
        assert!(parse_deep_link(
            "webdev-useful-tools://view?server=http://192.168.1.10:3030&id=nope"
        )
        .is_err());
    }

    #[test]
    fn detects_viewer_language() {
        assert_eq!(language_for("application/json", "data.bin"), Some("json"));
        assert_eq!(language_for("application/xml", "data.bin"), Some("xml"));
        assert_eq!(language_for("text/plain", "notes.bin"), Some("text"));
        assert_eq!(language_for("application/octet-stream", "data.bin"), None);
        assert_eq!(
            language_for("application/octet-stream", "README.md"),
            Some("text")
        );
    }

    #[test]
    fn text_response_contains_content() {
        let upload = db::SharedUpload {
            id: ID.to_string(),
            filename: "data.json".to_string(),
            content_type: "application/json".to_string(),
            data: br#"{"a":1}"#.to_vec(),
            size: 7,
            created_at: "now".to_string(),
            expires_at: None,
            stored: true,
        };
        let response = viewer_response(&upload);
        assert_eq!(response.viewer, "code");
        assert_eq!(response.language.as_deref(), Some("json"));
        assert_eq!(response.content.as_deref(), Some(r#"{"a":1}"#));
        assert!(response.stored);
        assert_eq!(response.expires_at, None);
    }

    #[test]
    fn binary_response_has_no_content() {
        let upload = db::SharedUpload {
            id: ID.to_string(),
            filename: "data.bin".to_string(),
            content_type: "application/octet-stream".to_string(),
            data: vec![0, 1, 2, 3],
            size: 4,
            created_at: "now".to_string(),
            expires_at: Some("2030-01-01 00:00:00".to_string()),
            stored: false,
        };
        let response = viewer_response(&upload);
        assert_eq!(response.viewer, "binary");
        assert_eq!(response.language, None);
        assert_eq!(response.content, None);
        assert_eq!(response.raw_url, format!("/api/uploads/{ID}/raw"));
        assert!(!response.stored);
        assert_eq!(response.expires_at.as_deref(), Some("2030-01-01 00:00:00"));
    }

    #[test]
    fn parses_every_retention_option() {
        assert_eq!(parse_retention("none"), Some(Retention::Memory));
        assert_eq!(parse_retention("1D"), Some(Retention::Days(1)));
        assert_eq!(parse_retention(" 3d "), Some(Retention::Days(3)));
        assert_eq!(parse_retention("7d"), Some(Retention::Days(7)));
        assert_eq!(parse_retention("always"), Some(Retention::Always));
        assert_eq!(parse_retention(""), None);
        assert_eq!(parse_retention("30d"), None);
    }

    #[test]
    fn memory_store_evicts_the_oldest_entry() {
        let uploads = MemoryUploads::default();
        let mut ids = Vec::new();
        for index in 0..=MAX_MEMORY_UPLOADS {
            let id = format!("{index:08}-{index:08}-{index:08}-{index:08}");
            uploads.insert(db::SharedUpload {
                id: id.clone(),
                filename: format!("{index}.txt"),
                content_type: "text/plain".to_string(),
                data: vec![index as u8],
                size: 1,
                created_at: "now".to_string(),
                expires_at: None,
                stored: false,
            });
            ids.push(id);
        }

        assert_eq!(uploads.len(), MAX_MEMORY_UPLOADS);
        assert_eq!(uploads.get(&ids[0]), None, "the oldest entry gave way");
        let newest = &ids[ids.len() - 1];
        assert!(uploads.get(newest).is_some());
        assert!(uploads.remove(newest), "the newest entry left on request");
        assert!(
            !uploads.remove(newest),
            "a second delete finds nothing to remove"
        );
        assert!(uploads.remove(&ids[ids.len() - 2]));
        assert_eq!(uploads.get(&ids[ids.len() - 2]), None);
        assert!(!uploads.remove("absent"));
    }

    #[test]
    fn builds_a_link_without_a_token_and_ignores_stray_parameters() {
        let link = build_deep_link("http://192.168.1.10:3030", ID).expect("link");
        assert!(!link.contains("token"));
        let parsed = parse_deep_link(&format!("{link}&token=0123456789abcdef")).expect("parse");
        assert_eq!(parsed.server, "http://192.168.1.10:3030");
        assert_eq!(parsed.id, ID);
    }

    #[test]
    fn rejects_public_server() {
        assert!(normalize_server("http://8.8.8.8:3030").is_err());
        assert!(normalize_server("http://192.168.1.10:3030").is_ok());
        assert!(normalize_server("http://127.0.0.1:3030").is_ok());
    }

    #[test]
    fn pending_link_can_be_taken_once() {
        let pending = PendingDeepLink::default();
        pending.set(DeepLinkTarget {
            server: "http://192.168.1.10:3030".to_string(),
            id: ID.to_string(),
        });
        assert!(pending.take().is_some());
        assert!(pending.take().is_none());
    }

    #[test]
    fn server_state_tracks_start_error_and_stop() {
        let state = ShareServerState::default();
        assert!(!state.snapshot().running);
        assert_eq!(state.snapshot().port, SHARE_PORT);

        let (sender, mut receiver) = watch::channel(false);
        state.mark_started(sender);
        let started = state.snapshot();
        assert!(started.running);
        assert_eq!(started.error, None);

        state.mark_error("port busy".to_string());
        let failed = state.snapshot();
        assert!(!failed.running);
        assert_eq!(failed.error.as_deref(), Some("port busy"));

        state.stop();
        assert_eq!(*receiver.borrow_and_update(), true);
    }
}
