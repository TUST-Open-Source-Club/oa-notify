//! 事件处理核心：幂等落库 → 偏好过滤 → ntfy 推送。
//!
//! 同时被内部 HTTP 接口与 Redis Streams 消费者复用，保证行为一致。

use chrono::{FixedOffset, Timelike};

use club_common::AppError;

use crate::domain::{should_deliver, topic_for, EventEnvelope, PreferenceView};
use crate::dto::PreferenceDto;
use crate::ntfy::NtfyMessage;
use crate::repo;
use crate::state::AppState;

/// 默认本地时区偏移（社团在中国：UTC+8）。
pub const DEFAULT_TZ_OFFSET_SECONDS: i32 = 8 * 3600;

/// 处理一条事件，返回（新建通知数, ntfy 推送数）。
///
/// - 幂等：同 `(user, event_id)` 只落库一次；
/// - 推送失败只记日志，不影响站内通知；
/// - 空接收人直接返回 (0, 0)。
pub async fn apply_event(state: &AppState, event: &EventEnvelope) -> Result<(u32, u32), AppError> {
    if event.target_users.is_empty() {
        return Ok((0, 0));
    }

    let now = state.now();
    let local = now.with_timezone(&FixedOffset::east_opt(DEFAULT_TZ_OFFSET_SECONDS).expect("tz"));
    let local_minutes = (local.hour() * 60 + local.minute()) as u16;

    let mut created = 0u32;
    let mut pushed = 0u32;
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
    Ok((created, pushed))
}
