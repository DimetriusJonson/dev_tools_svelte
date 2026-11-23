use std::net::SocketAddr;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::json;
use tokio::sync::watch;

use crate::share::{
    normalize_id, viewer_response, ShareServerState, ShareServerStatus, ShareStore, SHARE_PORT,
};

pub fn start(store: ShareStore) -> ShareServerState {
    let state = ShareServerState::default();
    bind_and_serve(&state, store);
    state
}

pub fn restart(state: &ShareServerState, store: ShareStore) -> ShareServerStatus {
    state.stop();
    bind_and_serve(state, store);
    state.snapshot()
}

fn bind_and_serve(state: &ShareServerState, store: ShareStore) {
    let listener = match std::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], SHARE_PORT))) {
        Ok(listener) => listener,
        Err(error) => {
            state.mark_error(format!("Could not start share server: {error}"));
            return;
        }
    };
    if let Err(error) = listener.set_nonblocking(true) {
        state.mark_error(format!("Could not start share server: {error}"));
        return;
    }

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    state.mark_started(shutdown_tx);
    // Nothing expired should still answer a request once the listener is up.
    store.purge_expired();

    let app = router(store);
    let task_state = state.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = serve(listener, app, shutdown_rx).await {
            task_state.mark_error(error);
        }
    });
}

async fn serve(
    listener: std::net::TcpListener,
    app: Router,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), String> {
    let listener = tokio::net::TcpListener::from_std(listener)
        .map_err(|error| format!("Could not start share server: {error}"))?;

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            loop {
                if *shutdown.borrow() {
                    break;
                }
                if shutdown.changed().await.is_err() {
                    break;
                }
            }
        })
        .await
        .map_err(|error| format!("Share server stopped: {error}"))
}

pub fn router(store: ShareStore) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/uploads/{id}", get(get_upload).delete(delete_upload))
        .route("/api/uploads/{id}/raw", get(get_raw))
        .with_state(store)
}

async fn health() -> impl IntoResponse {
    Json(json!({ "ok": true }))
}

async fn get_upload(Path(id): Path<String>, State(store): State<ShareStore>) -> Response {
    let id = match normalize_id(&id) {
        Ok(id) => id,
        Err(_) => return not_found(),
    };

    let upload = match store.find(&id) {
        Ok(Some(upload)) => upload,
        Ok(None) => return not_found(),
        Err(_) => return internal_error(),
    };

    Json(viewer_response(&upload)).into_response()
}

async fn get_raw(Path(id): Path<String>, State(store): State<ShareStore>) -> Response {
    let id = match normalize_id(&id) {
        Ok(id) => id,
        Err(_) => return not_found(),
    };

    let upload = match store.find(&id) {
        Ok(Some(upload)) => upload,
        Ok(None) => return not_found(),
        Err(_) => return internal_error(),
    };

    let mut headers = HeaderMap::new();
    if let Ok(value) = HeaderValue::from_str(&format!(
        "attachment; filename=\"{}\"",
        header_filename(&upload.filename)
    )) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    if let Ok(value) = HeaderValue::from_str(&upload.content_type) {
        headers.insert(header::CONTENT_TYPE, value);
    }

    (headers, Body::from(upload.data)).into_response()
}

/// Removes an upload. Nothing authenticates the caller beyond the id: the link is what
/// the sharer handed out, and whoever holds it decides when the payload is gone.
async fn delete_upload(Path(id): Path<String>, State(store): State<ShareStore>) -> Response {
    let id = match normalize_id(&id) {
        Ok(id) => id,
        Err(_) => return not_found(),
    };

    match store.delete(&id) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => not_found(),
        Err(_) => internal_error(),
    }
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "Not found").into_response()
}

fn internal_error() -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
}

