use std::sync::Arc;

use async_trait::async_trait;
use rearch::CapsuleHandle;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DbConn, DbErr, EntityTrait};
use tracing::instrument;

use crate::{config::db_conn_capsule, orm::short_url};

pub fn url_repository_capsule(
    CapsuleHandle { mut get, .. }: CapsuleHandle,
) -> Arc<dyn UrlRepository> {
    let db = get.as_ref(db_conn_capsule).clone();
    Arc::new(UrlRepositoryImpl { db })
}

#[async_trait]
pub trait UrlRepository: Send + Sync {
    async fn retrieve_url(&self, id: &str) -> Result<Option<short_url::Model>, DbErr>;

    /// Idempotently saves the [`short_url::Model`] to the database.
    async fn save_url(&self, url: short_url::Model) -> Result<short_url::Model, DbErr>;
}

struct UrlRepositoryImpl {
    db: DbConn,
}

#[async_trait]
impl UrlRepository for UrlRepositoryImpl {
    #[instrument(skip(self))]
    async fn retrieve_url(&self, id: &str) -> Result<Option<short_url::Model>, DbErr> {
        short_url::Entity::find_by_id(id).one(&self.db).await
    }

    #[instrument(skip(self))]
    async fn save_url(&self, url: short_url::Model) -> Result<short_url::Model, DbErr> {
        let to_insert = short_url::ActiveModel {
            id: Set(url.id),
            long_url: Set(url.long_url),
            expiration_time: Set(url.expiration_time),
        };
        to_insert.insert(&self.db).await
    }
}
