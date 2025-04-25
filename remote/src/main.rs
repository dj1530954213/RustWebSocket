mod connection;
mod message;
mod heartbeat;
mod handler;

use std::sync::Arc;
use tokio::net::TcpListener;
use log::info;

use connection::ConnectionManager;
use heartbeat::HeartbeatManager;
use handler::WebSocketHandler;

#[tokio::main]
async fn main() {
    // 设置环境变量以初始化日志
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    
    // 初始化日志
    env_logger::init();
    
    println!("启动服务器...");
    
    let addr = "127.0.0.1:8080";
    let listener = TcpListener::bind(addr).await.expect("无法绑定地址");
    println!("WebSocket 服务器启动在: {}", addr);
    info!("WebSocket 服务器启动在: {}", addr);

    // 创建连接管理器
    let connection_manager = Arc::new(ConnectionManager::new());
    
    // 创建心跳管理器（设置超时为30秒）
    let heartbeat_manager = Arc::new(HeartbeatManager::new(connection_manager.clone(), 30));
    
    // 启动心跳检查任务
    heartbeat_manager.start_heartbeat_check();
    
    // 创建WebSocket处理器
    let ws_handler = WebSocketHandler::new(connection_manager.clone(), heartbeat_manager.clone());

    println!("等待客户端连接...");
    while let Ok((stream, addr)) = listener.accept().await {
        println!("接受新连接: {}", addr);
        info!("接受新连接: {}", addr);
        
        let handler = ws_handler.clone();
        tokio::spawn(async move {
            handler.handle_connection(stream).await;
        });
    }
}

// 为WebSocketHandler实现Clone
impl Clone for WebSocketHandler {
    fn clone(&self) -> Self {
        WebSocketHandler::new(
            self.connection_manager.clone(),
            self.heartbeat_manager.clone()
        )
    }
}
