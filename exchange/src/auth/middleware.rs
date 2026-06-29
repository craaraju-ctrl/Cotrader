use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use serde_json::json;
use dashmap::DashMap;

use super::ApiKeyPair;

/// Extract the auth headers from a request and verify the HMAC signature.
/// Returns the user_id on success.
pub fn verify_auth_request(
    method: &str,
    path: &str,
    headers: &HeaderMap,
    api_keys: &DashMap<String, ApiKeyPair>,
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    // Read required headers
    let api_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Missing X-API-Key header"})),
            )
        })?;

    let signature = headers
        .get("x-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Missing X-Signature header"})),
            )
        })?;

    let nonce = headers
        .get("x-nonce")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Missing X-Nonce header"})),
            )
        })?;

    // Validate nonce is a timestamp within 5 minutes
    let timestamp_ms: i64 = nonce.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "X-Nonce must be a millisecond timestamp"})),
        )
    })?;

    if !super::validate_timestamp(timestamp_ms) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Nonce timestamp expired or invalid (max 5 minutes)"})),
        ));
    }

    // Look up the API key
    let entry = api_keys.get(api_key).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Invalid API key"})),
        )
    })?;

    let pair = entry.value();
    let user_id = pair.user_id.clone();
    let secret_key = pair.secret_key.clone();
    drop(entry);

    // Reconstruct the message: METHOD + PATH + NONCE
    let message = format!("{}{}{}", method, path, nonce);

    // Verify the HMAC signature
    if !super::verify_signature(&secret_key, &message, signature) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Invalid signature"})),
        ));
    }

    Ok(user_id)
}

/// Axum middleware that enforces HMAC auth on sensitive endpoints.
/// Reads X-API-Key, X-Signature, X-Nonce headers. On success passes the request
/// through with an x-authenticated-user header added. On failure returns 401.
pub async fn auth_middleware(
    State(app_state): State<super::super::api::AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    let method = req.method().to_string();
    let path = req.uri().path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or(req.uri().path())
        .to_string();
    let headers = req.headers().clone();

    match verify_auth_request(&method, &path, &headers, &app_state.api_keys) {
        Ok(user_id) => {
            // Add the authenticated user to request extensions
            req.extensions_mut().insert(AuthUser { user_id });
            next.run(req).await
        }
        Err((status, body)) => {
            (status, body).into_response()
        }
    }
}

/// Authenticated user info extracted by the middleware
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
}

/// Extract the authenticated user from request extensions (set by the middleware).
/// Returns 401 if not authenticated.
pub fn get_auth_user(req: &axum::http::Request<Body>) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    req.extensions()
        .get::<AuthUser>()
        .map(|u| u.user_id.clone())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Not authenticated"})),
            )
        })
}

