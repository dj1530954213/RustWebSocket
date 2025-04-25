# Rust WebSocket通信示例

本项目是一个基于tokio-tungstenite的多端实时通信系统示例，由三个组件组成：

- **remote**: 服务端，负责处理连接并转发消息
- **center**: 客户端，可以向local发送消息并接收消息
- **local**: 客户端，可以向center发送消息并接收消息

## 特点

- 支持分组功能，允许多组center和local进行隔离通信
- 使用中间件为消息添加组ID、发送方和接收方标识
- 支持双向实时通信
- 使用JSON进行消息序列化和反序列化

## 依赖

- Rust 1.70+
- tokio
- tokio-tungstenite
- serde
- futures-util
- uuid

## 运行方法

1. 首先运行服务器

```bash
cd remote
cargo run
```

2. 运行一个或多个center客户端

```bash
cd center
cargo run
```

3. 运行一个或多个local客户端

```bash
cd local
cargo run
```

4. 按照提示输入相同的组ID（在center和local之间需要匹配）

5. 开始通信！在center和local客户端中输入消息，消息将在彼此之间转发。

## 通信流程

1. 客户端（center/local）连接到服务器
2. 客户端发送初始化消息，包含组ID和客户端类型
3. 服务器为客户端分配ID并记录其类型和组ID
4. 客户端之间发送消息，服务器转发给同组的目标接收方

## 注意事项

- 需要确保center和local使用相同的组ID才能互相通信
- 服务器默认监听127.0.0.1:8080
- 输入"/quit"可以退出客户端

## 消息格式

```json
{
  "group_id": "组ID",
  "sender_type": "发送方类型(center/local)",
  "receiver_type": "接收方类型(center/local)",
  "content": "消息内容"
}
``` 