//! 社团 OA 通知服务（notify）。
//!
//! 职责：消费业务事件 → 落库站内通知 → 按用户偏好桥接 ntfy（iOS 走 APNs，
//! Android 走应用内常驻服务/官方 ntfy App）。
//!
//! 当前进度（M2）：配置、领域逻辑、ntfy 发布抽象、实体与迁移已完成；
//! 数据访问层、HTTP 路由与事件消费者在下一步接入。

#![warn(missing_docs)]

pub mod config;
pub mod domain;
/// 对外 DTO。
pub mod dto;
pub mod entity;
pub mod migration;
pub mod ntfy;
/// 数据访问层。
pub mod repo;
/// 应用状态。
pub mod state;
