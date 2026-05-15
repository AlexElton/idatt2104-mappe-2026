mod crdt;
mod server;

use server::handler::{router, AppState};

#[tokio::main]
async fn main() {
    let state = AppState::new();
    let app = router(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("failed to bind port 3000");

    println!("Listening on http://localhost:3000");
    axum::serve(listener, app).await.expect("server error");
}
