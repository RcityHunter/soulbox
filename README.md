# SoulBox - High-Performance AI Code Execution Sandbox 🚀

[![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Docker](https://img.shields.io/badge/docker-%230db7ed.svg?style=for-the-badge&logo=docker&logoColor=white)](https://www.docker.com/)
[![gRPC](https://img.shields.io/badge/gRPC-4285F4?style=for-the-badge&logo=google&logoColor=white)](https://grpc.io/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)

[English](#english) | [中文](#中文)

## English

### 🎯 Overview

SoulBox is a high-performance, secure AI code execution sandbox built with Rust, designed as an open-source alternative to E2B. It provides isolated environments for running untrusted code with comprehensive resource management, monitoring, and security features.

### ✨ Key Features

- **🔒 Secure Sandboxing**: Multi-layer isolation with Docker containers and optional Firecracker microVMs
- **⚡ High Performance**: Rust-based implementation with zero-copy operations and efficient resource pooling
- **🌐 Multiple Runtimes**: Support for Python, Node.js, Rust, Go, and more
- **📊 Real-time Monitoring**: Comprehensive metrics collection and resource tracking
- **🔄 WebSocket Support**: Real-time bidirectional communication for interactive sessions
- **🛡️ Security First**: JWT authentication, RBAC, audit logging, and input validation
- **📦 Container Pool**: Pre-warmed containers for instant code execution
- **💾 Persistent Storage**: File system management with quota enforcement
- **🔌 gRPC API**: High-performance RPC interface for service integration

### 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Client Applications                   │
└──────────┬────────────────────────────────┬─────────────┘
           │                                │
           ▼                                ▼
    ┌─────────────┐                  ┌─────────────┐
    │  REST API   │                  │  gRPC API   │
    └──────┬──────┘                  └──────┬──────┘
           │                                 │
           └────────────┬────────────────────┘
                        ▼
         ┌──────────────────────────────┐
         │     SoulBox Core Engine      │
         │  ┌────────────────────────┐  │
         │  │   Container Manager     │  │
         │  ├────────────────────────┤  │
         │  │    Container Pool       │  │
         │  ├────────────────────────┤  │
         │  │   Resource Monitor      │  │
         │  ├────────────────────────┤  │
         │  │   Security Layer        │  │
         │  └────────────────────────┘  │
         └──────────────┬───────────────┘
                        ▼
    ┌─────────────────────────────────────────┐
    │           Container Runtime              │
    │  ┌─────────┐  ┌─────────┐  ┌─────────┐ │
    │  │ Docker  │  │Firecracker│ │Podman   │ │
    │  └─────────┘  └─────────┘  └─────────┘ │
    └──────────────────────────────────────────┘
```

### 📋 Prerequisites

- **Rust**: 1.70+ (with cargo)
- **Docker**: 20.10+ or Podman 4.0+
- **Redis**: 6.0+ (for session management)
- **SurrealDB**: 1.0+ (optional, for data persistence)
- **Linux**: Ubuntu 20.04+ or similar (for Firecracker support)

### 🚀 Quick Start

#### 1. Clone the Repository

```bash
git clone https://github.com/RcityHunter/soulbox.git
cd soulbox
```

#### 2. Set Up Environment Variables

```bash
cp .env.example .env
# Edit .env and set your configuration
vim .env
```

Required environment variables:
```env
# Security (REQUIRED)
JWT_SECRET=your-super-secret-jwt-key-change-this-in-production-minimum-64-chars

# Server Configuration
SERVER_HOST=0.0.0.0
SERVER_PORT=3000
GRPC_PORT=50051

# Database
DATABASE_URL=surrealdb://localhost:8000
REDIS_URL=redis://localhost:6379

# Container Runtime
CONTAINER_RUNTIME=docker  # or 'podman', 'firecracker'
CONTAINER_POOL_SIZE=10
CONTAINER_TIMEOUT=300

# Security
ENABLE_RATE_LIMITING=true
MAX_REQUESTS_PER_MINUTE=100
ENABLE_AUDIT_LOG=true
```

#### 3. Start Dependencies

```bash
# Using Docker Compose
docker-compose up -d redis surrealdb

# Or manually
docker run -d --name redis -p 6379:6379 redis:7-alpine
docker run -d --name surrealdb -p 8000:8000 surrealdb/surrealdb:latest start
```

#### 4. Build and Run

```bash
# Build the project
cargo build --release

# Run database migrations
cargo run --bin migrate

# Start the server
cargo run --release

# Or using the built binary
./target/release/soulbox
```

#### 5. Verify Installation

```bash
# Check health endpoint
curl http://localhost:3000/health

# Test code execution
curl -X POST http://localhost:3000/api/v1/execute \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_JWT_TOKEN" \
  -d '{
    "runtime": "python",
    "code": "print(\"Hello from SoulBox!\")",
    "timeout": 5000
  }'
```

### 🔧 Configuration

#### Configuration File (soulbox.toml)

```toml
[server]
host = "0.0.0.0"
port = 3000
workers = 4

[grpc]
port = 50051
max_message_size = 4194304  # 4MB

[container]
runtime = "docker"
pool_min_size = 5
pool_max_size = 20
default_timeout = 30000  # 30 seconds
max_memory = 512  # MB
max_cpu = 1.0    # cores

[security]
jwt_algorithm = "HS256"
jwt_expiration = 900  # 15 minutes
enable_rate_limiting = true
rate_limit_requests = 100
rate_limit_window = 60  # seconds

[database]
surrealdb_url = "ws://localhost:8000/rpc"
redis_url = "redis://localhost:6379"
connection_pool_size = 10

[monitoring]
enable_metrics = true
metrics_port = 9090
log_level = "info"
```

### 📚 API Documentation

#### REST API Endpoints

##### Authentication
```http
POST /api/v1/auth/login
Content-Type: application/json

{
  "username": "user@example.com",
  "password": "secure_password"
}
```

##### Code Execution
```http
POST /api/v1/execute
Authorization: Bearer <token>
Content-Type: application/json

{
  "runtime": "python",
  "code": "print('Hello World')",
  "timeout": 5000,
  "memory_limit": 256,
  "cpu_limit": 0.5
}
```

##### Container Management
```http
# List containers
GET /api/v1/containers
Authorization: Bearer <token>

# Get container details
GET /api/v1/containers/{container_id}
Authorization: Bearer <token>

# Stop container
DELETE /api/v1/containers/{container_id}
Authorization: Bearer <token>
```

#### WebSocket API

```javascript
// Connect to WebSocket
const ws = new WebSocket('ws://localhost:3000/ws');

// Send code for execution
ws.send(JSON.stringify({
  type: 'execute',
  runtime: 'python',
  code: 'for i in range(5): print(i)',
  session_id: 'unique-session-id'
}));

// Receive output
ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  console.log('Output:', data.output);
};
```

### 🧪 Testing

```bash
# Run all tests
cargo test

# Run tests with coverage
cargo tarpaulin --out Html

# Run integration tests
cargo test --test integration

# Run specific test module
cargo test container::tests

# Run benchmarks
cargo bench
```

### 🔒 Security Considerations

1. **JWT Configuration**: Always use strong, unique JWT secrets (minimum 64 characters)
2. **Container Isolation**: Containers run with minimal privileges and resource limits
3. **Input Validation**: All code input is validated for malicious patterns
4. **Rate Limiting**: Configurable rate limiting to prevent abuse
5. **Audit Logging**: Comprehensive audit trail for all operations
6. **Network Isolation**: Containers run in isolated networks by default

### 🐛 Troubleshooting

#### Common Issues

1. **Docker permission errors**
   ```bash
   # Add user to docker group
   sudo usermod -aG docker $USER
   # Log out and back in
   ```

2. **Port already in use**
   ```bash
   # Find process using port
   lsof -i :3000
   # Kill process or change port in configuration
   ```

3. **JWT_SECRET not set**
   ```bash
   # Generate secure secret
   openssl rand -base64 64
   # Set in environment
   export JWT_SECRET="generated-secret-here"
   ```

4. **Container creation fails**
   ```bash
   # Check Docker service
   systemctl status docker
   # Restart if needed
   sudo systemctl restart docker
   ```

### 📈 Performance Tuning

```toml
# Optimize for high throughput
[performance]
container_pool_min = 20
container_pool_max = 100
container_warmup = true
connection_pool_size = 50
worker_threads = 8

# Enable caching
[cache]
enable = true
redis_cache_ttl = 300
memory_cache_size = 1000
```

### 🤝 Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit your changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

### 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

## 中文

### 🎯 概述

SoulBox 是一个用 Rust 构建的高性能、安全的 AI 代码执行沙箱，作为 E2B 的开源替代方案。它为运行不受信任的代码提供隔离环境，具有全面的资源管理、监控和安全功能。

### ✨ 核心特性

- **🔒 安全沙箱**: 使用 Docker 容器和可选的 Firecracker 微虚拟机的多层隔离
- **⚡ 高性能**: 基于 Rust 的实现，零拷贝操作和高效的资源池
- **🌐 多运行时支持**: 支持 Python、Node.js、Rust、Go 等
- **📊 实时监控**: 全面的指标收集和资源跟踪
- **🔄 WebSocket 支持**: 用于交互式会话的实时双向通信
- **🛡️ 安全优先**: JWT 认证、RBAC、审计日志和输入验证
- **📦 容器池**: 预热容器，实现即时代码执行
- **💾 持久存储**: 带配额强制的文件系统管理
- **🔌 gRPC API**: 用于服务集成的高性能 RPC 接口

### 🚀 快速开始

#### 1. 克隆仓库

```bash
git clone https://github.com/RcityHunter/soulbox.git
cd soulbox
```

#### 2. 配置环境变量

```bash
cp .env.example .env
# 编辑 .env 文件设置配置
vim .env
```

必需的环境变量：
```env
# 安全配置（必需）
JWT_SECRET=你的超级秘密jwt密钥在生产环境中必须更改最少64个字符

# 服务器配置
SERVER_HOST=0.0.0.0
SERVER_PORT=3000
GRPC_PORT=50051

# 数据库
DATABASE_URL=surrealdb://localhost:8000
REDIS_URL=redis://localhost:6379

# 容器运行时
CONTAINER_RUNTIME=docker  # 或 'podman'、'firecracker'
CONTAINER_POOL_SIZE=10
CONTAINER_TIMEOUT=300
```

#### 3. 启动依赖服务

```bash
# 使用 Docker Compose
docker-compose up -d redis surrealdb

# 或手动启动
docker run -d --name redis -p 6379:6379 redis:7-alpine
docker run -d --name surrealdb -p 8000:8000 surrealdb/surrealdb:latest start
```

#### 4. 构建和运行

```bash
# 构建项目
cargo build --release

# 运行数据库迁移
cargo run --bin migrate

# 启动服务器
cargo run --release

# 或使用构建的二进制文件
./target/release/soulbox
```

#### 5. 验证安装

```bash
# 检查健康端点
curl http://localhost:3000/health

# 测试代码执行
curl -X POST http://localhost:3000/api/v1/execute \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_JWT_TOKEN" \
  -d '{
    "runtime": "python",
    "code": "print(\"Hello from SoulBox!\")",
    "timeout": 5000
  }'
```

### 🔧 配置说明

配置文件位于 `soulbox.toml`，支持以下配置项：

- **服务器配置**: 端口、工作线程数等
- **容器配置**: 运行时、池大小、资源限制
- **安全配置**: JWT、速率限制、审计日志
- **数据库配置**: 连接字符串、连接池大小
- **监控配置**: 指标端口、日志级别

### 🧪 测试

```bash
# 运行所有测试
cargo test

# 运行特定模块测试
cargo test container::tests

# 运行集成测试
cargo test --test integration

# 运行基准测试
cargo bench
```

### 🔒 安全注意事项

1. **JWT 配置**: 始终使用强大、唯一的 JWT 密钥（最少 64 个字符）
2. **容器隔离**: 容器以最小权限和资源限制运行
3. **输入验证**: 所有代码输入都经过恶意模式验证
4. **速率限制**: 可配置的速率限制以防止滥用
5. **审计日志**: 所有操作的全面审计跟踪

### 🐛 故障排除

#### 常见问题

1. **Docker 权限错误**
   ```bash
   # 将用户添加到 docker 组
   sudo usermod -aG docker $USER
   # 注销并重新登录
   ```

2. **端口已被占用**
   ```bash
   # 查找使用端口的进程
   lsof -i :3000
   # 终止进程或在配置中更改端口
   ```

3. **JWT_SECRET 未设置**
   ```bash
   # 生成安全密钥
   openssl rand -base64 64
   # 设置环境变量
   export JWT_SECRET="生成的密钥"
   ```

### 📊 项目状态

- **版本**: 0.1.0-alpha
- **完成度**: 35%
- **稳定性**: 开发中
- **生产就绪**: 尚未就绪

### 🤝 贡献

欢迎贡献！请查看我们的[贡献指南](CONTRIBUTING.md)了解详情。

### 📄 许可证

本项目采用 MIT 许可证 - 查看 [LICENSE](LICENSE) 文件了解详情。

### 📞 联系方式

- **GitHub**: [@RcityHunter](https://github.com/RcityHunter)
- **Email**: RainbowcityHunter@gmail.com
- **Issues**: [GitHub Issues](https://github.com/RcityHunter/soulbox/issues)

---

**注意**: 该项目目前处于积极开发阶段，API 可能会发生变化。不建议在生产环境中使用。

---

Built with ❤️ using Rust 🦀