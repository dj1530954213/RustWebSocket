use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use log::{info, error, debug};

// 定义消息类型
#[derive(Serialize, Deserialize, Debug, Clone)]
struct WrappedMessage {
    group_id: String,       // 组ID
    sender_type: String,    // 发送方类型: "center" 或 "local"
    receiver_type: String,  // 接收方类型: "center" 或 "local"
    content: String,        // 消息内容
}

// 定义连接类型
#[derive(Debug, Clone, PartialEq, Eq)]
enum ClientType {
    Center,
    Local,
}

// 定义连接信息
struct Connection {
    client_type: ClientType,
    group_id: String,
    /*1、mpsc::UnboundedSender<Message> 是一个异步通道的发送端，用于发送消息。
    2、其是一个多生产者、单消费者的异步通道的发送端，可以无限制地发送 Message 类型的数据。
    3、允许你从外部异步地向某个客户端的 WebSocket 连接发送消息。
    4、UnboundedSender是一个无界通道，可以发送任意数量的消息，而不需要担心缓冲区溢出。
    后续使用需要监控这个通道，观察其是否有可能内存爆炸。是否有定时清理等功能。如果没有是不是需要考虑使用有界通道，需要了解细节
    */
    tx: mpsc::UnboundedSender<Message>,
}

type Connections = Arc<Mutex<HashMap<String, Connection>>>;

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

    let connections: Connections = Arc::new(Mutex::new(HashMap::new()));

    println!("等待客户端连接...");
    while let Ok((stream, addr)) = listener.accept().await {
        println!("接受新连接: {}", addr);
        info!("接受新连接: {}", addr);
        let connections = connections.clone();//这里的clone是Arc的克隆，其实是增加跨线程的引用计数，而不是深拷贝
        tokio::spawn(handle_connection(stream, connections));
    }
}

