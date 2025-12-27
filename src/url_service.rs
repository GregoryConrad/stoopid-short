use std::sync::Arc;

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use rand::RngCore;
use rearch::CapsuleHandle;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tracing::{error, instrument, warn};
use url::Url;

use crate::url_repo::{
    self, ExpirationTime, ExpirationTimeValidationError, SaveUrlError, ShortId,
    ShortIdValidationError, UrlRepository, url_repository_capsule,
};

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

#[derive(Debug)]
pub struct Redirect {
    pub url: String,
    pub max_age_seconds: u64,
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
        url: &str,
        expiration_timestamp: &str,
    ) -> Result<(ShortenedUrl, UrlCreationStatus), PutUrlError>;
    async fn post_url(
        &self,
        url: &str,
        expiration_timestamp: &str,
    ) -> Result<ShortenedUrl, PostUrlError>;
}

#[derive(Debug)]
pub enum GetUrlError {
    NotFound,
    Db(anyhow::Error),
}

#[derive(Debug, PartialEq, Eq)]
pub enum UrlCreationStatus {
    NewlyCreated,
    AlreadyExists,
}

#[derive(Debug, Error)]
pub enum PutUrlError {
    #[error("failed to parse timestamp: {0}")]
    TimestampParse(#[from] time::error::Parse),
    #[error("invalid expiration time: {0}")]
    InvalidExpirationTime(#[from] ExpirationTimeValidationError),
    #[error("invalid short ID: {0}")]
    InvalidShortId(#[from] ShortIdValidationError),
    #[error("invalid URL: {0}")]
    InvalidUrl(#[from] url::ParseError),
    #[error("short ID is already taken")]
    ShortIdAlreadyTaken,
    #[error("internal/database error: {0}")]
    Internal(anyhow::Error), // NOTE: no #[from] so we have to be explicit
}

#[derive(Debug, Error)]
pub enum PostUrlError {
    #[error("failed to parse timestamp: {0}")]
    TimestampParse(#[from] time::error::Parse),
    #[error("invalid expiration time: {0}")]
    InvalidExpirationTime(#[from] ExpirationTimeValidationError),
    #[error("invalid URL: {0}")]
    InvalidUrl(#[from] url::ParseError),
    #[error("internal/database error: {0}")]
    Internal(anyhow::Error), // NOTE: no #[from] so we have to be explicit
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
                max_age_seconds: (url.expiration_time.into_inner() - OffsetDateTime::now_utc())
                    .whole_seconds()
                    .try_into()
                    .unwrap_or(0),
            }),
            Ok(None) => Err(GetUrlError::NotFound),
            Err(err) => Err(GetUrlError::Db(err)),
        }
    }

    #[instrument(skip(self))]
    async fn put_url(
        &self,
        id: String,
        long_url: &str,
        expiration_timestamp: &str,
    ) -> Result<(ShortenedUrl, UrlCreationStatus), PutUrlError> {
        let expiration_time =
            OffsetDateTime::parse(expiration_timestamp, &Rfc3339)?.to_offset(time::UtcOffset::UTC);

        let to_save = url_repo::ShortUrl {
            short_id: ShortId::new(id)?,
            url: Url::parse(long_url)?,
            expiration_time: ExpirationTime::new(expiration_time)?,
        };

        match self.url_repo.save_url(to_save.clone()).await {
            Ok(short_url) => Ok((
                short_url
                    .try_into()
                    .context("Failed to convert new ShortUrl into external format")
                    .map_err(PutUrlError::Internal)?,
                UrlCreationStatus::NewlyCreated,
            )),
            Err(SaveUrlError::ItemAlreadyExists(existing_short_url))
                if to_save == existing_short_url =>
            {
                Ok((
                    existing_short_url
                        .try_into()
                        .context("Failed to convert existing ShortUrl into external format")
                        .map_err(PutUrlError::Internal)?,
                    UrlCreationStatus::AlreadyExists,
                ))
            }
            Err(SaveUrlError::ItemAlreadyExists(_)) => Err(PutUrlError::ShortIdAlreadyTaken),
            Err(SaveUrlError::Internal(internal_err)) => Err(PutUrlError::Internal(internal_err)),
        }
    }

    #[instrument(skip(self))]
    async fn post_url(
        &self,
        url: &str,
        expiration_timestamp: &str,
    ) -> Result<ShortenedUrl, PostUrlError> {
        const PUT_ATTEMPTS: usize = 3;
        const BYTES_TO_TAKE: usize = 5;

        // NOTE: start with zeroed salt so we can hopefully dedupe
        // if the user made the same POST request before
        let mut salt = [0; blake3::KEY_LEN];

        for _ in 0..PUT_ATTEMPTS {
            let hash = blake3::Hasher::new_keyed(&salt)
                .update(url.as_bytes())
                .update(expiration_timestamp.as_bytes())
                .finalize();

            let mut base62_buf = [0; 16];
            base62_buf[..BYTES_TO_TAKE].copy_from_slice(&hash.as_bytes()[..BYTES_TO_TAKE]);
            let attempt_id = base62::encode(u128::from_le_bytes(base62_buf));

            // NOTE: we defer our url creation logic to a PUT request with the attempt_id
            match self
                .put_url(attempt_id.clone(), url, expiration_timestamp)
                .await
            {
                Ok((shortened_url, _)) => return Ok(shortened_url),
                // NOTE: these are unrecoverable errors; early return to prevent retries
                Err(PutUrlError::InvalidUrl(inner)) => {
                    return Err(PostUrlError::InvalidUrl(inner));
                }
                Err(PutUrlError::TimestampParse(inner)) => {
                    return Err(PostUrlError::TimestampParse(inner));
                }
                Err(PutUrlError::InvalidExpirationTime(inner)) => {
                    return Err(PostUrlError::InvalidExpirationTime(inner));
                }
                Err(PutUrlError::Internal(err)) => {
                    error!(?err, "Encountered internal error in delegated PUT call");
                    return Err(PostUrlError::Internal(
                        err.context("Encountered internal error in delegated PUT call"),
                    ));
                }
                // NOTE: these are retryable errors; continue on
                Err(PutUrlError::InvalidShortId(err)) => {
                    // NOTE: this can be caused by:
                    // - A bug, in which we are not generating ShortIds of proper length
                    // - In _very_ rare scenarios when a lot of the trailing hashed bits are 0
                    warn!(?attempt_id, ?err, "Generated invalid ShortId");
                }
                Err(PutUrlError::ShortIdAlreadyTaken) => {
                    warn!(?attempt_id, "Generated ShortId that was already taken");
                }
            }

            rand::rng().fill_bytes(&mut salt);
        }

        Err(PostUrlError::Internal(anyhow!("Exhausted retry attempts")))
    }
}

impl TryFrom<url_repo::ShortUrl> for ShortenedUrl {
    type Error = anyhow::Error;

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
            expiration_timestamp: expiration_time
                .into_inner()
                .format(&Rfc3339)
                .context("Failed to format expiration timestamp")?,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use mockall::{mock, predicate::*};
    use time::Duration;

    use crate::url_repo::ShortUrl;

    use super::*;

    mock! {
        UrlRepository {}

        #[async_trait]
        impl UrlRepository for UrlRepository {
            async fn retrieve_url(&self, id: &str) -> anyhow::Result<Option<url_repo::ShortUrl>>;
            async fn save_url(&self, url: url_repo::ShortUrl) -> Result<url_repo::ShortUrl, SaveUrlError>;
            async fn delete_expired_urls(&self) -> anyhow::Result<()>;
        }
    }

    fn new_short_url(id: &str, url_str: &str, expires_in: Duration) -> url_repo::ShortUrl {
        url_repo::ShortUrl {
            short_id: ShortId::new(id.to_owned()).unwrap(),
            url: Url::parse(url_str).unwrap(),
            expiration_time: ExpirationTime::new(OffsetDateTime::now_utc() + expires_in).unwrap(),
        }
    }

    #[tokio::test]
    async fn test_get_url_success() {
        let mut mock_repo = MockUrlRepository::new();
        let short_id = "testurl123";
        let long_url = "https://example.com/long";
        let expected_short_url = new_short_url("testurl", long_url, Duration::days(1));

        let mock_return_value = Ok(Some(expected_short_url.clone()));
        mock_repo
            .expect_retrieve_url()
            .with(eq(short_id))
            .once()
            .return_once(move |_| mock_return_value);

        let service = UrlRestServiceImpl {
            url_repo: Arc::new(mock_repo),
        };
        let result = service.get_url(short_id).await.unwrap();
        assert_eq!(result.url, long_url);
        assert!(
            // NOTE: slight tolerance is allowed in case of slow tests
            (86395..=86400).contains(&result.max_age_seconds)
        );
    }

    #[tokio::test]
    async fn test_get_url_not_found() {
        let mut mock_repo = MockUrlRepository::new();
        let short_id = "testurl123";

        mock_repo
            .expect_retrieve_url()
            .with(eq(short_id))
            .once()
            .return_once(|_| Ok(None));

        let service = UrlRestServiceImpl {
            url_repo: Arc::new(mock_repo),
        };
        let get_url_err = service.get_url(short_id).await.unwrap_err();
        assert!(matches!(get_url_err, GetUrlError::NotFound));
    }

    #[tokio::test]
    async fn test_get_url_db_error() {
        let mut mock_repo = MockUrlRepository::new();
        let short_id = "testurl123";

        mock_repo
            .expect_retrieve_url()
            .with(eq(short_id))
            .once()
            .return_once(|_| Err(anyhow::anyhow!("test error")));

        let service = UrlRestServiceImpl {
            url_repo: Arc::new(mock_repo),
        };
        let get_url_err = service.get_url(short_id).await.unwrap_err();
        assert!(matches!(get_url_err, GetUrlError::Db(err) if err.to_string() == "test error"));
    }

    #[tokio::test]
    async fn test_put_url_newly_created() {
        let mut mock_repo = MockUrlRepository::new();
        let short_id = "newurl123".to_owned();
        let long_url = "https://example.com";
        let expected_short_url = new_short_url(&short_id, long_url, Duration::days(1));
        let expiration_timestamp_str = expected_short_url
            .expiration_time
            .clone()
            .into_inner()
            .format(&Rfc3339)
            .unwrap();

        mock_repo
            .expect_save_url()
            .with(eq(expected_short_url.clone()))
            .once()
            .return_once({
                let expected_short_url = expected_short_url.clone();
                move |_| Ok(expected_short_url)
            });

        let service = UrlRestServiceImpl {
            url_repo: Arc::new(mock_repo),
        };
        let (shortened_url, status) = service
            .put_url(short_id, long_url, &expiration_timestamp_str)
            .await
            .unwrap();

        assert_eq!(
            shortened_url.shortened_url_id,
            expected_short_url.short_id.into_inner()
        );
        assert_eq!(status, UrlCreationStatus::NewlyCreated);
    }

    #[tokio::test]
    async fn test_put_url_already_exists_same_content() {
        let mut mock_repo = MockUrlRepository::new();
        let short_id = "existurl123".to_owned();
        let long_url = "https://example.com";
        let existing_short_url = new_short_url(&short_id, long_url, Duration::days(1));
        let expiration_timestamp_str = existing_short_url
            .expiration_time
            .clone()
            .into_inner()
            .format(&Rfc3339)
            .unwrap();

        mock_repo
            .expect_save_url()
            .with(eq(existing_short_url.clone()))
            .once()
            .return_once({
                let existing_short_url = existing_short_url.clone();
                move |_| Err(SaveUrlError::ItemAlreadyExists(existing_short_url))
            });

        let service = UrlRestServiceImpl {
            url_repo: Arc::new(mock_repo),
        };
        let (shortened_url, status) = service
            .put_url(short_id, long_url, &expiration_timestamp_str)
            .await
            .unwrap();

        assert_eq!(
            shortened_url.shortened_url_id,
            existing_short_url.short_id.into_inner()
        );
        assert_eq!(status, UrlCreationStatus::AlreadyExists);
    }

    #[tokio::test]
    async fn test_put_url_short_id_already_taken() {
        let mut mock_repo = MockUrlRepository::new();
        let short_id = "takenurl123".to_owned();
        let long_url = "https://example.com";
        let conflicting_short_url =
            new_short_url("anotherurl123", "https://example.com", Duration::days(1));
        let expiration_timestamp_str = conflicting_short_url
            .expiration_time
            .clone()
            .into_inner()
            .format(&Rfc3339)
            .unwrap();

        let expected_short_url = ShortUrl {
            short_id: ShortId::new(short_id.clone()).unwrap(),
            url: Url::parse(long_url).unwrap(),
            expiration_time: conflicting_short_url.expiration_time.clone(),
        };
        mock_repo
            .expect_save_url()
            .with(eq(expected_short_url))
            .once()
            .return_once({
                let conflicting_short_url = conflicting_short_url.clone();
                move |_| Err(SaveUrlError::ItemAlreadyExists(conflicting_short_url))
            });

        let service = UrlRestServiceImpl {
            url_repo: Arc::new(mock_repo),
        };
        let result = service
            .put_url(short_id, long_url, &expiration_timestamp_str)
            .await
            .unwrap_err();

        assert!(matches!(result, PutUrlError::ShortIdAlreadyTaken));
    }

    #[tokio::test]
    async fn test_put_url_invalid_short_id() {
        let mock_repo = MockUrlRepository::new();
        let service = UrlRestServiceImpl {
            url_repo: Arc::new(mock_repo),
        };
        let result = service
            .put_url(
                "invalid_chars".to_owned(),
                "https://example.com",
                "2025-01-01T00:00:00Z",
            )
            .await
            .unwrap_err();
        assert!(matches!(
            result,
            PutUrlError::InvalidShortId(ShortIdValidationError::InvalidCharacters { invalid_chars })
                if invalid_chars == "_"
        ));
    }

    #[tokio::test]
    async fn test_put_url_invalid_long_url() {
        let mock_repo = MockUrlRepository::new();
        let service = UrlRestServiceImpl {
            url_repo: Arc::new(mock_repo),
        };
        let result = service
            .put_url("valid123".to_owned(), "not a url", "1234-01-01T00:00:00Z")
            .await
            .unwrap_err();
        assert!(matches!(result, PutUrlError::InvalidUrl(_)));
    }

    #[tokio::test]
    async fn test_put_url_invalid_timestamp_format() {
        let mock_repo = MockUrlRepository::new();
        let service = UrlRestServiceImpl {
            url_repo: Arc::new(mock_repo),
        };
        let result = service
            .put_url(
                "valid123".to_owned(),
                "https://example.com",
                "invalid-timestamp",
            )
            .await
            .unwrap_err();
        assert!(matches!(result, PutUrlError::TimestampParse(_)));
    }

    #[tokio::test]
    async fn test_put_url_expiration_time_in_past() {
        let mock_repo = MockUrlRepository::new();
        let service = UrlRestServiceImpl {
            url_repo: Arc::new(mock_repo),
        };
        let past_timestamp = (OffsetDateTime::now_utc() - Duration::days(1))
            .format(&Rfc3339)
            .unwrap();
        let result = service
            .put_url(
                "valid123".to_owned(),
                "https://example.com",
                &past_timestamp,
            )
            .await
            .unwrap_err();
        assert!(matches!(
            result,
            PutUrlError::InvalidExpirationTime(ExpirationTimeValidationError::InPast)
        ));
    }

    #[tokio::test]
    async fn test_put_url_db_error() {
        let mut mock_repo = MockUrlRepository::new();
        let short_id = "testurl123".to_owned();
        let long_url = "https://example.com";
        let expected_short_url = new_short_url(&short_id, long_url, Duration::days(1));
        let expiration_timestamp_str = expected_short_url
            .expiration_time
            .clone()
            .into_inner()
            .format(&Rfc3339)
            .unwrap();

        mock_repo
            .expect_save_url()
            .with(eq(expected_short_url))
            .once()
            .return_once(|_| Err(SaveUrlError::Internal(anyhow::anyhow!("test failure"))));

        let service = UrlRestServiceImpl {
            url_repo: Arc::new(mock_repo),
        };
        let result = service
            .put_url(short_id, long_url, &expiration_timestamp_str)
            .await
            .unwrap_err();
        assert!(matches!(result, PutUrlError::Internal(_)));
    }

    #[tokio::test]
    async fn test_post_url_newly_created() {
        let long_url = "https://example.com/";
        let expiration_time = OffsetDateTime::now_utc() + Duration::days(1);
        let expiration_timestamp = expiration_time.format(&Rfc3339).unwrap();

        let mut mock_repo = MockUrlRepository::new();
        mock_repo
            .expect_save_url()
            .withf(move |actual_short_url| {
                actual_short_url.url.as_str() == long_url
                    && actual_short_url.expiration_time.clone().into_inner() == expiration_time
            })
            .once()
            .return_once(Ok);

        let service = UrlRestServiceImpl {
            url_repo: Arc::new(mock_repo),
        };
        let result = service
            .post_url(long_url, &expiration_timestamp)
            .await
            .unwrap();
        assert_eq!(result.long_url, long_url);
        assert_eq!(result.expiration_timestamp, expiration_timestamp);
    }

    #[tokio::test]
    async fn test_post_url_newly_dedupe() {
        let long_url = "https://example.com/";
        let expiration_time = OffsetDateTime::now_utc() + Duration::days(1);
        let expiration_timestamp = expiration_time.format(&Rfc3339).unwrap();

        let mut mock_repo = MockUrlRepository::new();
        mock_repo
            .expect_save_url()
            .withf(move |actual_short_url| {
                actual_short_url.url.as_str() == long_url
                    && actual_short_url.expiration_time.clone().into_inner() == expiration_time
            })
            .once()
            .return_once(|short_url| Err(SaveUrlError::ItemAlreadyExists(short_url)));

        let service = UrlRestServiceImpl {
            url_repo: Arc::new(mock_repo),
        };
        let result = service
            .post_url(long_url, &expiration_timestamp)
            .await
            .unwrap();
        assert_eq!(result.long_url, long_url);
        assert_eq!(result.expiration_timestamp, expiration_timestamp);
    }

    #[tokio::test]
    async fn test_post_url_invalid_long_url() {
        let mock_repo = MockUrlRepository::new();
        let service = UrlRestServiceImpl {
            url_repo: Arc::new(mock_repo),
        };
        let result = service
            .post_url("not a url", "1234-01-01T00:00:00Z")
            .await
            .unwrap_err();
        assert!(matches!(result, PostUrlError::InvalidUrl(_)));
    }

    #[tokio::test]
    async fn test_post_url_invalid_timestamp_format() {
        let mock_repo = MockUrlRepository::new();
        let service = UrlRestServiceImpl {
            url_repo: Arc::new(mock_repo),
        };
        let result = service
            .post_url("https://example.com", "invalid-timestamp")
            .await
            .unwrap_err();
        assert!(matches!(result, PostUrlError::TimestampParse(_)));
    }

    #[tokio::test]
    async fn test_post_url_expiration_time_in_past() {
        let mock_repo = MockUrlRepository::new();
        let service = UrlRestServiceImpl {
            url_repo: Arc::new(mock_repo),
        };
        let past_timestamp = (OffsetDateTime::now_utc() - Duration::days(1))
            .format(&Rfc3339)
            .unwrap();
        let result = service
            .post_url("https://example.com", &past_timestamp)
            .await
            .unwrap_err();
        assert!(matches!(
            result,
            PostUrlError::InvalidExpirationTime(ExpirationTimeValidationError::InPast)
        ));
    }

    #[tokio::test]
    async fn test_post_url_db_error() {
        let long_url = "https://example.com/";
        let expiration_time = OffsetDateTime::now_utc() + Duration::days(1);
        let expiration_timestamp = expiration_time.format(&Rfc3339).unwrap();

        let mut mock_repo = MockUrlRepository::new();
        mock_repo
            .expect_save_url()
            .withf(move |actual_short_url| {
                actual_short_url.url.as_str() == long_url
                    && actual_short_url.expiration_time.clone().into_inner() == expiration_time
            })
            .once()
            .return_once(|_| Err(SaveUrlError::Internal(anyhow::anyhow!("test failure"))));

        let service = UrlRestServiceImpl {
            url_repo: Arc::new(mock_repo),
        };
        let result = service
            .post_url(long_url, &expiration_timestamp)
            .await
            .unwrap_err();
        assert!(matches!(result, PostUrlError::Internal(_)));
    }

    #[test]
    fn test_shortened_url_try_from_short_url() {
        let short_id = "abcDEF12";
        let long_url = "https://example.com/";
        let expiration_time = OffsetDateTime::now_utc() + Duration::days(2);

        let short_url = url_repo::ShortUrl {
            short_id: ShortId::new(short_id.to_owned()).unwrap(),
            url: Url::parse(long_url).unwrap(),
            expiration_time: ExpirationTime::new(expiration_time).unwrap(),
        };

        let shortened_url: ShortenedUrl = short_url.try_into().unwrap();

        assert_eq!(shortened_url.shortened_url_id, short_id);
        assert_eq!(shortened_url.long_url, long_url);
        assert_eq!(
            shortened_url.expiration_timestamp,
            expiration_time.format(&Rfc3339).unwrap()
        );
    }
}
