mod message;
mod connection;
mod heartbeat;

use futures_util::StreamExt;
use std::io::{self, Write};
use tokio_tungstenite::tungstenite::Message;
use log::{error, info, warn};

use message::{WrappedMessage, MessageService};
use connection::WebSocketConnection;
use heartbeat::HeartbeatManager;

#[tokio::main]
async fn main() {
    // 初始化日志
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    env_logger::init();
    
    // 获取用户输入的组ID
    println!("请输入您的组ID:");
    let mut group_id = String::new();
    io::stdin().read_line(&mut group_id).expect("读取输入失败");
    let group_id = group_id.trim().to_string();
    
    if group_id.is_empty() {
        println!("组ID不能为空");
        return;
    }
    
    // 创建消息服务
    let message_service = MessageService::new(group_id);
    
    // 连接到WebSocket服务器
    println!("正在连接到服务器...");
    let (connection, ws_receiver) = match WebSocketConnection::new("ws://127.0.0.1:8080").await {
        Ok(conn) => conn,
        Err(e) => {
            error!("连接服务器失败: {}", e);
            println!("连接服务器失败: {}", e);
            return;
        }
    };
    
    println!("已连接到服务器");
    
    // 发送初始化消息
    if let Err(e) = connection.send_init_message(&message_service).await {
        error!("发送初始化消息失败: {}", e);
        println!("发送初始化消息失败: {}", e);
        return;
    }
    
    // 创建心跳管理器 - 复制connection用于心跳任务
    let heartbeat_connection = WebSocketConnection {
        sender: connection.sender.clone(),
        url: connection.url.clone(),
    };
    let heartbeat_service = MessageService::new(message_service.group_id.clone());
    let heartbeat_manager = HeartbeatManager::new(heartbeat_service, heartbeat_connection, 20);
    
    // 启动心跳任务
    heartbeat_manager.start_heartbeat_task().await;
    
    // 创建用户输入任务
    let input_connection = WebSocketConnection {
        sender: connection.sender.clone(),
        url: connection.url.clone(),
    };
    let input_service = MessageService::new(message_service.group_id.clone());
    
    let input_task = tokio::spawn(async move {
        println!("启动用户输入任务...");
        loop {
            // 提示用户输入消息
            print!("请输入消息(发送给center): ");
            io::stdout().flush().unwrap();
            
            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_err() {
                continue;
            }
            
            let input = input.trim();
            if input.is_empty() {
                continue;
            }
            
            if input == "/quit" {
                // 发送断开连接消息到服务器
                info!("发送断开连接消息");
                let disconnect_msg = input_service.create_disconnect_message();
                match input_connection.send_message(&disconnect_msg).await {
                    Ok(_) => println!("断开连接消息发送成功"),
                    Err(e) => println!("断开连接消息发送失败: {}", e),
                }
                break;
            }
            
            println!("收到用户输入: {}", input);
            
            // 创建并发送消息
            let msg = input_service.create_message(input.to_string());
            if let Err(e) = input_connection.send_message(&msg).await {
                error!("发送消息失败: {}", e);
                println!("发送消息失败: {}", e);
                break;
            } else {
                println!("消息发送成功");
            }
        }
        
        println!("用户输入任务结束");
    });
    
    // 创建WebSocket消息接收任务
    let receiver_service = MessageService::new(message_service.group_id.clone());
    
    let receiver_task = tokio::spawn(async move {
        println!("开始监听接收消息...");
        
        let mut ws_receiver = ws_receiver;
        while let Some(result) = ws_receiver.next().await {
            match result {
                Ok(msg) => {
                    match msg {
                        Message::Text(text) => {
                            //println!("\n收到文本消息: {}", text);
                            match serde_json::from_str::<WrappedMessage>(&text) {
                                Ok(msg) => {
                                    println!("\n==== 收到消息 ====");
                                    println!("发送方: {}", msg.sender_type);
                                    println!("接收方: {}", msg.receiver_type);
                                    println!("组ID: {}", msg.group_id);
                                    println!("内容: {}", msg.content);
                                    if let Some(msg_type) = &msg.message_type {
                                        println!("消息类型: {}", msg_type);
                                    }
                                    println!("==================\n");
                                    
                                    // 使用消息服务处理消息
                                    let formatted_msg = receiver_service.process_received_message(&msg);
                                    println!("{}", formatted_msg);
                                    
                                    // 判断是否是断开连接通知，如果是则可以考虑自动退出
                                    if receiver_service.is_disconnect_notification(&msg) {
                                        warn!("\n对方已断开连接，无法发送消息！");
                                        println!("\n对方已断开连接，按回车继续或输入/quit退出");
                                    }
                                    
                                    // 判断是否是上线通知
                                    if receiver_service.is_connect_notification(&msg) {
                                        info!("\n对方已上线连接，可以开始发送消息");
                                        println!("\n对方已上线连接，可以开始聊天");
                                    }
                                    
                                    print!("请输入消息(发送给center): ");
                                    io::stdout().flush().unwrap();
                                },
                                Err(e) => {
                                    println!("\n解析消息失败: {}", e);
                                    println!("原始消息: {}", text);
                                }
                            }
                        },
                        Message::Binary(data) => println!("\n收到二进制消息，长度: {}", data.len()),
                        Message::Ping(_) => println!("\n收到ping"),
                        Message::Pong(_) => println!("\n收到pong"),
                        Message::Close(frame) => {
                            println!("\n收到关闭消息: {:?}", frame);
                            println!("\n服务器已断开连接");
                            break;
                        },
                        Message::Frame(_) => println!("\n收到帧消息"),
                    }
                },
                Err(e) => {
                    println!("\n接收消息错误: {}", e);
                    println!("\n与服务器的连接已断开");
                    break;
                }
            }
        }
        
        println!("\n消息接收任务结束");
    });
    
    // 等待任意一个任务完成
    tokio::select! {
        _ = input_task => println!("输入任务结束"),
        _ = receiver_task => println!("接收任务结束"),
    }
    
    println!("连接已关闭");
}
