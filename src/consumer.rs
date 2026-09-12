//! Redis Streams 事件消费者。
//!
//! 以消费组 `notify` 读取各模块事件流；处理成功或无法解析（毒消息）均 ACK，
//! 避免阻塞消费组；Redis 暂不可用时退避重试。

use std::time::Duration;

use club_bus::Bus;

use crate::domain::EventEnvelope;
use crate::ingest::apply_event;
use crate::state::SharedState;

/// 消费组名。
pub const GROUP: &str = "notify";
/// 订阅的模块流。
pub const STREAMS: &[&str] = &[
    "events.im",
    "events.task",
    "events.doc",
    "events.meeting",
    "events.event",
    "events.drive",
    "events.system",
];

/// 消费者主循环（放入独立 tokio 任务；随进程退出）。
pub async fn run(state: SharedState, redis_url: String, consumer_name: String) {
    let bus = loop {
        match Bus::connect(&redis_url).await {
            Ok(bus) => break bus,
            Err(err) => {
                tracing::warn!(error = %err, "连接 Redis 失败，2 秒后重试");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    };
    for stream in STREAMS {
        if let Err(err) = bus.ensure_group(stream, GROUP).await {
            tracing::warn!(stream, error = %err, "创建消费组失败");
        }
    }
    tracing::info!(consumer = %consumer_name, "事件消费者已启动");

    loop {
        match bus
            .read_group(STREAMS, GROUP, &consumer_name, 20, 5000)
            .await
        {
            Ok(messages) => {
                for message in messages {
                    match serde_json::from_value::<EventEnvelope>(message.payload.clone()) {
                        Ok(event) => match apply_event(&state, &event).await {
                            Ok((created, pushed)) => {
                                tracing::info!(
                                    event = %event.event_type,
                                    created,
                                    pushed,
                                    "总线事件已处理"
                                );
                            }
                            Err(err) => {
                                // 处理失败不 ACK，留给后续重投排查
                                tracing::error!(error = %err, id = %message.id, "事件处理失败，暂不 ACK");
                                continue;
                            }
                        },
                        Err(err) => {
                            tracing::warn!(error = %err, id = %message.id, "事件解析失败，按毒消息 ACK");
                        }
                    }
                    if let Err(err) = bus
                        .ack(&message.stream, GROUP, std::slice::from_ref(&message.id))
                        .await
                    {
                        tracing::warn!(error = %err, "ACK 失败");
                    }
                }
            }
            Err(err) => {
                tracing::warn!(error = %err, "读取事件流失败，2 秒后重试");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}
