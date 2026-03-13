mod errors;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    tracing::info!("AMA P0 starting...");
    // Config loading, server setup will be added in later tasks
}
