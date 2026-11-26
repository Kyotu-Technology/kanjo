use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::request::Parts;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use kanjo_core::api::{ApiError, Query};
use serde::de::DeserializeOwned;

/// Largest `limit` a list endpoint accepts; larger values are clamped.
pub(crate) const MAX_LIMIT: i64 = 1000;
const MAX_OFFSET: i64 = 10_000_000;
const MAX_TEXT_PARAM: usize = 200;

/// Every non-2xx answer of the API: a status plus an `ApiError` body.
#[derive(Debug)]
pub(crate) struct ApiErr {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub retry_after: Option<u64>,
}

impl ApiErr {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> ApiErr {
        ApiErr { status, code, message: message.into(), retry_after: None }
    }
    pub fn unauthorized(message: impl Into<String>) -> ApiErr {
        ApiErr::new(StatusCode::UNAUTHORIZED, "unauthorized", message)
    }
    pub fn forbidden(message: impl Into<String>) -> ApiErr {
        ApiErr::new(StatusCode::FORBIDDEN, "forbidden", message)
    }
    pub fn disabled(message: impl Into<String>) -> ApiErr {
        ApiErr::new(StatusCode::FORBIDDEN, "disabled", message)
    }
    pub fn not_found(message: impl Into<String>) -> ApiErr {
        ApiErr::new(StatusCode::NOT_FOUND, "not_found", message)
    }
    pub fn invalid(message: impl Into<String>) -> ApiErr {
        ApiErr::new(StatusCode::BAD_REQUEST, "invalid", message)
    }
    pub fn internal() -> ApiErr {
        ApiErr::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", "internal error; see the server log")
    }

    /// The error a bare status stands for, used when a response left the
    /// router without a JSON body (axum's own 405, 413 and similar).
    pub fn from_status(status: StatusCode) -> ApiErr {
        let reason = status.canonical_reason().unwrap_or("error").to_lowercase();
        let code = match status {
            StatusCode::UNAUTHORIZED | StatusCode::TOO_MANY_REQUESTS => "unauthorized",
            StatusCode::FORBIDDEN => "forbidden",
            StatusCode::NOT_FOUND => "not_found",
            s if s.is_client_error() => "invalid",
            _ => "internal",
        };
        ApiErr::new(status, code, reason)
    }
}

impl From<kanjo_core::Error> for ApiErr {
    fn from(e: kanjo_core::Error) -> ApiErr {
        use kanjo_core::Error;
        match e {
            Error::Invalid(m) => ApiErr::invalid(m),
            Error::NotFound(m) => ApiErr::not_found(m),
            other => {
                tracing::error!(error = %other, "request failed");
                ApiErr::internal()
            }
        }
    }
}

impl IntoResponse for ApiErr {
    fn into_response(self) -> Response {
        let body = ApiError { code: self.code.to_string(), message: self.message };
        let mut res = (self.status, Json(body)).into_response();
        if let Some(secs) = self.retry_after {
            if let Ok(v) = HeaderValue::from_str(&secs.to_string()) {
                res.headers_mut().insert(header::RETRY_AFTER, v);
            }
        }
        res
    }
}

/// `Json` whose rejections are `ApiError` bodies.
pub(crate) struct ApiJson<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for ApiJson<T> {
    type Rejection = ApiErr;

    async fn from_request(req: Request, state: &S) -> Result<Self, ApiErr> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(v)) => Ok(ApiJson(v)),
            Err(rej) => Err(json_rejection(rej)),
        }
    }
}

fn json_rejection(rej: JsonRejection) -> ApiErr {
    let status = rej.status();
    if status == StatusCode::PAYLOAD_TOO_LARGE {
        return ApiErr::new(status, "invalid", "request body too large");
    }
    ApiErr::new(status, "invalid", rej.body_text())
}

/// `Path` whose rejections are `ApiError` bodies.
pub(crate) struct ApiPath<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned + Send> FromRequestParts<S> for ApiPath<T> {
    type Rejection = ApiErr;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, ApiErr> {
        match axum::extract::Path::<T>::from_request_parts(parts, state).await {
            Ok(axum::extract::Path(v)) => Ok(ApiPath(v)),
            Err(rej) => Err(path_rejection(rej)),
        }
    }
}

fn path_rejection(rej: PathRejection) -> ApiErr {
    ApiErr::new(StatusCode::BAD_REQUEST, "invalid", rej.body_text())
}

/// The shared `Query` of the read endpoints, with bounded values.
pub(crate) struct ApiQuery(pub Query);

impl<S: Send + Sync> FromRequestParts<S> for ApiQuery {
    type Rejection = ApiErr;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, ApiErr> {
        match axum::extract::Query::<Query>::from_request_parts(parts, state).await {
            Ok(axum::extract::Query(q)) => bounded(q).map(ApiQuery),
            Err(rej) => Err(query_rejection(rej)),
        }
    }
}

fn query_rejection(rej: QueryRejection) -> ApiErr {
    ApiErr::new(StatusCode::BAD_REQUEST, "invalid", rej.body_text())
}

pub(crate) fn bounded(mut q: Query) -> Result<Query, ApiErr> {
    q.limit = q.limit.map(|l| l.clamp(1, MAX_LIMIT));
    q.offset = q.offset.map(|o| o.clamp(0, MAX_OFFSET));
    if let (Some(from), Some(to)) = (q.from, q.to) {
        if from > to {
            return Err(ApiErr::invalid("from must not be after to"));
        }
    }
    for text in [&q.model, &q.agent, &q.client, &q.q, &q.sort, &q.period, &q.lang, &q.format].into_iter().flatten() {
        if text.chars().count() > MAX_TEXT_PARAM {
            return Err(ApiErr::invalid(format!("query parameters are limited to {MAX_TEXT_PARAM} characters")));
        }
    }
    Ok(q)
}
