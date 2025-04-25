use serde::{Deserialize, Serialize};
use log::{info, error, debug};
use tokio_tungstenite::tungstenite::Message;

use crate::connection::{ClientType, Connections};

// 定义消息类型
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WrappedMessage {
    pub group_id: String,       // 组ID
    pub sender_type: String,    // 发送方类型: "center" 或 "local"
    pub receiver_type: String,  // 接收方类型: "center" 或 "local"
    pub content: String,        // 消息内容
    pub message_type: Option<String>, // 消息类型: "heartbeat", "disconnect", 或普通消息
}

pub struct MessageService {}

impl MessageService {
    pub fn new() -> Self {
        MessageService {}
    }

    // 构建初始化确认消息
    pub fn build_confirmation_message(&self, group_id: String, client_type: ClientType, client_id: String) -> WrappedMessage {
        WrappedMessage {
            group_id,
            sender_type: "server".to_string(),
            receiver_type: match client_type {
                ClientType::Center => "center".to_string(),
                ClientType::Local => "local".to_string(),
            },
            content: format!("连接成功，您的ID是: {}", client_id),
            message_type: None,
        }
    }

    // 构建心跳回应消息
    pub fn build_heartbeat_response(&self, group_id: String, client_type: ClientType) -> WrappedMessage {
        WrappedMessage {
            group_id,
            sender_type: "server".to_string(),
            receiver_type: match client_type {
                ClientType::Center => "center".to_string(),
                ClientType::Local => "local".to_string(),
            },
            content: "心跳确认".to_string(),
            message_type: Some("heartbeat".to_string()),
        }
    }

    // 构建连接拒绝消息
    pub fn build_rejection_message(&self, group_id: String, client_type: ClientType) -> WrappedMessage {
        WrappedMessage {
            group_id,
            sender_type: "server".to_string(),
            receiver_type: match client_type {
                ClientType::Center => "center".to_string(),
                ClientType::Local => "local".to_string(),
            },
            content: "连接被拒绝：同组内已存在相同类型的客户端".to_string(),
            message_type: Some("rejection".to_string()),
        }
    }

    // 构建发送确认消息
    pub fn build_send_confirmation(&self, group_id: String, client_type: ClientType) -> WrappedMessage {
        WrappedMessage {
            group_id,
            sender_type: "server".to_string(),
            receiver_type: match client_type {
                ClientType::Center => "center".to_string(),
                ClientType::Local => "local".to_string(),
            },
            content: "消息已发送".to_string(),
            message_type: None,
        }
    }

    // 构建断开连接通知
    pub fn build_disconnect_message(&self, group_id: String, client_type: ClientType) -> WrappedMessage {
        let disconnected_type = match client_type {
            ClientType::Center => "center".to_string(),
            ClientType::Local => "local".to_string(),
        };
        
        let receiver_type = match client_type {
            ClientType::Center => "local".to_string(),
            ClientType::Local => "center".to_string(),
        };
        
        WrappedMessage {
            group_id,
            sender_type: "server".to_string(),
            receiver_type,
            content: format!("{}已断开连接", disconnected_type),
            message_type: Some("disconnect".to_string()),
        }
    }

    // 构建客户端上线通知
    pub fn build_connect_message(&self, group_id: String, client_type: ClientType) -> WrappedMessage {
        let connected_type = match client_type {
            ClientType::Center => "center".to_string(),
            ClientType::Local => "local".to_string(),
        };
        
        let receiver_type = match client_type {
            ClientType::Center => "local".to_string(), 
            ClientType::Local => "center".to_string(),
        };
        
        WrappedMessage {
            group_id,
            sender_type: "server".to_string(),
            receiver_type,
            content: format!("{}已上线连接", connected_type),
            message_type: Some("connect".to_string()),
        }
    }

