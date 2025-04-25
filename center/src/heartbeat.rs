use std::time::Duration;
use tokio::time;
use log::{info, error, debug};

use crate::message::MessageService;
use crate::connection::WebSocketConnection;

pub struct HeartbeatManager {
    interval: Duration,
    message_service: MessageService,
    connection: WebSocketConnection,
}

impl HeartbeatManager {
    pub fn new(message_service: MessageService, connection: WebSocketConnection, interval_secs: u64) -> Self {
        HeartbeatManager {
            interval: Duration::from_secs(interval_secs),
            message_service,
            connection,
        }
    }
    
    // 启动心跳发送任务
    pub async fn start_heartbeat_task(self) {
        tokio::spawn(async move {
            info!("启动心跳任务，间隔: {}秒", self.interval.as_secs());
            let mut interval = time::interval(self.interval);
            
            loop {
                // 等待下一个间隔
                interval.tick().await;
                
                // 发送心跳消息
                debug!("发送心跳消息");
                match self.connection.send_heartbeat(&self.message_service).await {
                    Ok(_) => debug!("心跳消息发送成功"),
                    Err(e) => {
                        error!("发送心跳失败: {}", e);
                        // 发送失败可能意味着连接已断开，结束心跳任务
                        break;
                    }
                }
            }
            
            info!("心跳任务结束");
        });
    }
} 