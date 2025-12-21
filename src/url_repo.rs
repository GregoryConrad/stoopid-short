use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use rearch::CapsuleHandle;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, DbConn, EntityTrait, TransactionError, TransactionTrait,
};
use thiserror::Error;
use time::{Duration, OffsetDateTime};
use tracing::instrument;
use url::Url;

use crate::{config::db_conn_capsule, orm::short_url};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ShortUrl {
    pub(crate) short_id: ShortId,
    pub(crate) url: Url,
    pub(crate) expiration_time: ExpirationTime,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
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

    pub(crate) fn into_inner(self) -> String {
        self.inner
    }
}
#[derive(Debug, Error)]
pub enum ShortIdValidationError {
    #[error("short ID length must be between {min_len} and {max_len}")]
    InvalidLength { min_len: usize, max_len: usize },
    #[error("short ID must only contain alpha-numeric characters; invalid chars: {invalid_chars}")]
    InvalidCharacters { invalid_chars: String },
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
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

    pub(crate) const fn into_inner(self) -> OffsetDateTime {
        self.inner
    }
}
#[derive(Debug, Error)]
pub enum ExpirationTimeValidationError {
    #[error("expiration time is too far in the future; the current maximum is {max_time}")]
    TooFarInFuture { max_time: OffsetDateTime },
    #[error("expiration time cannot be in the past")]
    InPast,
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
    async fn save_url(&self, url: ShortUrl) -> Result<ShortUrl, SaveUrlError>;
}

#[derive(Debug, Error)]
pub enum SaveUrlError {
    #[error("an item with the specified id already exists in database and is not expired")]
    ItemAlreadyExists(ShortUrl),
    #[error("internal/database error: {0}")]
    Internal(#[from] anyhow::Error),
}

struct UrlRepositoryImpl {
    db: DbConn,
}

// NOTE: Our expired items cleanup is async, so we may fetch items that are already expired.
#[async_trait]
impl UrlRepository for UrlRepositoryImpl {
    #[instrument(skip(self))]
    async fn retrieve_url(&self, id: &str) -> anyhow::Result<Option<ShortUrl>> {
        let opt_url = short_url::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .context("Failed to query for existing item")?;
        opt_url
            .filter(|model| *model.expiration_time_seconds >= OffsetDateTime::now_utc())
            .map(TryInto::try_into)
            .transpose()
    }

    #[instrument(skip(self))]
    async fn save_url(&self, short_url: ShortUrl) -> Result<ShortUrl, SaveUrlError> {
        let short_id = short_url.short_id.into_inner();
        let long_url = short_url.url.as_str().to_owned();
        let expiration_time = short_url.expiration_time.into_inner();

        let inserted_model = self
            .db
            .transaction(|txn| {
                Box::pin(async move {
                    if let Some(existing) = short_url::Entity::find_by_id(&short_id)
                        .one(txn)
                        .await
                        .context("Failed to query for an existing item")?
                    {
                        if *existing.expiration_time_seconds >= OffsetDateTime::now_utc() {
                            return Err(SaveUrlError::ItemAlreadyExists(existing.try_into()?));
                        }

                        short_url::Entity::delete_by_id(existing.id)
                            .exec(txn)
                            .await
                            .context("Failed to delete existing expired item")?;
                    }

                    let to_insert = short_url::ActiveModel {
                        id: Set(short_id),
                        long_url: Set(long_url),
                        expiration_time_seconds: Set(expiration_time.into()),
                    };

                    Ok(to_insert
                        .insert(txn)
                        .await
                        .context("Failed to insert new item")?)
                })
            })
            .await
            .map_err(|txn_err| match txn_err {
                TransactionError::Connection(_) => anyhow::Error::from(txn_err)
                    .context("Failed to execute database transaction due to database connection")
                    .into(),
                TransactionError::Transaction(save_url_error) => save_url_error,
            })?;

        inserted_model.try_into().map_err(SaveUrlError::from)
    }
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
            short_id: ShortId::new(id).context("Failed to create ShortId from db model")?,
            url: Url::parse(&long_url).context("Failed to parse Url from db model")?,
            expiration_time: ExpirationTime::new(*expiration_time_seconds)
                .context("Failed to create ExpirationTime from db model")?,
        })
    }
}
