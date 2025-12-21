use std::sync::Arc;

use async_trait::async_trait;
use rearch::CapsuleHandle;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tracing::instrument;
use url::Url;

use crate::url_repo::{self, UrlRepository, url_repository_capsule};

#[derive(Deserialize)]
pub struct PutUrlPayload {
    pub url: String,
    pub expiration_timestamp: String,
}

#[derive(Deserialize)]
pub struct PostUrlPayload {
    pub url: String,
    pub expiration_timestamp: String,
}

#[derive(Debug, Serialize)]
pub struct ShortenedUrl {
    pub shortened_url_id: String,
    pub long_url: String,
    /// Timestamp in ISO-8601 format
    pub expiration_timestamp: String,
}

pub struct Redirect {
    pub url: String,
}

pub fn url_rest_service_capsule(
    CapsuleHandle { mut get, .. }: CapsuleHandle,
) -> Arc<dyn UrlRestService> {
    let url_repo = Arc::clone(get.as_ref(url_repository_capsule));
    Arc::new(UrlRestServiceImpl { url_repo })
}

#[async_trait]
pub trait UrlRestService: Send + Sync {
    async fn get_url(&self, id: &str) -> Result<Redirect, GetUrlError>;
    async fn put_url(
        &self,
        id: String,
        url: String,
        expiration_timestamp: &str,
    ) -> Result<(ShortenedUrl, UrlCreationStatus), PutUrlError>;
    async fn post_url(
        &self,
        url: String,
        expiration_timestamp: String,
    ) -> Result<ShortenedUrl, PostUrlError>;
}

pub enum GetUrlError {
    NotFound,
    Db(anyhow::Error),
}

pub enum UrlCreationStatus {
    NewlyCreated,
    AlreadyExists,
}

#[derive(Debug, Error)]
pub enum PutUrlError {
    #[error("failed to parse timestamp: {0}")]
    TimestampParse(#[from] time::error::Parse),
    #[error("invalid expiration time: {0}")]
    InvalidExpirationTime(#[from] url_repo::ExpirationTimeValidationError),
    #[error("invalid short ID: {0}")]
    InvalidShortId(#[from] url_repo::ShortIdValidationError),
    #[error("invalid URL: {0}")]
    InvalidUrl(#[from] url::ParseError),
    #[error("short ID is already taken")]
    ShortIdAlreadyTaken,
    #[error("failed to format timestamp: {0}")]
    TimestampFormat(#[from] time::error::Format),
    #[error("database error: {0}")]
    Db(anyhow::Error),
}

#[derive(Debug, Error)]
pub enum PostUrlError {
    #[error("database error: {0}")]
    Db(anyhow::Error),
}

struct UrlRestServiceImpl {
    url_repo: Arc<dyn UrlRepository>,
}

#[async_trait]
impl UrlRestService for UrlRestServiceImpl {
    #[instrument(skip(self))]
    async fn get_url(&self, id: &str) -> Result<Redirect, GetUrlError> {
        match self.url_repo.retrieve_url(id).await {
            Ok(Some(url)) => Ok(Redirect {
                url: url.url.as_str().to_owned(),
            }),
            Ok(None) => Err(GetUrlError::NotFound),
            Err(err) => Err(GetUrlError::Db(err)),
        }
    }

    #[instrument(skip(self))]
    async fn put_url(
        &self,
        id: String,
        long_url: String,
        expiration_timestamp: &str,
    ) -> Result<(ShortenedUrl, UrlCreationStatus), PutUrlError> {
        let expiration_time =
            OffsetDateTime::parse(expiration_timestamp, &Rfc3339)?.to_offset(time::UtcOffset::UTC);

        self.url_repo
            .save_url(url_repo::ShortUrl {
                short_id: url_repo::ShortId::new(id)?,
                url: Url::parse(&long_url)?,
                expiration_time: url_repo::ExpirationTime::new(expiration_time)?,
            })
            .await
            .map_err(PutUrlError::Db)?
            .try_into()
            .map_err(PutUrlError::TimestampFormat)
            // TODO inspect DbErr type to see if we need to return AlreadyExists or ShortIdAlreadyTaken
            .map(|short_url| (short_url, UrlCreationStatus::NewlyCreated))
    }

    #[instrument(skip(self))]
    async fn post_url(
        &self,
        url: String,
        expiration_timestamp: String,
    ) -> Result<ShortenedUrl, PostUrlError> {
        // TODO
        // 1. canonicalize URL
        // 2. init salt to 0
        // 3. Some SHA variant (or similar)
        // 4. Take first X bytes
        // 5. base 62
        // 6. try PUT call
        // 7. if fail, randomize salt
        // 8. retry (go back to step 3) up to 3 times
        //    on retries, also consider increasing byte length too
        // 9. return shortened URL info
        // StatusCode::CREATED // for newly created
        todo!("TODO: POST with {url} {expiration_timestamp}")
    }
}

impl TryFrom<url_repo::ShortUrl> for ShortenedUrl {
    type Error = time::error::Format;

    fn try_from(
        url_repo::ShortUrl {
            short_id,
            url,
            expiration_time,
        }: url_repo::ShortUrl,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            shortened_url_id: short_id.into_inner(),
            long_url: url.into(),
            expiration_timestamp: expiration_time.into_inner().format(&Rfc3339)?,
        })
    }
}
