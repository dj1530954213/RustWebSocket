use futures_util::{SinkExt, StreamExt};
use tokio::sync::Mutex;
use std::sync::Arc;
use tokio_tungstenite::{connect_async, tungstenite::Message, WebSocketStream, MaybeTlsStream};
use url::Url;
use log::{info, error, debug};
use futures_util::stream::SplitStream;
use futures_util::stream::SplitSink;
use tokio::net::TcpStream;

use crate::message::{WrappedMessage, MessageService};

// 使用MaybeTlsStream包装的TcpStream类型
type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct WebSocketConnection {
    pub sender: Arc<Mutex<SplitSink<WsStream, Message>>>,
    pub url: Url,
}

impl WebSocketConnection {
    // 创建新的连接
    pub async fn new(url_str: &str) -> Result<(Self, SplitStream<WsStream>), String> {
        let url = match Url::parse(url_str) {
            Ok(url) => url,
            Err(e) => return Err(format!("无效的URL: {}", e)),
        };
        
        // 连接WebSocket服务器
        let (ws_stream, _) = match connect_async(url.clone()).await {
            Ok(conn) => conn,
            Err(e) => return Err(format!("连接服务器失败: {}", e)),
        };
        
        info!("已连接到服务器: {}", url);
        
        let (ws_sender, ws_receiver) = ws_stream.split();
        let sender = Arc::new(Mutex::new(ws_sender));
        
        Ok((
            WebSocketConnection {
                sender,
                url,
            },
            ws_receiver
        ))
    }
    
    // 发送消息
    pub async fn send_message(&self, message: &WrappedMessage) -> Result<(), String> {
        let json = match serde_json::to_string(message) {
            Ok(json) => json,
            Err(e) => return Err(format!("序列化消息失败: {}", e)),
        };
        
        debug!("准备发送消息: {}", json);
        
        let mut sender = self.sender.lock().await;
        match sender.send(Message::Text(json.clone())).await {
            Ok(_) => {
                debug!("消息发送成功");
                Ok(())
            },
            Err(e) => Err(format!("发送消息失败: {}", e)),
        }
    }
    
    // 发送初始化消息
    pub async fn send_init_message(&self, message_service: &MessageService) -> Result<(), String> {
        let init_msg = message_service.create_init_message();
        info!("发送初始化消息");
        self.send_message(&init_msg).await
    }
    
    // 发送心跳消息
    pub async fn send_heartbeat(&self, message_service: &MessageService) -> Result<(), String> {
        let heartbeat_msg = message_service.create_heartbeat_message();
        debug!("发送心跳消息");
        self.send_message(&heartbeat_msg).await
    }
} 