fn header_filename(filename: &str) -> String {
    let name = filename
        .chars()
        .map(|character| {
            if character.is_ascii()
                && !character.is_ascii_control()
                && character != '"'
                && character != '\\'
            {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if name.is_empty() {
        "upload.bin".to_string()
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use crate::db;
    use http_body_util::BodyExt;
    use rusqlite::Connection;
    use tower::ServiceExt;

    fn test_store() -> (ShareStore, String) {
        let connection = Connection::open_in_memory().expect("database");
        db::init_schema(&connection).expect("schema");
        let id = db::insert_shared_upload(
            &connection,
            "data.json",
            "application/json",
            br#"{"hello":"world"}"#,
            None,
        )
        .expect("insert");
        (
            ShareStore::new(db::AppDb {
                conn: Arc::new(Mutex::new(connection)),
            }),
            id,
        )
    }

    fn memory_upload() -> db::SharedUpload {
        db::SharedUpload {
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            filename: "scratch.txt".to_string(),
            content_type: "text/plain".to_string(),
            data: b"temporary".to_vec(),
            size: 9,
            created_at: "now".to_string(),
            expires_at: None,
            stored: false,
        }
    }

    async fn body_text(response: Response) -> String {
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        String::from_utf8(bytes.to_vec()).expect("utf8")
    }

    async fn request(router: Router, method: &str, uri: String) -> Response {
        router
            .oneshot(
                axum::http::Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response")
    }

    #[tokio::test]
    async fn health_responds_ok() {
        let (store, _) = test_store();
        let response = request(router(store), "GET", "/health".to_string()).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(body_text(response).await.contains("\"ok\":true"));
    }

    #[tokio::test]
    async fn upload_endpoint_returns_viewer_payload() {
        let (store, id) = test_store();
        let response = request(router(store), "GET", format!("/api/uploads/{id}")).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_text(response).await;
        assert!(body.contains("\"viewer\":\"code\""));
        assert!(body.contains("\"language\":\"json\""));
        assert!(body.contains(r#"{\"hello\":\"world\"}"#));
        assert!(body.contains(&format!("/api/uploads/{id}/raw")));
        assert!(body.contains("\"stored\":true"));
    }

    #[tokio::test]
    async fn raw_endpoint_downloads_attachment() {
        let (store, id) = test_store();
        let response = request(router(store), "GET", format!("/api/uploads/{id}/raw")).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_DISPOSITION)
                .expect("disposition"),
            "attachment; filename=\"data.json\""
        );
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .expect("content type"),
            "application/json"
        );
        assert_eq!(body_text(response).await, r#"{"hello":"world"}"#);
    }

    #[tokio::test]
    async fn unknown_or_invalid_id_is_not_found() {
        for id in ["missing", "00000000-0000-4000-8000-000000000000"] {
            let (store, _) = test_store();
            let response = request(router(store), "GET", format!("/api/uploads/{id}")).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
    }

    #[tokio::test]
    async fn delete_removes_a_stored_upload_once() {
        let (store, id) = test_store();
        let response = request(
            router(store.clone()),
            "DELETE",
            format!("/api/uploads/{id}"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let response = request(router(store.clone()), "GET", format!("/api/uploads/{id}")).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = request(router(store), "DELETE", format!("/api/uploads/{id}")).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn delete_rejects_a_malformed_id_without_touching_anything() {
        let (store, id) = test_store();
        let response = request(
            router(store.clone()),
            "DELETE",
            "/api/uploads/nope".to_string(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let response = request(router(store), "GET", format!("/api/uploads/{id}")).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn memory_upload_answers_and_leaves_nothing_after_delete() {
        let (store, _) = test_store();
        store.memory.insert(memory_upload());
        let id = "550e8400-e29b-41d4-a716-446655440000".to_string();

        let response = request(router(store.clone()), "GET", format!("/api/uploads/{id}")).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_text(response).await;
        assert!(body.contains("\"stored\":false"));

        let response = request(
            router(store.clone()),
            "DELETE",
            format!("/api/uploads/{id}"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(store.memory.len(), 0);

        let response = request(router(store), "GET", format!("/api/uploads/{id}")).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn header_filename_is_safe() {
        assert_eq!(header_filename("a\"b\\c\n.txt"), "a_b_c_.txt");
        assert_eq!(header_filename("файл.txt"), "____.txt");
        assert_eq!(header_filename(""), "upload.bin");
    }
}
