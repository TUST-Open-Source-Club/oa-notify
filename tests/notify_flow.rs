//! notify 集成测试：事件落库 + 偏好过滤 + ntfy 推送 + 用户接口。

mod common;

use axum::http::StatusCode;
use club_auth_sdk::TokenVerifier;
use common::*;
use serde_json::json;
use uuid::Uuid;

fn event(target: &[Uuid], event_type: &str, priority: Option<&str>) -> serde_json::Value {
    json!({
        "id": Uuid::now_v7().to_string(),
        "type": event_type,
        "targetUsers": target.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
        "resource": { "type": "task", "id": "t1", "url": "/tasks/t1" },
        "title": "有新任务指派给你",
        "body": "「迎新活动物料」",
        "priority": priority,
    })
}

#[tokio::test]
async fn event_ingest_is_idempotent_and_user_can_read() {
    let app = spawn().await;
    let user = Uuid::now_v7();
    let token = issue_token(&app, user);

    let payload = event(&[user], "task.assigned", Some("high"));
    let first = service_post(&app, "task", "/api/v1/notify/internal/events", &payload).await;
    let first = first.expect(StatusCode::OK);
    assert_eq!(first["created"], 1);
    assert_eq!(first["pushed"], 1, "默认偏好应推送 ntfy");
    assert_eq!(app.ntfy.count(), 1);

    // 幂等：重复投递同一事件不重复落库/推送
    let second = service_post(&app, "task", "/api/v1/notify/internal/events", &payload).await;
    let second = second.expect(StatusCode::OK);
    assert_eq!(second["created"], 0);
    assert_eq!(second["pushed"], 0);
    assert_eq!(app.ntfy.count(), 1);

    // 列表
    let list = request(
        &app.app,
        "GET",
        "/api/v1/notify/notifications",
        Some(&token),
        None,
    )
    .await;
    let list = list.expect(StatusCode::OK);
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert_eq!(list["items"][0]["read"], false);
    assert_eq!(list["items"][0]["url"], "/tasks/t1");
    assert!(list["nextCursor"].is_null());

    // 未读数 + 单条已读 + 全部已读
    let count = request(
        &app.app,
        "GET",
        "/api/v1/notify/notifications/unread-count",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(count.expect(StatusCode::OK)["count"], 1);

    let id = list["items"][0]["id"].as_str().unwrap();
    let read = request(
        &app.app,
        "POST",
        &format!("/api/v1/notify/notifications/{id}/read"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(read.expect(StatusCode::OK)["updated"], 1);

    let payload2 = event(&[user], "task.comment.added", None);
    service_post(&app, "task", "/api/v1/notify/internal/events", &payload2)
        .await
        .expect(StatusCode::OK);
    let read_all = request(
        &app.app,
        "POST",
        "/api/v1/notify/notifications/read-all",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(read_all.expect(StatusCode::OK)["updated"], 1);

    let count = request(
        &app.app,
        "GET",
        "/api/v1/notify/notifications/unread-count",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(count.expect(StatusCode::OK)["count"], 0);
}

#[tokio::test]
async fn muted_module_stores_but_does_not_push() {
    let app = spawn().await;
    let user = Uuid::now_v7();
    let token = issue_token(&app, user);

    // 静音 task 模块
    let saved = request(
        &app.app,
        "PUT",
        "/api/v1/notify/preferences",
        Some(&token),
        Some(&json!({ "mutedModules": ["task"], "ntfyEnabled": true })),
    )
    .await;
    saved.expect(StatusCode::OK);

    let result = service_post(
        &app,
        "task",
        "/api/v1/notify/internal/events",
        &event(&[user], "task.assigned", Some("high")),
    )
    .await;
    let result = result.expect(StatusCode::OK);
    assert_eq!(result["created"], 1, "站内通知仍应落库");
    assert_eq!(result["pushed"], 0, "静音模块不应推送");
    assert_eq!(app.ntfy.count(), 0);

    // 偏好回读
    let prefs = request(
        &app.app,
        "GET",
        "/api/v1/notify/preferences",
        Some(&token),
        None,
    )
    .await;
    let prefs = prefs.expect(StatusCode::OK);
    assert_eq!(prefs["mutedModules"][0], "task");

    // 非法模块 → 422
    let invalid = request(
        &app.app,
        "PUT",
        "/api/v1/notify/preferences",
        Some(&token),
        Some(&json!({ "mutedModules": ["nope"], "ntfyEnabled": true })),
    )
    .await;
    invalid.expect(StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn quiet_hours_block_normal_but_not_urgent() {
    let app = spawn().await;
    let user = Uuid::now_v7();
    let token = issue_token(&app, user);

    // 用当前 UTC+8 时间前后 1 小时构造免打扰窗口
    let now = chrono::Utc::now().with_timezone(&chrono::FixedOffset::east_opt(8 * 3600).unwrap());
    use chrono::Timelike;
    let from = format!("{:02}:{:02}", (now.hour() + 23) % 24, now.minute());
    let to = format!("{:02}:{:02}", (now.hour() + 1) % 24, now.minute());
    request(
        &app.app,
        "PUT",
        "/api/v1/notify/preferences",
        Some(&token),
        Some(&json!({ "mutedModules": [], "quietFrom": from, "quietTo": to, "ntfyEnabled": true })),
    )
    .await
    .expect(StatusCode::OK);

    let normal = service_post(
        &app,
        "task",
        "/api/v1/notify/internal/events",
        &event(&[user], "task.due_soon", None),
    )
    .await;
    assert_eq!(
        normal.expect(StatusCode::OK)["pushed"],
        0,
        "免打扰时段不推送"
    );

    let urgent = service_post(
        &app,
        "im",
        "/api/v1/notify/internal/events",
        &event(&[user], "im.message.mentioned", Some("urgent")),
    )
    .await;
    assert_eq!(
        urgent.expect(StatusCode::OK)["pushed"],
        1,
        "紧急消息应绕过免打扰"
    );
}

#[tokio::test]
async fn cursor_pagination_returns_next_page() {
    let app = spawn().await;
    let user = Uuid::now_v7();
    let token = issue_token(&app, user);

    for i in 0..3 {
        service_post(
            &app,
            "task",
            "/api/v1/notify/internal/events",
            &event(&[user], &format!("task.event{i}"), None),
        )
        .await
        .expect(StatusCode::OK);
    }

    let page1 = request(
        &app.app,
        "GET",
        "/api/v1/notify/notifications?limit=2",
        Some(&token),
        None,
    )
    .await;
    let page1 = page1.expect(StatusCode::OK);
    assert_eq!(page1["items"].as_array().unwrap().len(), 2);
    let cursor = page1["nextCursor"].as_str().expect("nextCursor");

    let page2 = request(
        &app.app,
        "GET",
        &format!("/api/v1/notify/notifications?limit=2&cursor={cursor}"),
        Some(&token),
        None,
    )
    .await;
    let page2 = page2.expect(StatusCode::OK);
    assert_eq!(page2["items"].as_array().unwrap().len(), 1);
    assert!(page2["nextCursor"].is_null());
}

#[tokio::test]
async fn devices_register_list_and_revoke() {
    let app = spawn().await;
    let user = Uuid::now_v7();
    let token = issue_token(&app, user);

    let registered = request(
        &app.app,
        "POST",
        "/api/v1/notify/devices",
        Some(&token),
        Some(&json!({ "vendor": "apple", "platform": "ios", "token": "apns-token-123456", "deviceName": "iPhone" })),
    )
    .await;
    registered.expect(StatusCode::OK);

    let devices = request(
        &app.app,
        "GET",
        "/api/v1/notify/devices",
        Some(&token),
        None,
    )
    .await;
    let devices = devices.expect(StatusCode::OK);
    assert_eq!(devices.as_array().unwrap().len(), 1);
    assert_eq!(devices[0]["vendor"], "apple");

    // 非法平台 → 422
    let invalid = request(
        &app.app,
        "POST",
        "/api/v1/notify/devices",
        Some(&token),
        Some(&json!({ "vendor": "desktop", "token": "apns-token-123456" })),
    )
    .await;
    invalid.expect(StatusCode::UNPROCESSABLE_ENTITY);

    let revoked = request(
        &app.app,
        "DELETE",
        "/api/v1/notify/devices/apns-token-123456",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(revoked.expect(StatusCode::OK)["revoked"], true);

    let devices = request(
        &app.app,
        "GET",
        "/api/v1/notify/devices",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(devices.expect(StatusCode::OK).as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn authentication_is_required() {
    let app = spawn().await;
    // 用户接口无令牌 → 401
    request(&app.app, "GET", "/api/v1/notify/notifications", None, None)
        .await
        .expect(StatusCode::UNAUTHORIZED);
    // 内部事件无服务令牌 → 401
    request(
        &app.app,
        "POST",
        "/api/v1/notify/internal/events",
        None,
        Some(&event(&[Uuid::now_v7()], "task.assigned", None)),
    )
    .await
    .expect(StatusCode::UNAUTHORIZED);
    // 健康检查
    let health = request(&app.app, "GET", "/healthz", None, None).await;
    assert_eq!(health.expect(StatusCode::OK)["status"], "ok");
}

#[tokio::test]
async fn invalid_payloads_and_preference_validation() {
    let app = spawn().await;
    let user = Uuid::now_v7();
    let token = issue_token(&app, user);

    // 缺少必填字段 → 400
    let invalid = service_post(
        &app,
        "task",
        "/api/v1/notify/internal/events",
        &json!({ "id": Uuid::now_v7().to_string(), "type": "task.x", "targetUsers": [] }),
    )
    .await;
    invalid.expect(StatusCode::BAD_REQUEST);

    // 空接收人 → created 0
    let empty = service_post(
        &app,
        "task",
        "/api/v1/notify/internal/events",
        &event(&[], "task.assigned", None),
    )
    .await;
    assert_eq!(empty.expect(StatusCode::OK)["created"], 0);

    // 免打扰时间格式错误 → 422
    let bad_time = request(
        &app.app,
        "PUT",
        "/api/v1/notify/preferences",
        Some(&token),
        Some(&json!({ "mutedModules": [], "quietFrom": "25:00", "ntfyEnabled": true })),
    )
    .await;
    bad_time.expect(StatusCode::UNPROCESSABLE_ENTITY);

    // 未注册设备解绑 → revoked false
    let revoked = request(
        &app.app,
        "DELETE",
        "/api/v1/notify/devices/not-exists-token",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(revoked.expect(StatusCode::OK)["revoked"], false);

    // readyz
    let ready = request(&app.app, "GET", "/readyz", None, None).await;
    assert_eq!(ready.expect(StatusCode::OK)["database"], "ok");
}

#[tokio::test]
async fn unread_filter_and_read_status() {
    let app = spawn().await;
    let user = Uuid::now_v7();
    let token = issue_token(&app, user);

    for i in 0..2 {
        service_post(
            &app,
            "task",
            "/api/v1/notify/internal/events",
            &event(&[user], &format!("task.e{i}"), None),
        )
        .await
        .expect(StatusCode::OK);
    }
    let list = request(
        &app.app,
        "GET",
        "/api/v1/notify/notifications",
        Some(&token),
        None,
    )
    .await;
    let list = list.expect(StatusCode::OK);
    let first_id = list["items"][0]["id"].as_str().unwrap().to_string();
    request(
        &app.app,
        "POST",
        &format!("/api/v1/notify/notifications/{first_id}/read"),
        Some(&token),
        None,
    )
    .await
    .expect(StatusCode::OK);

    let unread = request(
        &app.app,
        "GET",
        "/api/v1/notify/notifications?unreadOnly=true",
        Some(&token),
        None,
    )
    .await;
    let unread = unread.expect(StatusCode::OK);
    assert_eq!(unread["items"].as_array().unwrap().len(), 1);
    assert_eq!(unread["items"][0]["read"], false);
}

#[tokio::test]
async fn jwks_loading_and_token_verification() {
    use std::sync::RwLock;

    let app = spawn().await;
    let jwks_value = serde_json::to_value(&app.signing).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let jwks_for_route = jwks_value.clone();
    let server = axum::Router::new().route(
        "/.well-known/jwks.json",
        axum::routing::get(move || {
            let value = jwks_for_route.clone();
            async move { axum::Json(value) }
        }),
    );
    tokio::spawn(async move {
        axum::serve(listener, server).await.unwrap();
    });

    let mut config = app.state.config.clone();
    config.issuer = format!("http://{addr}");
    let state2 = notify_service::state::SharedState::new(notify_service::state::AppState {
        db: app.state.db.clone(),
        config: config.clone(),
        ntfy: app.ntfy.clone(),
        push: app.state.push.clone(),
        signing_key: RwLock::new(None),
    });

    // 未加载 JWKS 时验签失败
    assert!(state2.verify_token("x").is_err());
    // 加载后可用
    state2.load_jwks().await.expect("load jwks");
    let claims = club_auth_sdk::Claims {
        sub: Uuid::now_v7().to_string(),
        name: "u".into(),
        avatar: None,
        roles: vec![],
        scopes: vec![],
        guest: false,
        iss: config.issuer.clone(),
        iat: chrono::Utc::now().timestamp(),
        exp: chrono::Utc::now().timestamp() + 60,
        jti: Uuid::now_v7().to_string(),
    };
    let kid = club_auth_sdk::jwks::key_id_from_pem(&app.public_pem);
    let token = club_auth_sdk::encode_access_token(&claims, &app.private_pem, &kid).unwrap();
    let verified = state2.verify_token(&token).expect("verify");
    assert_eq!(verified.name, "u");
    // 坏令牌 → 401
    assert!(state2.verify_token("bad-token").is_err());
}

#[tokio::test]
async fn multi_channel_fallback_chain() {
    let app = spawn().await;
    let user = Uuid::now_v7();
    let token = issue_token(&app, user);

    request(
        &app.app,
        "POST",
        "/api/v1/notify/devices",
        Some(&token),
        Some(&json!({ "vendor": "huawei", "platform": "android", "token": "huawei-token-1" })),
    )
    .await
    .expect(StatusCode::OK);

    service_post(
        &app,
        "task",
        "/api/v1/notify/internal/events",
        &event(&[user], "task.assigned", Some("high")),
    )
    .await
    .expect(StatusCode::OK);
    assert_eq!(app.vendor.count(), 1, "应走华为通道");
    assert_eq!(app.fcm.count(), 0);
    assert_eq!(app.ntfy.count(), 0, "厂商成功不应兜底 ntfy");

    app.vendor.set_fail(true);
    service_post(
        &app,
        "task",
        "/api/v1/notify/internal/events",
        &event(&[user], "task.comment.added", Some("high")),
    )
    .await
    .expect(StatusCode::OK);
    assert_eq!(app.vendor.count(), 2);
    assert_eq!(app.fcm.count(), 1, "厂商失败后应尝试 FCM");
    assert_eq!(app.ntfy.count(), 1, "FCM 失败后应兜底 ntfy");
}
