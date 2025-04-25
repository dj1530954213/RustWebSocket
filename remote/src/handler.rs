use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use futures_util::{SinkExt, StreamExt};
use log::{info, error, debug};
use std::sync::Arc;

use crate::connection::{ConnectionManager, ClientType, Connections};
use crate::message::{MessageService, WrappedMessage};
use crate::heartbeat::HeartbeatManager;

pub struct WebSocketHandler {
    pub connection_manager: Arc<ConnectionManager>,
    pub message_service: MessageService,
    pub heartbeat_manager: Arc<HeartbeatManager>,
}

impl WebSocketHandler {
    pub fn new(connection_manager: Arc<ConnectionManager>, heartbeat_manager: Arc<HeartbeatManager>) -> Self {
        WebSocketHandler {
            connection_manager,
            message_service: MessageService::new(),
            heartbeat_manager,
        }
    }
    
    pub async fn handle_connection(&self, stream: TcpStream) {
        let addr = match stream.peer_addr() {
            Ok(addr) => addr,
            Err(e) => {
                error!("无法获取对等地址: {}", e);
                return;
            }
        };
        
        let ws_stream = match tokio_tungstenite::accept_async(stream).await {
            Ok(stream) => stream,
            Err(e) => {
                error!("WebSocket握手失败: {}", e);
                return;
            }
        };
        
        info!("WebSocket连接建立: {}", addr);
        
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        
        // 等待客户端发送初始化消息
        let init_msg = match self.receive_init_message(&mut ws_receiver).await {
            Some(msg) => msg,
            None => return,
        };
        
        // 解析客户端类型和组ID
        let (client_type, group_id) = match self.parse_client_info(&init_msg) {
            Some(info) => info,
            None => return,
        };
        
        // 验证连接
        if !self.connection_manager.validate_new_connection(&client_type, &group_id) {
            info!("拒绝连接: 同组内已存在相同类型的客户端");
            
            // 发送拒绝消息
            let reject_msg = self.message_service.build_rejection_message(group_id, client_type);
            if let Ok(json) = serde_json::to_string(&reject_msg) {
                let _ = ws_sender.send(Message::Text(json)).await;
            }
            
            return;
        }
        
        // 创建客户端ID和消息通道
        let client_id = self.connection_manager.generate_client_id();
        let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
        
        // 添加连接
        if !self.connection_manager.add_connection(client_id.clone(), client_type.clone(), group_id.clone(), tx.clone()) {
            error!("添加连接失败");
            return;
        }
        
        // 初始化心跳记录
        self.heartbeat_manager.update_heartbeat(&client_id, client_type.clone(), group_id.clone());
        
        // 发送确认消息
        let confirm_msg = self.message_service.build_confirmation_message(group_id.clone(), client_type.clone(), client_id.clone());
        if let Ok(json) = serde_json::to_string(&confirm_msg) {
            if let Err(e) = ws_sender.send(Message::Text(json)).await {
                error!("发送确认消息失败: {}", e);
                return;
            }
        }
        
        info!("客户端注册: 类型={:?}, 组ID={}, 客户端ID={}", client_type, group_id, client_id);
        
        // 查找同组的伙伴客户端并通知上线
        if let Some((partner_id, _)) = self.connection_manager.find_partner(&client_id) {
            self.notify_partner_connect(&partner_id, client_type.clone(), group_id.clone());
        }
        
        // 启动发送任务
        let sender_client_id = client_id.clone();
        let sender_task = tokio::spawn(async move {
            debug!("开始监听发送给客户端 {} 的消息...", sender_client_id);
            while let Some(msg) = rx.recv().await {
                if let Err(e) = ws_sender.send(msg).await {
                    error!("发送消息到客户端 {} 失败: {}", sender_client_id, e);
                    break;
                }
            }
            debug!("客户端 {} 的消息发送通道已关闭", sender_client_id);
        });
        
        // 启动接收任务
        let connections = self.connection_manager.get_connections();
        let receiver_client_id = client_id.clone();
        let message_service = self.message_service.clone();
        let heartbeat_manager = self.heartbeat_manager.clone();
        let client_type_clone = client_type.clone();
        let group_id_clone = group_id.clone();
        
        let receiver_task = tokio::spawn(async move {
            handle_client_messages(
                &mut ws_receiver, 
                &connections, 
                &receiver_client_id,
                client_type_clone,
                group_id_clone,
                message_service,
                heartbeat_manager
            ).await;
        });
        
        // 等待任务完成
        tokio::select! {
            _ = sender_task => {
                debug!("发送任务结束，客户端ID: {}", client_id);
            },
            _ = receiver_task => {
                debug!("接收任务结束，客户端ID: {}", client_id);
            },
        }
        
        // 处理客户端断开
        self.handle_disconnect(&client_id);
    }
    
