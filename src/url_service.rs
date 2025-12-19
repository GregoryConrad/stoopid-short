use std::sync::Arc;

use async_trait::async_trait;
use rearch::CapsuleHandle;
use sea_orm::DbErr;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tracing::instrument;

use crate::{
    api,
    orm::short_url,
    url_repo::{UrlRepository, url_repository_capsule},
};

pub fn url_rest_service_capsule(
    CapsuleHandle { mut get, .. }: CapsuleHandle,
) -> Arc<dyn UrlRestService> {
    let url_repo = Arc::clone(get.as_ref(url_repository_capsule));
    Arc::new(UrlRestServiceImpl { url_repo })
}

#[async_trait]
pub trait UrlRestService: Send + Sync {
    async fn get_url(&self, id: &str) -> Result<api::Redirect, GetUrlError>;
    async fn put_url(
        &self,
        id: String,
        url: String,
        expiration_timestamp: &str,
    ) -> Result<api::ShortenedUrl, PutUrlError>;
    async fn post_url(
        &self,
        url: String,
        expiration_timestamp: String,
    ) -> Result<api::ShortenedUrl, PostUrlError>;
}

struct UrlRestServiceImpl {
    url_repo: Arc<dyn UrlRepository>,
}

#[async_trait]
impl UrlRestService for UrlRestServiceImpl {
    #[instrument(skip(self))]
    async fn get_url(&self, id: &str) -> Result<api::Redirect, GetUrlError> {
        match self.url_repo.retrieve_url(id).await {
            Ok(Some(url)) => Ok(api::Redirect { url: url.long_url }),
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
    ) -> Result<api::ShortenedUrl, PutUrlError> {
        // TODO validate id is in base62, proper len (6-20). make a new capsule for this validation
        // TODO validate expiration_timestamp is within the next 10 years (or so)
        // TODO validate long_url is a valid URL

        let expiration_time =
            OffsetDateTime::parse(expiration_timestamp, &Rfc3339)?.to_offset(time::UtcOffset::UTC);

        self.url_repo
            .save_url(short_url::Model {
                id,
                long_url,
                expiration_time_seconds: expiration_time.into(),
            })
            .await?
            .try_into()
            .map_err(PutUrlError::TimestampFormat)

        // TODO handle the already-created and identical case up here by modifying return type
    }

    #[instrument(skip(self))]
    async fn post_url(
        &self,
        url: String,
        expiration_timestamp: String,
    ) -> Result<api::ShortenedUrl, PostUrlError> {
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

pub enum GetUrlError {
    NotFound,
    Db(DbErr),
}

#[derive(Debug, thiserror::Error)]
pub enum PutUrlError {
    #[error("Failed to parse timestamp from request")]
    TimestampParse(#[from] time::error::Parse),
    #[error("Failed to format timestamp from database as String")]
    TimestampFormat(#[from] time::error::Format),
    #[error("Database error")]
    Db(#[from] DbErr),
}

#[derive(Debug, thiserror::Error)]
pub enum PostUrlError {
    #[error("Database error")]
    DbError(#[from] DbErr),
}

impl TryFrom<short_url::Model> for api::ShortenedUrl {
    fn try_from(
        short_url::Model {
            id,
            long_url,
            expiration_time_seconds,
        }: short_url::Model,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            shortened_url_id: id,
            long_url,
            expiration_timestamp: expiration_time_seconds.format(&Rfc3339)?,
        })
    }

    type Error = time::error::Format;
}
