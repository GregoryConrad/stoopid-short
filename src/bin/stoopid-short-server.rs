use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing,
};
use rearch::Container;
use serde::Serialize;
use stoopid_short::{
    config,
    url_service::{self, GetUrlError, PostUrlError, PutUrlError, url_rest_service_capsule},
};
use tokio::net::TcpListener;
use tracing::{error, info, instrument};
use uuid::Uuid;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let container = config::init_container().await?;

    let app = Router::new()
        .route("/", routing::post(post_url))
        .route("/{id}", routing::get(get_url).put(put_url))
        .with_state(container.clone());

    let listener = TcpListener::bind(container.read(config::addr_capsule)).await?;
    info!(addr = %listener.local_addr()?, "Started listening on TCP");
    axum::serve(listener, app).await?;
    Ok(())
}

#[instrument(skip(container))]
async fn get_url(State(container): State<Container>, Path(id): Path<String>) -> impl IntoResponse {
    container
        .read(url_rest_service_capsule)
        .get_url(&id)
        .await
        .map(|url_service::Redirect { url }| Redirect::temporary(&url))
        .map_err(|error: GetUrlError| {
            let err_uuid = Uuid::new_v4();
            match error {
                GetUrlError::NotFound => (
                    StatusCode::NOT_FOUND,
                    Json(Error {
                        error: "Not found".to_owned(),
                        error_id: err_uuid.to_string(),
                    }),
                ),
                GetUrlError::Db(db_err) => {
                    error!(?db_err, "Encountered database error");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(Error {
                            error: "Internal server error".to_owned(),
                            error_id: err_uuid.to_string(),
                        }),
                    )
                }
            }
        })
}

#[instrument(skip(container))]
async fn put_url(
    State(container): State<Container>,
    Path(id): Path<String>,
    Json(url_service::PutUrlPayload {
        url,
        expiration_timestamp,
    }): Json<url_service::PutUrlPayload>,
) -> impl IntoResponse {
    container
        .read(url_rest_service_capsule)
        .put_url(id, &url, &expiration_timestamp)
        .await
        .map(|(short_url, creation_status)| {
            (
                match creation_status {
                    url_service::UrlCreationStatus::NewlyCreated => StatusCode::CREATED,
                    url_service::UrlCreationStatus::AlreadyExists => StatusCode::OK,
                },
                Json(short_url),
            )
        })
        .map_err(|error: PutUrlError| {
            let err_uuid = Uuid::new_v4();
            match error {
                PutUrlError::ShortIdAlreadyTaken => {
                    info!(?err_uuid, ?error, "Short ID exists under a different entry");
                    (
                        StatusCode::CONFLICT,
                        Json(Error {
                            error: error.to_string(),
                            error_id: err_uuid.to_string(),
                        }),
                    )
                }
                PutUrlError::TimestampParse(_)
                | PutUrlError::InvalidExpirationTime(_)
                | PutUrlError::InvalidShortId(_)
                | PutUrlError::InvalidUrl(_) => {
                    info!(?err_uuid, ?error, "User submitted a bad request");
                    (
                        StatusCode::BAD_REQUEST,
                        Json(Error {
                            error: error.to_string(),
                            error_id: err_uuid.to_string(),
                        }),
                    )
                }
                PutUrlError::TimestampFormat(_) | PutUrlError::Internal(_) => {
                    error!(?err_uuid, ?error, "Encountered an error during a request");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(Error {
                            error: "Internal server error".to_owned(),
                            error_id: err_uuid.to_string(),
                        }),
                    )
                }
            }
        })
}

#[instrument(skip(container))]
async fn post_url(
    State(container): State<Container>,
    Json(url_service::PostUrlPayload {
        url,
        expiration_timestamp,
    }): Json<url_service::PostUrlPayload>,
) -> impl IntoResponse {
    container
        .read(url_rest_service_capsule)
        .post_url(&url, &expiration_timestamp)
        .await
        .map(Json)
        .map_err(|error: PostUrlError| {
            let err_uuid = Uuid::new_v4();
            match error {
                PostUrlError::Db(_) => {
                    error!(?err_uuid, ?error, "Encountered error during a request");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(Error {
                            error: "Internal server error".to_owned(),
                            error_id: err_uuid.to_string(),
                        }),
                    )
                }
            }
        })
}

#[derive(Serialize)]
pub struct Error {
    error: String,
    error_id: String,
}