    // 接收初始化消息
    async fn receive_init_message(&self, ws_receiver: &mut futures_util::stream::SplitStream<tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>>) -> Option<WrappedMessage> {
        match ws_receiver.next().await {
            Some(Ok(msg)) => {
                match msg.to_text() {
                    Ok(text) => {
                        info!("收到初始化消息: {}", text);
                        match serde_json::from_str::<WrappedMessage>(text) {
                            Ok(parsed_msg) => Some(parsed_msg),
                            Err(e) => {
                                error!("解析初始化消息失败: {}", e);
                                None
                            }
                        }
                    },
                    Err(e) => {
                        error!("初始化消息不是文本: {}", e);
                        None
                    }
                }
            },
            Some(Err(e)) => {
                error!("接收初始化消息失败: {}", e);
                None
            },
            None => {
                error!("连接关闭，未收到初始化消息");
                None
            }
        }
    }
    
    // 解析客户端信息
    fn parse_client_info(&self, init_msg: &WrappedMessage) -> Option<(ClientType, String)> {
        let client_type = match init_msg.sender_type.as_str() {
            "center" => ClientType::Center,
            "local" => ClientType::Local,
            _ => {
                error!("未知的客户端类型: {}", init_msg.sender_type);
                return None;
            }
        };
        
        Some((client_type, init_msg.group_id.clone()))
    }
    
    // 处理客户端断开连接
    fn handle_disconnect(&self, client_id: &str) {
        if let Some((client_type, group_id)) = self.connection_manager.remove_connection(client_id) {
            // 移除心跳记录
            self.heartbeat_manager.remove_heartbeat(client_id);
            
            // 查找伙伴并通知
            if let Some((partner_id, _)) = self.connection_manager.find_partner(client_id) {
                self.connection_manager.notify_partner_disconnect(&partner_id, client_type, group_id);
            }
        }
    }
    
    // 通知伙伴客户端有新连接
    fn notify_partner_connect(&self, partner_id: &str, client_type: ClientType, group_id: String) {
        self.connection_manager.notify_partner_connect(partner_id, client_type, group_id);
    }
}

