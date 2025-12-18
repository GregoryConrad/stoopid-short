use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing,
};
use rearch::Container;
use sea_orm::Database;
use tokio::net::TcpListener;
use tracing::{error, info, instrument};

use stoopid_short::{
    api,
    config::{addr_capsule, db_conn_init_action, db_connection_options_capsule},
    url_service::{GetUrlError, PostUrlError, PutUrlError, url_rest_service_capsule},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // TODO init tracing + more helpful logging throughout
    // initialize tracing/metrics?
    // tracing_subscriber::fmt::init();
    let container = init_container().await?;

    let app = Router::new()
        .route("/", routing::post(post_url))
        .route("/{id}", routing::get(get_url).put(put_url))
        .with_state(container.clone());

    let listener = TcpListener::bind(container.read(addr_capsule)).await?;
    info!(addr = %listener.local_addr()?, "Started listening on TCP");
    axum::serve(listener, app).await?;
    Ok(())
}

#[instrument]
async fn init_container() -> anyhow::Result<Container> {
    let container = Container::new();

    let (db_connection_options, set_db_conn) =
        container.read((db_connection_options_capsule, db_conn_init_action));
    set_db_conn(Database::connect(db_connection_options).await?);

    Ok(container)
}

#[instrument(skip(container))]
async fn get_url(State(container): State<Container>, Path(id): Path<String>) -> impl IntoResponse {
    container
        .read(url_rest_service_capsule)
        .get_url(&id)
        .await
        .map(|api::Redirect { url }| Redirect::temporary(&url))
        .map_err(|error: GetUrlError| match error {
            GetUrlError::NotFound => (
                StatusCode::NOT_FOUND,
                Json(api::Error {
                    error: "Not found".to_owned(),
                }),
            )
                .into_response(),
            GetUrlError::Db(db_err) => {
                error!(?db_err, "Encountered DbErr");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(api::Error {
                        error: "Internal server error: database".to_owned(),
                    }),
                )
                    .into_response()
            }
        })
}

#[instrument(skip(container))]
async fn put_url(
    State(container): State<Container>,
    Path(id): Path<String>,
    Json(api::PutUrlPayload {
        url,
        expiration_timestamp,
    }): Json<api::PutUrlPayload>,
) -> impl IntoResponse {
    // TODO handle the following, but modify UrlRestService to do so.
    // Add something like a new PutUrlError::ExactResourceAlreadyExists,
    // for when curr copy == one in db
    // StatusCode::OK // for idempotnent and already exists
    // StatusCode::CREATED // for newly created
    container
        .read(url_rest_service_capsule)
        .put_url(id, url, &expiration_timestamp)
        .await
        .map(Json)
        .map_err(|error: PutUrlError| match error {
            PutUrlError::TimestampParse(parse_error) => (
                StatusCode::BAD_REQUEST,
                Json(api::Error {
                    error: format!("Timestamp {expiration_timestamp} is invalid: {parse_error}"),
                }),
            ),
            PutUrlError::TimestampFormat(format_error) => {
                error!(
                    ?format_error,
                    "Encountered Format error while formatting timestamp from db"
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(api::Error {
                        error: "Internal server error: format".to_owned(),
                    }),
                )
            }
            PutUrlError::Db(db_err) => {
                error!(?db_err, "Encountered DbErr");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(api::Error {
                        error: "Internal server error: database".to_owned(),
                    }),
                )
            }
        })
}

#[instrument(skip(container))]
async fn post_url(
    State(container): State<Container>,
    Json(api::PostUrlPayload {
        url,
        expiration_timestamp,
    }): Json<api::PostUrlPayload>,
) -> impl IntoResponse {
    container
        .read(url_rest_service_capsule)
        .post_url(url, expiration_timestamp)
        .await
        .map(Json)
        .map_err(|error: PostUrlError| match error {
            PostUrlError::DbError(db_err) => {
                error!(?db_err, "Encountered DbErr");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(api::Error {
                        error: "Internal server error: database".to_owned(),
                    }),
                )
            }
        })
}
