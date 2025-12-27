use anyhow::Context;
use stoopid_short::{config, url_repo::url_repository_capsule};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let container = config::init_container().await?;

    container
        .read(url_repository_capsule)
        .delete_expired_urls()
        .await
        .context("Failed to delete expired URLs")
}
