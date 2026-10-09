use bmcloud_server::{db, router, state::AppState};

#[tokio::main]
async fn main() {
    let conn = db::init("bmcloud.db").expect("Failed to initialize database");
    let state = AppState::new(conn);

    let app = router(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    println!("Сервер запущен: http://localhost:3000");

    axum::serve(listener, app).await.unwrap();
}
