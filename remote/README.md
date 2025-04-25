# WebSocket消息中转服务器

这是一个用Rust编写的WebSocket消息中转服务器，用于在同一组内的中心设备（Center）和本地设备（Local）之间传递消息。

## 功能特点

- 支持分组通信：每个客户端属于一个特定的组，只能与同组内的客户端通信
- 连接验证：同一组内只允许一个中心设备和一个本地设备连接
- 心跳检测：客户端需定期发送心跳消息保持连接
- 离线通知：当一方断开连接时，服务器会通知同组的另一方
- 消息格式验证和处理：确保消息的格式和内容正确

## 系统架构

服务器采用模块化设计，主要分为以下几个模块：

1. **连接管理模块（Connection Manager）**：
   - 管理客户端连接
   - 验证新连接
   - 处理客户端断开连接

2. **消息处理模块（Message Service）**：
   - 构建不同类型的消息
   - 处理接收到的消息
   - 转发消息到目标客户端

3. **心跳管理模块（Heartbeat Manager）**：
   - 记录和更新客户端心跳
   - 检测超时客户端
   - 处理离线客户端

4. **WebSocket处理模块（WebSocket Handler）**：
   - 处理WebSocket连接
   - 协调其他模块的功能
   - 处理消息的接收和发送

## 消息格式

```json
{
  "group_id": "组ID",
  "sender_type": "发送方类型（center或local）",
  "receiver_type": "接收方类型（center或local）",
  "content": "消息内容",
  "message_type": "消息类型（可选：heartbeat, disconnect等）"
}
```

## 使用方法

1. 启动服务器：

```bash
cargo run
```

2. 客户端连接流程：
   - 建立WebSocket连接
   - 发送初始化消息，包含客户端类型和组ID
   - 接收服务器确认消息
   - 定期发送心跳消息
   - 发送/接收业务消息

## 心跳机制

### 服务器端

- 服务器在30秒内未收到心跳时判定客户端离线
- 服务器会在接收到心跳消息时回复心跳确认
- 服务器会通知同组的另一方，有伙伴离线

### 客户端

- 客户端应每20秒发送一次心跳消息
- 客户端需要处理心跳响应和断开连接通知
- 心跳消息格式：

```json
{
  "group_id": "组ID",
  "sender_type": "客户端类型",
  "receiver_type": "server",
  "content": "心跳",
  "message_type": "heartbeat"
}
```

### 客户端实现示例

在客户端代码中添加心跳发送任务：

```rust
// 启动心跳发送任务
tokio::spawn(async move {
    let mut interval = tokio::time::interval(Duration::from_secs(20));
    
    loop {
        interval.tick().await;
        
        // 构建心跳消息
        let heartbeat_msg = WrappedMessage {
            group_id: group_id.clone(),
            sender_type: client_type.clone(),
            receiver_type: "server".to_string(),
            content: "心跳".to_string(),
            message_type: Some("heartbeat".to_string()),
        };
        
        // 序列化并发送心跳消息
        if let Ok(json) = serde_json::to_string(&heartbeat_msg) {
            if let Err(e) = sender.send(Message::Text(json)).await {
                println!("发送心跳失败: {}", e);
                break;
            }
        }
    }
});
```

处理来自服务器的消息时，需要识别和处理心跳响应和断开连接通知：

```rust
// 处理服务器消息
if let Some(msg_type) = &parsed_msg.message_type {
    match msg_type.as_str() {
        "heartbeat" => {
            println!("收到服务器心跳响应");
        },
        "disconnect" => {
            println!("同组的伙伴已断开连接: {}", parsed_msg.content);
            // 处理伙伴断开连接的逻辑
        },
        _ => {
            // 处理其他类型的消息
        }
    }
}
```

完整的客户端心跳实现示例可参考 `client_heartbeat_example.rs` 文件。 