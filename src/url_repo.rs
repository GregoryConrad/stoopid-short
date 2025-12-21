use std::sync::Arc;

use async_trait::async_trait;
use rearch::CapsuleHandle;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, DbConn, EntityTrait};
use thiserror::Error;
use time::{Duration, OffsetDateTime};
use tracing::instrument;
use url::Url;

use crate::{config::db_conn_capsule, orm::short_url};

#[derive(Debug)]
pub struct ShortUrl {
    pub(crate) short_id: ShortId,
    pub(crate) url: Url,
    pub(crate) expiration_time: ExpirationTime,
}

#[derive(Debug)]
pub struct ShortId {
    inner: String,
}
impl ShortId {
    pub(crate) fn new(short_id: String) -> Result<Self, ShortIdValidationError> {
        let (min_len, max_len) = (6, 16);
        if !(min_len..=max_len).contains(&short_id.len()) {
            return Err(ShortIdValidationError::InvalidLength { min_len, max_len });
        }

        let invalid_chars = short_id
            .chars()
            .filter(|c| !c.is_ascii_alphanumeric())
            .collect::<String>();
        if !invalid_chars.is_empty() {
            return Err(ShortIdValidationError::InvalidCharacters { invalid_chars });
        }

        Ok(Self { inner: short_id })
    }
}
#[derive(Debug, Error)]
pub enum ShortIdValidationError {
    #[error("short ID length must be between {min_len} and {max_len}")]
    InvalidLength { min_len: usize, max_len: usize },
    #[error("short ID must only contain alpha-numeric characters; invalid chars: {invalid_chars}")]
    InvalidCharacters { invalid_chars: String },
}
impl From<ShortId> for String {
    fn from(value: ShortId) -> Self {
        value.inner
    }
}

#[derive(Debug)]
pub struct ExpirationTime {
    inner: OffsetDateTime,
}
impl ExpirationTime {
    pub(crate) fn new(
        proposed_time: OffsetDateTime,
    ) -> Result<Self, ExpirationTimeValidationError> {
        const MAX_TTL: Duration = Duration::days(10 * 365);

        let now = OffsetDateTime::now_utc();
        if proposed_time < now {
            return Err(ExpirationTimeValidationError::InPast);
        }

        let max_time = now + MAX_TTL;
        if proposed_time > max_time {
            return Err(ExpirationTimeValidationError::TooFarInFuture { max_time });
        }

        Ok(Self {
            inner: proposed_time,
        })
    }
}
#[derive(Debug, Error)]
pub enum ExpirationTimeValidationError {
    #[error("expiration time is too far in the future; the current maximum is {max_time}")]
    TooFarInFuture { max_time: OffsetDateTime },
    #[error("expiration time cannot be in the past")]
    InPast,
}
impl From<ExpirationTime> for OffsetDateTime {
    fn from(value: ExpirationTime) -> Self {
        value.inner
    }
}

pub fn url_repository_capsule(
    CapsuleHandle { mut get, .. }: CapsuleHandle,
) -> Arc<dyn UrlRepository> {
    let db = get.as_ref(db_conn_capsule).clone();
    Arc::new(UrlRepositoryImpl { db })
}

#[async_trait]
pub trait UrlRepository: Send + Sync {
    async fn retrieve_url(&self, id: &str) -> anyhow::Result<Option<ShortUrl>>;

    /// Idempotently saves the [`ShortUrl`] to the database.
    async fn save_url(&self, url: ShortUrl) -> anyhow::Result<ShortUrl>;
}

impl TryFrom<short_url::Model> for ShortUrl {
    type Error = anyhow::Error;

    fn try_from(
        short_url::Model {
            id,
            long_url,
            expiration_time_seconds,
        }: short_url::Model,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            short_id: ShortId::new(id)?,
            url: Url::parse(&long_url)?,
            expiration_time: ExpirationTime::new(*expiration_time_seconds)?,
        })
    }
}

struct UrlRepositoryImpl {
    db: DbConn,
}

#[async_trait]
impl UrlRepository for UrlRepositoryImpl {
    #[instrument(skip(self))]
    async fn retrieve_url(&self, id: &str) -> anyhow::Result<Option<ShortUrl>> {
        let opt_url = short_url::Entity::find_by_id(id).one(&self.db).await?;
        opt_url.map(TryInto::try_into).transpose()
        // TODO: if expired, None
    }

    #[instrument(skip(self))]
    async fn save_url(&self, url: ShortUrl) -> anyhow::Result<ShortUrl> {
        let to_insert = short_url::ActiveModel {
            id: Set(url.short_id.inner),
            long_url: Set(url.url.as_str().to_owned()),
            expiration_time_seconds: Set(url.expiration_time.inner.into()),
        };
        to_insert.insert(&self.db).await?.try_into()
        // TODO: if error, but expired, delete and re-try (up to 3 times)
    }
}
