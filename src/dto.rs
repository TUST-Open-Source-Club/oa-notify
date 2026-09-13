//! 对外 DTO。

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

use crate::entity::{device, notification, preference};

/// 通知 DTO。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationDto {
    /// 通知 ID。
    pub id: String,
    /// 事件类型。
    pub event_type: String,
    /// 标题。
    pub title: String,
    /// 正文摘要。
    pub body: String,
    /// 优先级。
    pub priority: String,
    /// 资源类型。
    pub resource_type: Option<String>,
    /// 资源 ID。
    pub resource_id: Option<String>,
    /// 跳转路径。
    pub url: Option<String>,
    /// 是否已读。
    pub read: bool,
    /// 创建时间。
    pub created_at: DateTime<FixedOffset>,
}

impl From<&notification::Model> for NotificationDto {
    /// 从实体转换。
    fn from(model: &notification::Model) -> Self {
        Self {
            id: model.id.to_string(),
            event_type: model.event_type.clone(),
            title: model.title.clone(),
            body: model.body.clone(),
            priority: model.priority.clone(),
            resource_type: model.resource_type.clone(),
            resource_id: model.resource_id.clone(),
            url: model.url.clone(),
            read: model.read_at.is_some(),
            created_at: model.created_at,
        }
    }
}

/// 偏好 DTO（读写共用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferenceDto {
    /// 静音模块。
    pub muted_modules: Vec<String>,
    /// 免打扰开始（HH:MM）。
    pub quiet_from: Option<String>,
    /// 免打扰结束（HH:MM）。
    pub quiet_to: Option<String>,
    /// 是否启用 ntfy。
    pub ntfy_enabled: bool,
}

impl Default for PreferenceDto {
    /// 默认偏好：全部开启、无静音、无免打扰。
    fn default() -> Self {
        Self {
            muted_modules: Vec::new(),
            quiet_from: None,
            quiet_to: None,
            ntfy_enabled: true,
        }
    }
}

impl From<&preference::Model> for PreferenceDto {
    /// 从实体转换（容忍脏 JSON）。
    fn from(model: &preference::Model) -> Self {
        let muted = match &model.muted_modules {
            serde_json::Value::Array(items) => items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        };
        Self {
            muted_modules: muted,
            quiet_from: model.quiet_from.clone(),
            quiet_to: model.quiet_to.clone(),
            ntfy_enabled: model.ntfy_enabled,
        }
    }
}

/// 设备 DTO。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDto {
    /// 设备记录 ID。
    pub id: String,
    /// 厂商。
    pub vendor: String,
    /// 平台。
    pub platform: String,
    /// 设备名称。
    pub device_name: Option<String>,
    /// 最近上报时间。
    pub last_seen_at: DateTime<FixedOffset>,
}

impl From<&device::Model> for DeviceDto {
    /// 从实体转换。
    fn from(model: &device::Model) -> Self {
        Self {
            id: model.id.to_string(),
            vendor: model.vendor.clone(),
            platform: model.platform.clone(),
            device_name: model.device_name.clone(),
            last_seen_at: model.last_seen_at,
        }
    }
}