// 处理客户端消息
async fn handle_client_messages(
    ws_receiver: &mut futures_util::stream::SplitStream<tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>>,
    connections: &Connections,
    client_id: &str,
    client_type: ClientType,
    group_id: String,
    message_service: MessageService,
    heartbeat_manager: Arc<HeartbeatManager>
) {
    while let Some(result) = ws_receiver.next().await {
        match result {
            Ok(msg) => {
                match msg {
                    Message::Text(text) => {
                        debug!("收到客户端 {} 的消息: {}", client_id, text);
                        
                        match serde_json::from_str::<WrappedMessage>(&text) {
                            Ok(parsed_msg) => {
                                // 更新心跳
                                heartbeat_manager.update_heartbeat(client_id, client_type.clone(), group_id.clone());
                                
                                // 检查是否是心跳消息
                                if message_service.is_heartbeat(&parsed_msg) {
                                    debug!("收到心跳消息");
                                    
                                    // 发送心跳回应
                                    let heartbeat_response = message_service.build_heartbeat_response(group_id.clone(), client_type.clone());
                                    if let Ok(json) = serde_json::to_string(&heartbeat_response) {
                                        if let Some(conn) = connections.lock().unwrap().get(client_id) {
                                            if let Err(e) = conn.tx.send(Message::Text(json)) {
                                                error!("发送心跳回应失败: {}", e);
                                            }
                                        }
                                    }
                                    
                                    continue;
                                }
                                
                                // 检查是否是断开连接消息
                                if message_service.is_disconnect(&parsed_msg) {
                                    info!("收到客户端 {} 的断开连接消息", client_id);
                                    
                                    // 发送确认消息
                                    let confirm_msg = message_service.build_send_confirmation(group_id.clone(), client_type.clone());
                                    if let Ok(json) = serde_json::to_string(&confirm_msg) {
                                        if let Some(conn) = connections.lock().unwrap().get(client_id) {
                                            let _ = conn.tx.send(Message::Text(json));
                                        }
                                    }
                                    
                                    // 通知同组的伙伴客户端断开连接
                                    // 注意，这里不实际移除连接，因为退出循环后会自动调用handle_disconnect
                                    if let Some((partner_id, _)) = find_partner_connection(connections, client_id, &group_id, &client_type) {
                                        notify_partner_disconnect(connections, &partner_id, client_type.clone(), group_id.clone(), &message_service);
                                    }
                                    
                                    // 退出循环，导致连接被关闭
                                    break;
                                }
                                
                                // 处理普通消息
                                let processed_msg = message_service.process_message(parsed_msg, &client_type, &group_id, client_id);
                                
                                // 转发消息
                                let _forward_success = message_service.forward_message(processed_msg, connections, client_id);
                                
                                // 发送确认消息
                                let confirm_msg = message_service.build_send_confirmation(group_id.clone(), client_type.clone());
                                if let Ok(json) = serde_json::to_string(&confirm_msg) {
                                    if let Some(conn) = connections.lock().unwrap().get(client_id) {
                                        if let Err(e) = conn.tx.send(Message::Text(json)) {
                                            error!("发送确认消息失败: {}", e);
                                        }
                                    }
                                }
                            },
                            Err(e) => {
                                error!("解析消息失败: {}", e);
                            }
                        }
                    },
                    Message::Binary(data) => {
                        debug!("收到二进制消息，长度: {}", data.len());
                    },
                    Message::Ping(_) => {
                        debug!("收到ping，发送pong");
                        // 回复pong
                        if let Some(conn) = connections.lock().unwrap().get(client_id) {
                            if let Err(e) = conn.tx.send(Message::Pong(vec![])) {
                                error!("发送pong失败: {}", e);
                            }
                        }
                        
                        // 顺便更新心跳
                        heartbeat_manager.update_heartbeat(client_id, client_type.clone(), group_id.clone());
                    },
                    Message::Pong(_) => {
                        debug!("收到pong");
                        heartbeat_manager.update_heartbeat(client_id, client_type.clone(), group_id.clone());
                    },
                    Message::Close(_) => {
                        info!("收到关闭消息");
                        break;
                    },
                    Message::Frame(_) => {
                        debug!("收到帧消息");
                    },
                }
            },
            Err(e) => {
                error!("接收消息错误: {}", e);
                break;
            }
        }
    }
}

// 辅助函数：查找同组伙伴客户端
fn find_partner_connection(connections: &Connections, client_id: &str, group_id: &str, client_type: &ClientType) -> Option<(String, ClientType)> {
    let connections_lock = connections.lock().unwrap();
    
    if let Some(conn) = connections_lock.get(client_id) {
        // 寻找同组不同类型的客户端
        for (id, other_conn) in connections_lock.iter() {
            if id != client_id && other_conn.group_id == *group_id && other_conn.client_type != *client_type {
                return Some((id.clone(), other_conn.client_type.clone()));
            }
        }
    }
    
    None
}

// 辅助函数：通知伙伴客户端断开连接
fn notify_partner_disconnect(connections: &Connections, partner_id: &str, client_type: ClientType, group_id: String, message_service: &MessageService) {
    let connections_lock = connections.lock().unwrap();
    
    if let Some(partner_conn) = connections_lock.get(partner_id) {
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

// 为MessageService实现Clone
impl Clone for MessageService {
    fn clone(&self) -> Self {
        MessageService::new()
    }
} 