    // 处理接收到的消息，确保消息格式正确
    pub fn process_message(&self, mut msg: WrappedMessage, client_type: &ClientType, group_id: &str, _client_id: &str) -> WrappedMessage {
        debug!("处理原始消息: {:?}", msg);
        
        // 保存原始的接收方类型，因为客户端可能已经设置了正确的接收方
        let original_receiver = msg.receiver_type.clone();
        
        // 确保组ID正确
        msg.group_id = group_id.to_string();
        
        // 确保发送方类型正确
        msg.sender_type = match client_type {
            ClientType::Center => "center".to_string(),
            ClientType::Local => "local".to_string(),
        };
        
        // 如果接收方类型不是center或local，则根据发送方类型设置默认接收方
        if !(original_receiver == "center" || original_receiver == "local") {
            msg.receiver_type = match client_type {
                ClientType::Center => "local".to_string(),
                ClientType::Local => "center".to_string(),
            };
        } else {
            // 保留客户端设置的接收方
            msg.receiver_type = original_receiver;
        }
        
        // 打印处理后的消息信息
        debug!("处理后的消息: {:?}", msg);
        info!("处理消息: 从 {:?} 到 {} 在组 {}", client_type, msg.receiver_type, group_id);
        
        msg
    }

    // 转发消息到目标接收方
    pub fn forward_message(&self, msg: WrappedMessage, connections: &Connections, sender_id: &str) -> bool {
        debug!("开始消息转发，发送方ID: {}", sender_id);
        
        // 将消息JSON缓存，避免多次序列化
        let json_msg = match serde_json::to_string(&msg) {
            Ok(json) => {
                debug!("消息序列化成功: {}", json);
                json
            },
            Err(e) => {
                error!("消息序列化失败: {}", e);
                return false;
            }
        };

        let connections_lock = match connections.lock() {
            Ok(lock) => lock,
            Err(e) => {
                error!("获取连接锁失败: {:?}", e);
                return false;
            }
        };
        
        let target_type = match msg.receiver_type.as_str() {
            "center" => ClientType::Center,
            "local" => ClientType::Local,
            _ => {
                error!("未知的接收方类型: {}", msg.receiver_type);
                return false;
            }
        };
        
        debug!("寻找接收方，目标类型: {:?}, 组ID: {}", target_type, msg.group_id);
        
        // 创建一个集合存储所有匹配的接收方
        let mut targets = Vec::new();
        let mut found_target = false;
        
        for (receiver_id, connection) in connections_lock.iter() {
            // 跳过发送者自己
            if receiver_id == sender_id {
                continue;
            }
            
            // 如果客户端类型和组ID都匹配，则转发消息
            if connection.client_type == target_type && connection.group_id == msg.group_id {
                found_target = true;
                targets.push(receiver_id.clone());
            }
        }
        
        // 释放锁，然后发送消息
        drop(connections_lock);
        
        if found_target {
            debug!("找到 {} 个匹配的接收方", targets.len());
            
            for target_id in targets {
                if let Some(conn) = connections.lock().unwrap().get(&target_id) {
                    debug!("准备发送消息到接收方 {}", target_id);
                    let websocket_msg = Message::Text(json_msg.clone());
                    match conn.tx.send(websocket_msg) {
                        Ok(_) => debug!("消息成功发送到接收方 {}", target_id),
                        Err(e) => error!("发送消息到接收方 {} 失败: {}", target_id, e),
                    }
                } else {
                    error!("获取接收方 {} 的连接失败", target_id);
                }
            }
            
            true
        } else {
            info!("警告: 未找到同组的目标接收方！组号: {}, 目标类型: {}", msg.group_id, msg.receiver_type);
            false
        }
    }

    // 判断是否是心跳消息
    pub fn is_heartbeat(&self, msg: &WrappedMessage) -> bool {
        match &msg.message_type {
            Some(message_type) if message_type == "heartbeat" => true,
            _ => false,
        }
    }
    
    // 判断是否是断开连接消息
    pub fn is_disconnect(&self, msg: &WrappedMessage) -> bool {
        match &msg.message_type {
            Some(message_type) if message_type == "disconnect" => true,
            _ => false,
        }
    }
} 