async fn handle_connection(stream: TcpStream, connections: Connections) {
    let addr = stream.peer_addr().expect("无法获取对等地址");
    
    let ws_stream = match tokio_tungstenite::accept_async(stream).await {
        Ok(stream) => stream,
        Err(e) => {
            error!("WebSocket握手失败: {}", e);
            println!("WebSocket握手失败: {}", e);
            return;
        }
    };
    
    println!("WebSocket连接建立: {}", addr);
    info!("WebSocket连接建立: {}", addr);
    
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    
    // 等待客户端发送初始化消息，包含客户端类型和组ID
    let init_msg = match ws_receiver.next().await {
        Some(Ok(msg)) => msg,
        Some(Err(e)) => {
            println!("接收初始化消息失败: {}", e);
            return;
        },
        None => {
            println!("连接关闭，未收到初始化消息");
            return;
        }
    };
    
    let text = match init_msg.to_text() {
        Ok(text) => text,
        Err(e) => {
            println!("初始化消息不是文本: {}", e);
            return;
        }
    };
    
    println!("收到初始化消息: {}", text);
    info!("收到初始化消息: {}", text);
    
    let init_msg: WrappedMessage = match serde_json::from_str(text) {
        Ok(msg) => msg,
        Err(e) => {
            println!("解析初始化消息失败: {}", e);
            return;
        }
    };
    
    let client_type = match init_msg.sender_type.as_str() {
        "center" => ClientType::Center,
        "local" => ClientType::Local,
        _ => {
            error!("未知的客户端类型: {}", init_msg.sender_type);
            println!("未知的客户端类型: {}", init_msg.sender_type);
            return;
        }
    };
    
    let group_id = init_msg.group_id.clone();
    let client_id = Uuid::new_v4().to_string();
    let client_id_for_sender = client_id.clone();
    let client_id_for_select = client_id.clone();
    
    // 创建消息通道 - 这里是关键，确保通道被正确创建和使用
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    
    println!("为客户端 {} 创建了消息通道", client_id);
    
    // 存储连接信息
    {
        let mut connections_lock = connections.lock().unwrap();
        connections_lock.insert(client_id.clone(), Connection {
            client_type: client_type.clone(),
            group_id: group_id.clone(),
            tx: tx.clone(), // 使用克隆的tx
        });
        
        // 打印当前所有连接信息，用于调试
        println!("当前连接数: {}", connections_lock.len());
        for (id, conn) in connections_lock.iter() {
            println!("  客户端ID: {}, 类型: {:?}, 组ID: {}", id, conn.client_type, conn.group_id);
        }
    }
    
    // 打印设备成功连接的消息
    println!("----");
    println!("新客户端连接成功");
    println!("客户端类型：{:?}", client_type);
    println!("客户端ID：{}", client_id);
    println!("组号：{}", group_id);
    println!("----");
    
    info!("客户端注册: 类型={:?}, 组ID={}, 客户端ID={}", client_type, group_id, client_id);
    
    // 向客户端发送确认消息
    let confirm_msg = WrappedMessage {
        group_id: group_id.clone(),
        sender_type: "server".to_string(),
        receiver_type: match client_type {
            ClientType::Center => "center".to_string(),
            ClientType::Local => "local".to_string(),
        },
        content: format!("连接成功，您的ID是: {}", client_id),
    };
    
    if let Ok(json) = serde_json::to_string(&confirm_msg) {
        println!("发送确认消息: {}", json);
        if let Err(e) = ws_sender.send(Message::Text(json)).await {
            error!("发送确认消息失败: {}", e);
            println!("发送确认消息失败: {}", e);
        }
    }
    
    // 处理从websocket接收的消息
    let connections_for_receiver = connections.clone();
    let client_id_clone = client_id.clone();
    
    // 创建从rx到ws_sender的转发任务 - 这个任务负责将发送给该客户端的消息从通道取出并通过WebSocket发送
    let sender_task = tokio::spawn(async move {
        println!("开始监听发送给客户端 {} 的消息...", client_id_for_sender);
        while let Some(msg) = rx.recv().await {
            println!("接收到需要转发到客户端 {} 的消息: {:?}", client_id_for_sender, msg);
            match ws_sender.send(msg).await {
                Ok(_) => println!("消息成功转发到客户端 {}", client_id_for_sender),
                Err(e) => {
                    println!("发送消息到客户端 {} 失败: {}", client_id_for_sender, e);
                    break;
                }
            }
        }
        println!("客户端 {} 的消息发送通道已关闭", client_id_for_sender);
    });
    
    // 处理从客户端接收的消息
    let receiver_task = tokio::spawn(async move {
        println!("开始监听客户端 {} 发送的消息...", client_id_clone);
        
        while let Some(result) = ws_receiver.next().await {
            match result {
                Ok(msg) => {
                    match msg {
                        Message::Text(text) => {
                            println!("\n收到客户端 {} 的消息: {}", client_id_clone, text);
                            match serde_json::from_str::<WrappedMessage>(&text) {
                                Ok(parsed_msg) => {
                                    println!("解析消息成功，准备处理");
                                    
                                    // 添加中间件处理：确保消息有正确的组ID和发送方信息
                                    let processed_msg = process_message(parsed_msg, &client_type, &group_id, &client_id_clone);
                                    
                                    // 转发消息到同组的目标接收方
                                    println!("开始转发消息");
                                    forward_message(processed_msg, &connections_for_receiver, &client_id_clone);
                                    
                                    // 确认消息已经转发
                                    println!("发送确认消息到发送方...");
                                    let confirm_msg = WrappedMessage {
                                        group_id: group_id.clone(),
                                        sender_type: "server".to_string(),
                                        receiver_type: match client_type {
                                            ClientType::Center => "center".to_string(),
                                            ClientType::Local => "local".to_string(),
                                        },
                                        content: "消息已发送".to_string(),
                                    };
                                    
                                    if let Ok(json) = serde_json::to_string(&confirm_msg) {
                                        if let Some(conn) = connections_for_receiver.lock().unwrap().get(&client_id_clone) {
                                            println!("找到发送方的连接，发送确认消息");
                                            if let Err(e) = conn.tx.send(Message::Text(json)) {
                                                println!("发送确认消息失败: {}", e);
                                            } else {
                                                println!("确认消息发送成功");
                                            }
                                        } else {
                                            println!("找不到发送方的连接");
                                        }
                                    }
                                },
                                Err(e) => {
                                    println!("解析消息失败: {}", e);
                                    error!("解析消息失败: {}", e);
                                }
                            }
                        },
                        Message::Binary(data) => {
                            println!("收到二进制消息，长度: {}", data.len());
                        },
                        Message::Ping(_) => {
                            println!("收到ping，发送pong");
                            // 回复pong
                            if let Some(conn) = connections_for_receiver.lock().unwrap().get(&client_id_clone) {
                                if let Err(e) = conn.tx.send(Message::Pong(vec![])) {
                                    println!("发送pong失败: {}", e);
                                }
                            }
                        },
                        Message::Pong(_) => {
                            println!("收到pong");
                        },
                        Message::Close(_) => {
                            println!("收到关闭消息");
                            break;
                        },
                        Message::Frame(_) => {
                            println!("收到帧消息");
                        },
                    }
                },
                Err(e) => {
                    println!("接收消息错误: {}", e);
                    error!("接收消息错误: {}", e);
                    break;
                }
            }
        }
        
        // 连接关闭时，移除客户端
        println!("客户端 {} 连接关闭，移除连接信息", client_id_clone);
        let mut connections_lock = connections_for_receiver.lock().unwrap();
        connections_lock.remove(&client_id_clone);
        println!("客户端断开连接: {}", client_id_clone);
        info!("客户端断开连接: {}", client_id_clone);
    });
    
    // 等待任意一个任务完成
    tokio::select! {
        _ = sender_task => {
            println!("发送任务结束，客户端ID: {}", client_id_for_select);
        },
        _ = receiver_task => {
            println!("接收任务结束，客户端ID: {}", client_id_for_select);
        },
    }
}

