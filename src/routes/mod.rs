//! HTTP 路由装配与公共辅助。

use axum::http::HeaderMap;
use axum::routing::{delete, get, post};
use axum::Router;

use crate::state::SharedState;

pub mod devices;
pub mod health;
pub mod internal;
pub mod notifications;
pub mod preferences;

/// 从请求头提取客户端信息：(IP, User-Agent)。
pub fn client_info(headers: &HeaderMap) -> (Option<String>, Option<String>) {
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(|value| value.trim().to_string());
    let user_agent = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    (ip, user_agent)
}

/// `/api/v1/notify` 下的路由。
pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/notifications", get(notifications::list_notifications))
        .route(
            "/notifications/unread-count",
            get(notifications::unread_count),
        )
        .route("/notifications/read-all", post(notifications::read_all))
        .route("/notifications/{id}/read", post(notifications::mark_read))
        .route(
            "/preferences",
            get(preferences::get_preferences).put(preferences::put_preferences),
        )
        .route(
            "/devices",
            get(devices::list_devices).post(devices::register_device),
        )
        .route("/devices/{token}", delete(devices::revoke_device))
        // 内部服务接口（服务令牌保护）
        .route("/internal/events", post(internal::ingest_event))
}
