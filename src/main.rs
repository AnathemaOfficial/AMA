use ama::errors;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    tracing::info!("AMA P0 starting...");
}
