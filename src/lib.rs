//! 社团 OA 通知服务（notify）。
//!
//! 职责：消费业务事件 → 落库站内通知 → 按用户偏好桥接 ntfy（iOS 走 APNs，
//! Android 走应用内常驻服务/官方 ntfy App）。
//!
//! 当前进度（M2）：配置、领域逻辑、ntfy 发布抽象、实体与迁移已完成；
//! 数据访问层、HTTP 路由与事件消费者在下一步接入。

#![warn(missing_docs)]

pub mod config;
/// Redis Streams 事件消费者。
pub mod consumer;
/// 数据库连接辅助。
pub mod db;
pub mod domain;
/// 对外 DTO。
pub mod dto;
pub mod entity;
/// 事件处理核心（HTTP 与总线共用）。
pub mod ingest;
pub mod migration;
pub mod ntfy;
/// 数据访问层。
pub mod repo;
/// HTTP 路由。
pub mod routes;
/// 应用状态。
pub mod state;

use axum::Router;
use tower_http::trace::TraceLayer;

use crate::state::SharedState;

/// 构建完整的 HTTP 路由（/healthz、/readyz 与 /api/v1/notify/*）。
pub fn build_router(state: SharedState) -> Router {
    Router::new()
        .route("/healthz", axum::routing::get(routes::health::healthz))
        .route("/readyz", axum::routing::get(routes::health::readyz))
        .nest("/api/v1/notify", routes::router())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
