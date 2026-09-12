//! 领域逻辑（纯函数）：事件信封、优先级、偏好过滤、ntfy 主题。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 事件资源引用。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EventResource {
    /// 资源类型。
    #[serde(rename = "type")]
    pub resource_type: String,
    /// 资源 ID。
    pub id: String,
    /// 点击跳转路径。
    #[serde(default)]
    pub url: Option<String>,
}

/// 跨服务事件信封（与需求文档 5.4 对齐）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventEnvelope {
    /// 事件 ID。
    pub id: Uuid,
    /// 事件类型，如 `task.assigned`。
    #[serde(rename = "type")]
    pub event_type: String,
    /// 触发者。
    #[serde(default)]
    pub actor_id: Option<Uuid>,
    /// 接收用户列表。
    #[serde(default)]
    pub target_users: Vec<Uuid>,
    /// 关联资源。
    #[serde(default)]
    pub resource: Option<EventResource>,
    /// 标题。
    pub title: String,
    /// 正文摘要（不含敏感内容）。
    #[serde(default)]
    pub body: String,
    /// 优先级：urgent / high / default / low。
    #[serde(default)]
    pub priority: Option<String>,
    /// 去重键（同一业务对象的多次通知折叠）。
    #[serde(default)]
    pub dedup_key: Option<String>,
}

impl EventEnvelope {
    /// 归一化优先级（默认 default）。
    pub fn normalized_priority(&self) -> &str {
        match self.priority.as_deref() {
            Some("urgent") => "urgent",
            Some("high") => "high",
            Some("low") => "low",
            _ => "default",
        }
    }

    /// 是否为紧急（免打扰时段仍然推送）。
    pub fn is_urgent(&self) -> bool {
        self.normalized_priority() == "urgent"
    }

    /// 从事件类型提取模块名（`task.assigned` → `task`）。
    pub fn module(&self) -> &str {
        self.event_type.split('.').next().unwrap_or("system")
    }
}

/// 映射到 ntfy 优先级（1 ~ 5）。
pub fn ntfy_priority(priority: &str) -> u8 {
    match priority {
        "urgent" => 5,
        "high" => 4,
        "low" => 2,
        _ => 3,
    }
}

/// 偏好过滤所需的最小视图。
#[derive(Debug, Clone)]
pub struct PreferenceView {
    /// 静音模块。
    pub muted_modules: Vec<String>,
    /// 免打扰开始（HH:MM）。
    pub quiet_from: Option<String>,
    /// 免打扰结束（HH:MM）。
    pub quiet_to: Option<String>,
    /// 是否启用 ntfy。
    pub ntfy_enabled: bool,
}

/// 判断是否应推送 ntfy：模块未静音、功能开启、且不在免打扰时段（紧急除外）。
pub fn should_deliver(
    prefs: &PreferenceView,
    module: &str,
    local_minutes: Option<u16>,
    is_urgent: bool,
) -> bool {
    if prefs.muted_modules.iter().any(|m| m == module) {
        return false;
    }
    if !prefs.ntfy_enabled {
        return false;
    }
    if is_urgent {
        return true;
    }
    if let (Some(from), Some(to), Some(now)) = (
        prefs.quiet_from.as_deref().and_then(parse_hhmm),
        prefs.quiet_to.as_deref().and_then(parse_hhmm),
        local_minutes,
    ) {
        if in_quiet_window(from, to, now) {
            return false;
        }
    }
    true
}

/// 解析 `HH:MM` 为当日分钟数。
pub fn parse_hhmm(value: &str) -> Option<u16> {
    let (hour, minute) = value.split_once(':')?;
    let hour: u16 = hour.parse().ok()?;
    let minute: u16 = minute.parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(hour * 60 + minute)
}

