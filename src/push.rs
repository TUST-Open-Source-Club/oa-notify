//! 多通道推送路由：设备厂商通道 → FCM → ntfy（逐级兜底）。

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use club_common::AppError;

use crate::entity::device;
use crate::ntfy::{NtfyMessage, NtfyPublisher};
use crate::repo;

/// 支持登记的厂商（与需求 13.7 对齐）。
pub const VENDORS: &[&str] = &[
    "apple", "huawei", "honor", "xiaomi", "oppo", "vivo", "meizu", "fcm",
];

/// 待推送消息（仅摘要与深链）。
#[derive(Debug, Clone)]
pub struct PushMessage {
    /// 标题。
    pub title: String,
    /// 摘要。
    pub body: String,
    /// 点击深链。
    pub url: Option<String>,
    /// 优先级：urgent/high/default/low。
    pub priority: String,
}

/// 单一推送通道。
#[async_trait]
pub trait PushChannel: Send + Sync {
    /// 通道标识（厂商名或 fcm）。
    fn name(&self) -> &'static str;

    /// 发送一条消息。
    async fn send(&self, device: &device::Model, message: &PushMessage) -> Result<(), AppError>;
}

/// 日志通道：开发模式/未配置厂商凭据时使用（生产替换为厂商 HTTP 实现）。
#[derive(Debug, Clone)]
pub struct LogChannel {
    /// 通道名。
    pub channel: &'static str,
}

#[async_trait]
impl PushChannel for LogChannel {
    fn name(&self) -> &'static str {
        self.channel
    }

    async fn send(
        &self,
        device_row: &device::Model,
        message: &PushMessage,
    ) -> Result<(), AppError> {
        tracing::info!(
            channel = self.channel,
            token = %device_row.token,
            title = %message.title,
            "推送（开发模式，未实际调用厂商接口）"
        );
        Ok(())
    }
}

/// 投递结果统计。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PushOutcome {
    /// 厂商通道成功数。
    pub vendor: u32,
    /// FCM 成功数。
    pub fcm: u32,
    /// 是否走了 ntfy 兜底。
    pub ntfy: bool,
}

impl PushOutcome {
    /// 是否有任一通道成功。
    pub fn delivered(&self) -> bool {
        self.vendor + self.fcm > 0 || self.ntfy
    }
}

/// 推送路由器。
pub struct PushRouter {
    /// 厂商通道表（vendor → 通道实现）。
    pub vendors: HashMap<String, Arc<dyn PushChannel>>,
    /// FCM 兜底通道（可选）。
    pub fcm: Option<Arc<dyn PushChannel>>,
    /// ntfy 最终兜底。
    pub ntfy: Arc<dyn NtfyPublisher>,
}

impl PushRouter {
    /// 按设备逐级投递；所有设备均失败（或没有设备）时落到 ntfy。
    pub async fn dispatch(
        &self,
        db: &sea_orm::DatabaseConnection,
        user_id: uuid::Uuid,
        message: &PushMessage,
    ) -> Result<PushOutcome, AppError> {
        let devices = repo::list_devices(db, user_id).await?;
        let mut outcome = PushOutcome::default();
        for device_row in &devices {
            if let Some(channel) = self.vendors.get(&device_row.vendor) {
                match channel.send(device_row, message).await {
                    Ok(()) => {
                        outcome.vendor += 1;
                        continue;
                    }
                    Err(err) => {
                        tracing::warn!(vendor = %device_row.vendor, error = %err, "厂商通道失败，尝试降级");
                    }
                }
            }
            if let Some(fcm) = &self.fcm {
                match fcm.send(device_row, message).await {
                    Ok(()) => {
                        outcome.fcm += 1;
                        continue;
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, "FCM 失败，尝试降级 ntfy");
                    }
                }
            }
        }
        if outcome.vendor + outcome.fcm == 0 {
            let ntfy_message = NtfyMessage {
                topic: crate::domain::topic_for(user_id),
                title: message.title.clone(),
                message: message.body.clone(),
                priority: message.priority.clone(),
                click: message.url.clone(),
            };
            match self.ntfy.publish(&ntfy_message).await {
                Ok(()) => outcome.ntfy = true,
                Err(err) => tracing::warn!(error = %err, "ntfy 兜底失败"),
            }
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ntfy::LogNtfyPublisher;
    use chrono::Utc;
    use std::sync::Mutex;
    use uuid::Uuid;

    /// 记录调用的测试通道。
    struct RecordingChannel {
        name: &'static str,
        fail: bool,
        calls: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl PushChannel for RecordingChannel {
        fn name(&self) -> &'static str {
            self.name
        }

        async fn send(
            &self,
            _device: &device::Model,
            _message: &PushMessage,
        ) -> Result<(), AppError> {
            self.calls.lock().expect("lock").push(self.name.to_string());
            if self.fail {
                Err(AppError::internal("channel down"))
            } else {
                Ok(())
            }
        }
    }

    fn device_row(vendor: &str) -> device::Model {
        let now = Utc::now().fixed_offset();
        device::Model {
            id: Uuid::now_v7(),
            user_id: Uuid::now_v7(),
            vendor: vendor.to_string(),
            platform: "android".to_string(),
            token: "token-1".to_string(),
            device_name: None,
            last_seen_at: now,
            created_at: now,
            revoked_at: None,
        }
    }

    #[test]
    fn vendor_priority_order_is_declared() {
        assert_eq!(VENDORS[0], "apple");
        assert!(VENDORS.contains(&"huawei"));
        assert!(VENDORS.contains(&"fcm"));
    }

    #[test]
    fn outcome_delivery_semantics() {
        assert!(!PushOutcome::default().delivered());
        assert!(PushOutcome {
            vendor: 1,
            ..Default::default()
        }
        .delivered());
        assert!(PushOutcome {
            fcm: 1,
            ..Default::default()
        }
        .delivered());
        assert!(PushOutcome {
            ntfy: true,
            ..Default::default()
        }
        .delivered());
    }

    #[test]
    fn recording_channel_definition() {
        let channel = RecordingChannel {
            name: "huawei",
            fail: false,
            calls: Mutex::new(vec![]),
        };
        assert_eq!(channel.name(), "huawei");
        assert!(channel.calls.lock().expect("lock").is_empty());
        let _ = device_row("huawei");
        let _ = LogNtfyPublisher;
    }
}
