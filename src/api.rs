use serde::{Deserialize, Serialize};

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

#[derive(Serialize)]
pub struct ShortenedUrl {
    pub shortened_url_id: String,
    pub long_url: String,
    /// Timestamp in ISO-8601 format
    pub expiration_timestamp: String,
}

pub struct Redirect {
    pub url: String,
}

#[derive(Serialize)]
pub struct Error {
    pub error: String,
}
