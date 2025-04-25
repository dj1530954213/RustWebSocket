use serde::{Deserialize, Serialize};
use log::{debug, info};

// 定义消息类型，与服务器端保持一致
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WrappedMessage {
    pub group_id: String,       // 组ID
    pub sender_type: String,    // 发送方类型: "center" 或 "local"
    pub receiver_type: String,  // 接收方类型: "center" 或 "local"
    pub content: String,        // 消息内容
    pub message_type: Option<String>, // 消息类型: "heartbeat", "disconnect" 等
}

pub struct MessageService {
    pub group_id: String,
    pub client_type: String,
}

impl MessageService {
    pub fn new(group_id: String) -> Self {
        MessageService {
            group_id,
            client_type: "center".to_string(),
        }
    }

    // 创建初始化消息
    pub fn create_init_message(&self) -> WrappedMessage {
        WrappedMessage {
            group_id: self.group_id.clone(),
            sender_type: self.client_type.clone(),
            receiver_type: "server".to_string(),
            content: "初始化连接".to_string(),
            message_type: None,
        }
    }

    // 创建心跳消息
    pub fn create_heartbeat_message(&self) -> WrappedMessage {
        WrappedMessage {
            group_id: self.group_id.clone(),
            sender_type: self.client_type.clone(),
            receiver_type: "server".to_string(),
            content: "心跳".to_string(),
            message_type: Some("heartbeat".to_string()),
        }
    }
    
    // 创建断开连接消息
    pub fn create_disconnect_message(&self) -> WrappedMessage {
        WrappedMessage {
            group_id: self.group_id.clone(),
            sender_type: self.client_type.clone(),
            receiver_type: "server".to_string(),
            content: "客户端主动断开连接".to_string(),
            message_type: Some("disconnect".to_string()),
        }
    }

    // 创建普通消息
    pub fn create_message(&self, content: String) -> WrappedMessage {
        WrappedMessage {
            group_id: self.group_id.clone(),
            sender_type: self.client_type.clone(),
            receiver_type: "local".to_string(), // center发送给local
            content,
            message_type: None,
        }
    }

    // 处理接收到的消息
    pub fn process_received_message(&self, message: &WrappedMessage) -> String {
        // 检查消息类型
        if let Some(msg_type) = &message.message_type {
            match msg_type.as_str() {
                "heartbeat" => {
                    debug!("收到心跳响应");
                    return format!("[心跳响应]: {}", message.content);
                },
                "disconnect" => {
                    info!("收到断开连接通知: {}", message.content);
                    return format!("[断开通知]: {}", message.content);
                },
                "connect" => {
                    info!("收到上线连接通知: {}", message.content);
                    return format!("[上线通知]: {}", message.content);
                },
                _ => {}
            }
        }

        // 根据发送方处理普通消息
        match message.sender_type.as_str() {
            "server" => {
                format!("[服务器]: {}", message.content)
            },
            "local" => {
                format!("[Local -> Center]: {}", message.content)
            },
            _ => {
                format!("[未知发送方 {}]: {}", message.sender_type, message.content)
            }
        }
    }

    // 检查消息是否是心跳响应
    pub fn is_heartbeat_response(&self, message: &WrappedMessage) -> bool {
        if let Some(msg_type) = &message.message_type {
            return msg_type == "heartbeat";
        }
        false
    }

    // 是否是断开连接通知
    pub fn is_disconnect_notification(&self, message: &WrappedMessage) -> bool {
        if let Some(msg_type) = &message.message_type {
            return msg_type == "disconnect";
        }
        false
    }

    // 是否是上线连接通知
    pub fn is_connect_notification(&self, message: &WrappedMessage) -> bool {
        if let Some(msg_type) = &message.message_type {
            return msg_type == "connect";
        }
        false
    }
} 