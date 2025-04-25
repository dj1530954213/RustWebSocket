use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::time;
use log::warn;

use crate::connection::{ConnectionManager, ClientType};

// 心跳记录
pub struct HeartbeatRecord {
    pub last_heartbeat: Instant,
    pub client_type: ClientType,
    pub group_id: String,
}

// 心跳管理器
pub struct HeartbeatManager {
    heartbeats: Arc<Mutex<HashMap<String, HeartbeatRecord>>>,
    connection_manager: Arc<ConnectionManager>,
    timeout: Duration,
}

impl HeartbeatManager {
    pub fn new(connection_manager: Arc<ConnectionManager>, timeout_seconds: u64) -> Self {
        HeartbeatManager {
            heartbeats: Arc::new(Mutex::new(HashMap::new())),
            connection_manager,
            timeout: Duration::from_secs(timeout_seconds),
        }
    }
    
    // 更新心跳记录
    pub fn update_heartbeat(&self, client_id: &str, client_type: ClientType, group_id: String) {
        let mut heartbeats = self.heartbeats.lock().unwrap();
        heartbeats.insert(client_id.to_string(), HeartbeatRecord {
            last_heartbeat: Instant::now(),
            client_type,
            group_id,
        });
    }
    
    // 移除心跳记录
    pub fn remove_heartbeat(&self, client_id: &str) {
        let mut heartbeats = self.heartbeats.lock().unwrap();
        heartbeats.remove(client_id);
    }
    
    // 启动心跳检查任务
    pub fn start_heartbeat_check(&self) {
        let heartbeats = self.heartbeats.clone();
        let connection_manager = self.connection_manager.clone();
        let timeout = self.timeout;
        
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(5)); // 每5秒检查一次
            
            loop {
                interval.tick().await;
                
                let now = Instant::now();
                let mut expired_clients = Vec::new();
                
                // 查找超时的客户端
                {
                    let heartbeats = heartbeats.lock().unwrap();
                    for (client_id, record) in heartbeats.iter() {
                        if now.duration_since(record.last_heartbeat) > timeout {
                            expired_clients.push((
                                client_id.clone(), 
                                record.client_type.clone(),
                                record.group_id.clone()
                            ));
                        }
                    }
                }
                
                // 处理超时的客户端
                for (client_id, client_type, group_id) in expired_clients {
                    warn!("客户端 {} 心跳超时，将断开连接", client_id);
                    
                    // 查找伙伴客户端并通知
                    if let Some((partner_id, _)) = connection_manager.find_partner(&client_id) {
                        connection_manager.notify_partner_disconnect(&partner_id, client_type, group_id);
                    }
                    
                    // 移除连接和心跳记录
                    connection_manager.remove_connection(&client_id);
                    
                    let mut heartbeats = heartbeats.lock().unwrap();
                    heartbeats.remove(&client_id);
                }
            }
        });
    }
} 