// 中间件：处理消息，确保正确的元数据
fn process_message(mut msg: WrappedMessage, client_type: &ClientType, group_id: &str, client_id: &str) -> WrappedMessage {
    println!("处理原始消息: {:?}", msg);
    
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
    
    // 打印处理的消息信息
    println!("----");
    println!("发送方：{}     ID：{}", msg.sender_type, client_id);
    println!("接收方：{}", msg.receiver_type);
    println!("组号：{}", msg.group_id);
    println!("内容：{}", msg.content);
    println!("----");
    
    println!("处理后的消息: {:?}", msg);
    info!("处理消息: 从 {:?} 到 {} 在组 {}", client_type, msg.receiver_type, group_id);
    
    msg
}

// 转发消息到目标接收方
fn forward_message(msg: WrappedMessage, connections: &Connections, sender_id: &str) {
    println!("\n==== 开始消息转发 ====");
    println!("发送方ID: {}", sender_id);
    println!("消息内容: {:?}", msg);
    
    // 将消息JSON缓存，避免多次序列化
    let json_msg = match serde_json::to_string(&msg) {
        Ok(json) => {
            println!("消息序列化成功: {}", json);
            json
        },
        Err(e) => {
            println!("消息序列化失败: {}", e);
            return;
        }
    };

    let connections_lock = match connections.lock() {
        Ok(lock) => lock,
        Err(e) => {
            println!("获取连接锁失败: {:?}", e);
            return;
        }
    };
    
    println!("当前连接数: {}", connections_lock.len());
    println!("所有连接:");
    for (id, conn) in connections_lock.iter() {
        println!("  ID: {}, 类型: {:?}, 组ID: {}", id, conn.client_type, conn.group_id);
    }
    
    let mut found_target = false;
    let target_type = match msg.receiver_type.as_str() {
        "center" => ClientType::Center,
        "local" => ClientType::Local,
        _ => {
            println!("未知的接收方类型: {}", msg.receiver_type);
            return;
        }
    };
    
    println!("寻找接收方，目标类型: {:?}, 组ID: {}", target_type, msg.group_id);
    
    // 创建一个集合存储所有匹配的接收方，不直接在迭代中发送以避免潜在的锁问题
    let mut targets = Vec::new();
    
    for (receiver_id, connection) in connections_lock.iter() {
        // 跳过发送者自己
        if receiver_id == sender_id {
            println!("跳过发送者自己: {}", receiver_id);
            continue;
        }
        
        println!("检查接收方: ID: {}, 类型: {:?}, 组ID: {}", 
                 receiver_id, connection.client_type, connection.group_id);
        println!("是否匹配: 类型: {}, 组ID: {}", 
                 connection.client_type == target_type, connection.group_id == msg.group_id);
        
        // 如果客户端类型和组ID都匹配，则转发消息
        if connection.client_type == target_type && connection.group_id == msg.group_id {
            found_target = true;
            targets.push(receiver_id.clone());
        }
    }
    
    // 释放锁，然后发送消息
    drop(connections_lock);
    
    if found_target {
        println!("找到 {} 个匹配的接收方", targets.len());
        
        for target_id in targets {
            if let Some(conn) = connections.lock().unwrap().get(&target_id) {
                println!("准备发送消息到接收方 {}", target_id);
                let websocket_msg = Message::Text(json_msg.clone());
                match conn.tx.send(websocket_msg) {
                    Ok(_) => println!("消息成功发送到接收方 {}", target_id),
                    Err(e) => println!("发送消息到接收方 {} 失败: {}", target_id, e),
                }
            } else {
                println!("获取接收方 {} 的连接失败", target_id);
            }
        }
    } else {
        println!("警告: 未找到同组的目标接收方！组号: {}, 目标类型: {}", msg.group_id, msg.receiver_type);
    }
    
    println!("==== 消息转发结束 ====\n");
}