/// 判断当前分钟是否落在免打扰窗口内（支持跨午夜，如 22:00 ~ 07:00）。
pub fn in_quiet_window(from: u16, to: u16, now: u16) -> bool {
    if from == to {
        return false;
    }
    if from < to {
        now >= from && now < to
    } else {
        now >= from || now < to
    }
}

/// 用户 ntfy 主题（不可猜测）。
pub fn topic_for(user_id: Uuid) -> String {
    format!("u_{}", user_id.simple())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(event_type: &str, priority: Option<&str>) -> EventEnvelope {
        EventEnvelope {
            id: Uuid::nil(),
            event_type: event_type.into(),
            actor_id: None,
            target_users: vec![Uuid::nil()],
            resource: None,
            title: "t".into(),
            body: "b".into(),
            priority: priority.map(str::to_string),
            dedup_key: None,
        }
    }

    #[test]
    fn priority_normalization_and_mapping() {
        assert_eq!(envelope("a.b", None).normalized_priority(), "default");
        assert_eq!(
            envelope("a.b", Some("nope")).normalized_priority(),
            "default"
        );
        assert_eq!(
            envelope("a.b", Some("urgent")).normalized_priority(),
            "urgent"
        );
        assert_eq!(ntfy_priority("urgent"), 5);
        assert_eq!(ntfy_priority("high"), 4);
        assert_eq!(ntfy_priority("default"), 3);
        assert_eq!(ntfy_priority("low"), 2);
        assert!(envelope("a.b", Some("urgent")).is_urgent());
    }

    #[test]
    fn module_is_prefix() {
        assert_eq!(envelope("task.assigned", None).module(), "task");
        assert_eq!(envelope("single", None).module(), "single");
    }

    #[test]
    fn hhmm_parsing() {
        assert_eq!(parse_hhmm("22:30"), Some(22 * 60 + 30));
        assert_eq!(parse_hhmm("00:00"), Some(0));
        assert!(parse_hhmm("24:00").is_none());
        assert!(parse_hhmm("12:60").is_none());
        assert!(parse_hhmm("bad").is_none());
    }

    #[test]
    fn quiet_window_including_midnight() {
        assert!(in_quiet_window(22 * 60, 7 * 60, 23 * 60));
        assert!(in_quiet_window(22 * 60, 7 * 60, 2 * 60));
        assert!(!in_quiet_window(22 * 60, 7 * 60, 12 * 60));
        assert!(in_quiet_window(9 * 60, 18 * 60, 10 * 60));
        assert!(!in_quiet_window(9 * 60, 18 * 60, 20 * 60));
        assert!(
            !in_quiet_window(9 * 60, 9 * 60, 9 * 60),
            "相等窗口视为不启用"
        );
    }

    #[test]
    fn deliver_rules() {
        let prefs = PreferenceView {
            muted_modules: vec!["im".into()],
            quiet_from: Some("22:00".into()),
            quiet_to: Some("07:00".into()),
            ntfy_enabled: true,
        };
        assert!(
            !should_deliver(&prefs, "im", Some(12 * 60), false),
            "静音模块"
        );
        assert!(
            !should_deliver(&prefs, "task", Some(23 * 60), false),
            "免打扰"
        );
        assert!(should_deliver(&prefs, "task", Some(12 * 60), false));
        // 紧急消息绕过免打扰，但显式静音的模块仍然拦截
        assert!(
            should_deliver(&prefs, "task", Some(23 * 60), true),
            "紧急绕过免打扰"
        );
        assert!(
            !should_deliver(&prefs, "im", Some(12 * 60), true),
            "静音模块优先"
        );
        let disabled = PreferenceView {
            ntfy_enabled: false,
            ..prefs.clone()
        };
        assert!(!should_deliver(&disabled, "task", Some(12 * 60), false));
    }

    #[test]
    fn topic_is_stable_and_not_guessable() {
        let id = Uuid::now_v7();
        let topic = topic_for(id);
        assert!(topic.starts_with("u_"));
        assert_eq!(topic.len(), 2 + 32);
    }
}
