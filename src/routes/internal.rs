//! 内部事件接收：服务令牌鉴权后交给 [`crate::ingest::apply_event`]。

use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde_json::{json, Value};

use club_auth_sdk::verify_service_token;
use club_common::AppError;

use crate::domain::EventEnvelope;
use crate::ingest::apply_event;
use crate::state::{AppState, SharedState};

/// 服务令牌允许的时间偏差（秒）。
const SERVICE_TOKEN_LEEWAY: i64 = 60;

/// 校验服务令牌。
fn verify_service(state: &AppState, headers: &HeaderMap, body: &[u8]) -> Result<String, AppError> {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
    };
    let service = header("x-service-name")
        .ok_or_else(|| AppError::unauthorized("AUTH_SERVICE_TOKEN_MISSING", "缺少服务令牌"))?;
    let timestamp: i64 = header("x-service-timestamp")
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| AppError::unauthorized("AUTH_SERVICE_TOKEN_MISSING", "缺少服务令牌"))?;
    let signature = header("x-service-signature")
        .ok_or_else(|| AppError::unauthorized("AUTH_SERVICE_TOKEN_MISSING", "缺少服务令牌"))?;
    verify_service_token(
        &service,
        state.config.internal_secret.as_bytes(),
        timestamp,
        body,
        &signature,
        state.now().timestamp(),
        SERVICE_TOKEN_LEEWAY,
    )
    .map_err(|_| AppError::unauthorized("AUTH_SERVICE_TOKEN_INVALID", "服务令牌无效"))?;
    Ok(service)
}

/// `POST /internal/events`：接收业务事件（HTTP 通道；总线通道见消费者）。
pub async fn ingest_event(
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, AppError> {
    let service = verify_service(&state, &headers, &body)?;
    let event: EventEnvelope = serde_json::from_slice(&body).map_err(|err| {
        AppError::bad_request("NOTIFY_INVALID_JSON", format!("事件不是合法 JSON: {err}"))
    })?;
    let (created, pushed) = apply_event(&state, &event).await?;
    tracing::info!(service = %service, event = %event.event_type, created, pushed, "事件已处理");
    Ok(Json(json!({ "created": created, "pushed": pushed })))
}
