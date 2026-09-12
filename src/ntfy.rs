//! ntfy 发布抽象：生产走 HTTP，开发/测试使用日志或替身实现。

use async_trait::async_trait;
use serde_json::json;

use club_common::AppError;

use crate::domain::ntfy_priority;

/// ntfy 消息。
#[derive(Debug, Clone)]
pub struct NtfyMessage {
    /// 主题（用户级）。
    pub topic: String,
    /// 标题。
    pub title: String,
    /// 摘要正文（不含敏感内容）。
    pub message: String,
    /// 优先级（urgent/high/default/low）。
    pub priority: String,
    /// 点击跳转。
    pub click: Option<String>,
}

/// 发布端口。
#[async_trait]
pub trait NtfyPublisher: Send + Sync {
    /// 发布一条消息；失败返回 500（调用方记录并忽略，不阻断通知落库）。
    async fn publish(&self, message: &NtfyMessage) -> Result<(), AppError>;
}

/// 日志发布器（开发模式）。
#[derive(Debug, Default, Clone)]
pub struct LogNtfyPublisher;

#[async_trait]
impl NtfyPublisher for LogNtfyPublisher {
    async fn publish(&self, message: &NtfyMessage) -> Result<(), AppError> {
        tracing::info!(
            topic = %message.topic,
            title = %message.title,
            "ntfy 推送（开发模式，未实际发送）"
        );
        Ok(())
    }
}

/// HTTP 发布器：调用 ntfy JSON 发布接口。
#[derive(Debug, Clone)]
pub struct HttpNtfyPublisher {
    base_url: String,
    token: String,
    client: reqwest::Client,
}

impl HttpNtfyPublisher {
    /// 创建发布器。
    pub fn new(base_url: &str, token: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl NtfyPublisher for HttpNtfyPublisher {
    async fn publish(&self, message: &NtfyMessage) -> Result<(), AppError> {
        let mut request = self
            .client
            .post(format!("{}/", self.base_url))
            .json(&json!({
                "topic": message.topic,
                "title": message.title,
                "message": message.message,
                "priority": ntfy_priority(&message.priority),
                "click": message.click,
            }));
        if !self.token.is_empty() {
            request = request.bearer_auth(&self.token);
        }
        request
            .send()
            .await
            .map_err(AppError::internal)?
            .error_for_status()
            .map(|_| ())
            .map_err(AppError::internal)
    }
}
