use axum::http::StatusCode;
use axum::routing::get;
use axum::Router;
use tokio::net::TcpListener;

#[tokio::test]
async fn reqwest_client_fetches_plain_http() {
    let app = Router::new().route(
        "/health",
        get(|| async { (StatusCode::OK, r#"{"ok":true}"#) }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .expect("client");
    let response = client
        .get(format!("http://{addr}/health"))
        .send()
        .await
        .expect("send");

    assert!(response.status().is_success());
    let body: serde_json::Value = response.json().await.expect("json");
    assert_eq!(body["ok"], serde_json::json!(true));
}
