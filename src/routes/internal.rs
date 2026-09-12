//! 内部事件接收：落库站内通知，并按偏好桥接 ntfy。

use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use chrono::FixedOffset;
use serde_json::{json, Value};

use club_auth_sdk::verify_service_token;
use club_common::AppError;

use crate::domain::{should_deliver, topic_for, EventEnvelope, PreferenceView};
use crate::dto::PreferenceDto;
use crate::ntfy::NtfyMessage;
use crate::repo;
use crate::state::{AppState, SharedState};

/// 服务令牌允许的时间偏差（秒）。
const SERVICE_TOKEN_LEEWAY: i64 = 60;
/// 默认本地时区偏移（社团在中国：UTC+8）。
const DEFAULT_TZ_OFFSET_SECONDS: i32 = 8 * 3600;

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

/// `POST /internal/events`：接收业务事件。
///
/// 流程：幂等落库 → 读取偏好（默认全开）→ 判断是否推送 ntfy。
/// 推送失败只记日志，不影响落库结果。
pub async fn ingest_event(
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, AppError> {
    let service = verify_service(&state, &headers, &body)?;
    let event: EventEnvelope = serde_json::from_slice(&body).map_err(|err| {
        AppError::bad_request("NOTIFY_INVALID_JSON", format!("事件不是合法 JSON: {err}"))
    })?;
    if event.target_users.is_empty() {
        return Ok(Json(json!({ "created": 0, "pushed": 0 })));
    }

    let now = state.now();
    let local = now.with_timezone(&FixedOffset::east_opt(DEFAULT_TZ_OFFSET_SECONDS).expect("tz"));
    let local_minutes = (local.hour() * 60 + local.minute()) as u16;

    let mut created = 0;
    let mut pushed = 0;
    for user_id in &event.target_users {
        let is_new = repo::insert_notification_idempotent(
            &state.db,
            repo::NewNotification {
                user_id: *user_id,
                event_id: event.id,
                event_type: event.event_type.clone(),
                title: event.title.clone(),
                body: event.body.clone(),
                priority: event.normalized_priority().to_string(),
                resource_type: event.resource.as_ref().map(|r| r.resource_type.clone()),
                resource_id: event.resource.as_ref().map(|r| r.id.clone()),
                url: event.resource.as_ref().and_then(|r| r.url.clone()),
            },
            now,
        )
        .await?;
        if !is_new {
            continue;
        }
        created += 1;

        // 偏好：无记录视为默认（全部开启）
        let prefs = repo::get_preference(&state.db, *user_id)
            .await?
            .map(|model| PreferenceDto::from(&model))
            .unwrap_or_default();
        let view = PreferenceView {
            muted_modules: prefs.muted_modules,
            quiet_from: prefs.quiet_from,
            quiet_to: prefs.quiet_to,
            ntfy_enabled: prefs.ntfy_enabled,
        };
        if should_deliver(
            &view,
            event.module(),
            Some(local_minutes),
            event.is_urgent(),
        ) {
            let message = NtfyMessage {
                topic: topic_for(*user_id),
                title: event.title.clone(),
                message: event.body.clone(),
                priority: event.normalized_priority().to_string(),
                click: event.resource.as_ref().and_then(|r| r.url.clone()),
            };
            match state.ntfy.publish(&message).await {
                Ok(()) => pushed += 1,
                Err(err) => {
                    tracing::warn!(error = %err, user = %user_id, "ntfy 推送失败（通知已落库）")
                }
            }
        }
    }

    tracing::info!(service = %service, event = %event.event_type, created, pushed, "事件已处理");
    Ok(Json(json!({ "created": created, "pushed": pushed })))
}

use chrono::Timelike;
