use std::env::{self, VarError};

use rearch::{CData, CapsuleHandle, Container};
use sea_orm::{ConnectOptions, Database, DbConn};
use tracing::{info, instrument, warn};

/// # Errors
/// Will return [`Err`] if the connection to the database fails.
#[instrument]
pub async fn init_container() -> anyhow::Result<Container> {
    info!("Initializing container");
    let container = Container::new();

    let (db_connection_options, set_db_conn) =
        container.read((db_connection_options_capsule, db_conn_init_action));

    info!(?db_connection_options, "Connecting to database");
    set_db_conn(Database::connect(db_connection_options).await?);

    info!("Container initialized");
    Ok(container)
}

/// # Panics
/// Panics when environment variable is not set or is invalid.
#[must_use]
pub fn db_connection_options_capsule(_: CapsuleHandle) -> ConnectOptions {
    const ENV_VAR_NAME: &str = "DB_URL";
    env::var(ENV_VAR_NAME)
        .unwrap_or_else(|err| match err {
            VarError::NotPresent => panic!("{ENV_VAR_NAME} is not set"),
            VarError::NotUnicode(actual) => {
                panic!("{ENV_VAR_NAME} is invalid unicode: {}", actual.display());
            }
        })
        .into()
}

fn db_conn_manager(
    CapsuleHandle { register, .. }: CapsuleHandle,
) -> (Option<DbConn>, impl use<> + CData + Fn(Option<DbConn>)) {
    register.register(rearch_effects::state::<rearch_effects::Cloned<_>>(None))
}

pub fn db_conn_init_action(
    CapsuleHandle { mut get, .. }: CapsuleHandle,
) -> impl use<> + CData + Fn(DbConn) {
    let set_db_conn = get.as_ref(db_conn_manager).1.clone();
    move |db| set_db_conn(Some(db))
}

/// # Panics
/// Panics when the [`DbConn`] was not set via [`db_conn_init_action`].
pub fn db_conn_capsule(CapsuleHandle { mut get, .. }: CapsuleHandle) -> DbConn {
    let db_conn = get.as_ref(db_conn_manager).0.clone();
    db_conn.expect("DbConn should've been set via db_conn_init_action!")
}

/// # Panics
/// Panics when environment variable is invalid.
pub fn addr_capsule(_: CapsuleHandle) -> String {
    const ENV_VAR_NAME: &str = "ADDR";
    const DEFAULT_ADDR: &str = "127.0.0.1:0";

    match env::var(ENV_VAR_NAME) {
        Ok(addr) => {
            info!(addr, "{ENV_VAR_NAME} environment variable set");
            addr
        }
        Err(VarError::NotPresent) => {
            warn!(
                addr = DEFAULT_ADDR,
                "{ENV_VAR_NAME} environment variable not set; defaulting to {DEFAULT_ADDR}"
            );
            DEFAULT_ADDR.to_string()
        }
        Err(VarError::NotUnicode(actual)) => {
            panic!(
                "{ENV_VAR_NAME} environment variable is invalid: {}",
                actual.display()
            );
        }
    }
}
