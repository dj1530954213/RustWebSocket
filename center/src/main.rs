use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use tokio::sync::{mpsc, Mutex};
use std::sync::Arc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;
use log::{info, error};

// 定义消息类型，与服务器端保持一致
#[derive(Serialize, Deserialize, Debug, Clone)]
struct WrappedMessage {
    group_id: String,       // 组ID
    sender_type: String,    // 发送方类型: "center" 或 "local"
    receiver_type: String,  // 接收方类型: "center" 或 "local"
    content: String,        // 消息内容
}

#[tokio::main]
async fn main() {
    // 初始化日志
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
    
    // 连接到WebSocket服务器
    let url = Url::parse("ws://127.0.0.1:8080").expect("无效的URL");
    
    println!("正在连接到服务器...");
    let (ws_stream, _) = match connect_async(url).await {
        Ok(conn) => conn,
        Err(e) => {
            error!("连接服务器失败: {}", e);
            println!("连接服务器失败: {}", e);
            return;
        }
    };
    
    println!("已连接到服务器");
    
    let (ws_sender, ws_receiver) = ws_stream.split();
    
    // 使用Arc和Mutex保护ws_sender，以便在多个任务间共享
    let ws_sender = Arc::new(Mutex::new(ws_sender));
    
    // 发送初始化消息
    let init_msg = WrappedMessage {
        group_id: group_id.clone(),
        sender_type: "center".to_string(),
        receiver_type: "server".to_string(),
        content: "初始化连接".to_string(),
    };
    
    let init_json = serde_json::to_string(&init_msg).expect("序列化失败");
    println!("发送初始化消息: {}", init_json);
    
    // 通过临时获取锁发送初始化消息
    let mut sender_lock = ws_sender.lock().await;
    sender_lock.send(Message::Text(init_json)).await.expect("发送初始化消息失败");
    drop(sender_lock); // 释放锁
    
    // 创建用户输入任务
    let ws_sender_clone = ws_sender.clone();
    let group_id_clone = group_id.clone();
    
    let input_task = tokio::spawn(async move {
        println!("启动用户输入任务...");
        loop {
            // 提示用户输入消息
            print!("请输入消息(发送给local): ");
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
                break;
            }
            
            println!("收到用户输入: {}", input);
            
            // 创建消息
            let msg = WrappedMessage {
                group_id: group_id_clone.clone(),
                sender_type: "center".to_string(),
                receiver_type: "local".to_string(),
                content: input.to_string(),
            };
            
            // 序列化并发送消息
            match serde_json::to_string(&msg) {
                Ok(json) => {
                    println!("准备发送消息到WebSocket: {}", json);
                    // 获取锁并发送消息
                    let mut sender = ws_sender_clone.lock().await;
                    match sender.send(Message::Text(json.clone())).await {
                        Ok(_) => println!("消息成功发送到WebSocket服务器"),
                        Err(e) => {
                            error!("发送消息到WebSocket失败: {}", e);
                            println!("发送消息到WebSocket失败: {}", e);
                            break;
                        }
                    }
                }
                Err(e) => {
                    error!("序列化消息失败: {}", e);
                    println!("序列化消息失败: {}", e);
                }
            }
        }
        
        println!("用户输入任务结束");
    });
    
    // 创建WebSocket消息接收任务
    let receiver_task = tokio::spawn(async move {
        println!("开始监听接收消息...");
        
        let mut ws_receiver = ws_receiver;
        while let Some(result) = ws_receiver.next().await {
            match result {
                Ok(msg) => {
                    match msg {
                        Message::Text(text) => {
                            println!("\n收到文本消息: {}", text);
                            match serde_json::from_str::<WrappedMessage>(&text) {
                                Ok(msg) => {
                                    println!("\n==== 收到消息 ====");
                                    println!("发送方: {}", msg.sender_type);
                                    println!("接收方: {}", msg.receiver_type);
                                    println!("组ID: {}", msg.group_id);
                                    println!("内容: {}", msg.content);
                                    println!("==================\n");
                                    
                                    match msg.sender_type.as_str() {
                                        "server" => {
                                            println!("\n[服务器]: {}", msg.content);
                                        }
                                        "local" => {
                                            println!("\n[Local -> Center]: {}", msg.content);
                                        }
                                        _ => {
                                            println!("\n[未知发送方 {}]: {}", msg.sender_type, msg.content);
                                        }
                                    }
                                    print!("请输入消息(发送给local): ");
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
                            break;
                        },
                        Message::Frame(_) => println!("\n收到帧消息"),
                    }
                },
                Err(e) => {
                    println!("\n接收消息错误: {}", e);
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
