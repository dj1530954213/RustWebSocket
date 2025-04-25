use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;
use log::{info, error, debug};
use serde::{Deserialize, Serialize};

use crate::message::MessageService;

// 定义连接类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientType {
    Center,
    Local,
}

// 定义连接信息
pub struct Connection {
    pub client_type: ClientType,
    pub group_id: String,
    pub tx: mpsc::UnboundedSender<Message>,
}

pub type Connections = Arc<Mutex<HashMap<String, Connection>>>;

// 定义连接管理器
pub struct ConnectionManager {
    connections: Connections,
}

impl ConnectionManager {
    pub fn new() -> Self {
        ConnectionManager {
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    pub fn get_connections(&self) -> Connections {
        self.connections.clone()
    }
    
    // 验证是否可以添加新连接
    pub fn validate_new_connection(&self, client_type: &ClientType, group_id: &str) -> bool {
        let connections = self.connections.lock().unwrap();
        
        // 检查同一组内是否已存在同类型的客户端
        for (_, conn) in connections.iter() {
            if conn.group_id == group_id && conn.client_type == *client_type {
                // 已存在同组同类型的客户端，拒绝连接
                return false;
            }
        }
        
        true
    }
    
    // 添加新连接
    pub fn add_connection(&self, client_id: String, client_type: ClientType, 
                          group_id: String, tx: mpsc::UnboundedSender<Message>) -> bool {
        if !self.validate_new_connection(&client_type, &group_id) {
            return false;
        }
        
        let mut connections = self.connections.lock().unwrap();
        connections.insert(client_id, Connection {
            client_type,
            group_id,
            tx,
        });
        
        // 打印当前所有连接信息，用于调试
        info!("当前连接数: {}", connections.len());
        for (id, conn) in connections.iter() {
            debug!("  客户端ID: {}, 类型: {:?}, 组ID: {}", id, conn.client_type, conn.group_id);
        }
        
        true
    }
    
    // 移除连接
    pub fn remove_connection(&self, client_id: &str) -> Option<(ClientType, String)> {
        let mut connections = self.connections.lock().unwrap();
        
        if let Some(conn) = connections.get(client_id) {
            let client_type = conn.client_type.clone();
            let group_id = conn.group_id.clone();
            connections.remove(client_id);
            info!("客户端断开连接: {}", client_id);
            return Some((client_type, group_id));
        }
        
        None
    }
    
    // 查找同组的伙伴
    pub fn find_partner(&self, client_id: &str) -> Option<(String, ClientType)> {
        let connections = self.connections.lock().unwrap();
        
        if let Some(conn) = connections.get(client_id) {
            let group_id = &conn.group_id;
            let client_type = &conn.client_type;
            
            // 寻找同组不同类型的客户端
            for (id, other_conn) in connections.iter() {
                if id != client_id && other_conn.group_id == *group_id && other_conn.client_type != *client_type {
                    return Some((id.clone(), other_conn.client_type.clone()));
                }
            }
        }
        
        None
    }
    
    // 通知同组伙伴客户端下线
    pub fn notify_partner_disconnect(&self, partner_id: &str, client_type: ClientType, group_id: String) {
        let connections = self.connections.lock().unwrap();
        
        if let Some(partner_conn) = connections.get(partner_id) {
            let message_service = MessageService::new();
            let disconnect_msg = message_service.build_disconnect_message(group_id, client_type);
            
            if let Ok(json) = serde_json::to_string(&disconnect_msg) {
                if let Err(e) = partner_conn.tx.send(Message::Text(json)) {
                    error!("发送断开连接通知失败: {}", e);
                } else {
                    info!("已向伙伴发送断开连接通知");
                }
            }
        }
    }
    
    // 通知同组伙伴客户端上线
    pub fn notify_partner_connect(&self, partner_id: &str, client_type: ClientType, group_id: String) {
        let connections = self.connections.lock().unwrap();
        
        if let Some(partner_conn) = connections.get(partner_id) {
            let message_service = MessageService::new();
            let connect_msg = message_service.build_connect_message(group_id, client_type);
            
            if let Ok(json) = serde_json::to_string(&connect_msg) {
                if let Err(e) = partner_conn.tx.send(Message::Text(json)) {
                    error!("发送上线通知失败: {}", e);
                } else {
                    info!("已向伙伴发送上线通知");
                }
            }
        }
    }
    
    // 生成新的客户端ID
    pub fn generate_client_id(&self) -> String {
        Uuid::new_v4().to_string()
    }
} 