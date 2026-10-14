use bmcloud_server::{db, router, state::AppState};

#[tokio::main]
async fn main() {
    let db_path = std::env::var("BMCLOUD_DB_PATH").unwrap_or_else(|_| "bmcloud.db".to_string());
    let bind_addr = std::env::var("BMCLOUD_BIND_ADDR").unwrap_or("0.0.0.0:80".to_string());

    let conn = db::init(&db_path).expect("Failed to initialize database");
    let state = AppState::new(conn);

    let app = router(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await.unwrap();

    let addr = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| bind_addr.clone());

    println!("Server is running: http://{addr}");

    axum::serve(listener, app).await.unwrap();
}
