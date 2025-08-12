#!/bin/bash

echo "🚀 启动 SoulBox 认证演示"
echo "========================="

# 检查是否已经有服务在运行
if lsof -Pi :3000 -sTCP:LISTEN -t >/dev/null; then
    echo "⚠️  端口 3000 已被占用，尝试终止现有进程..."
    pkill -f "target.*soulbox" 2>/dev/null || true
    sleep 2
fi

echo "📋 1. 编译项目..."
cargo build --release
if [ $? -ne 0 ]; then
    echo "❌ 编译失败"
    exit 1
fi

echo "📋 2. 启动 SoulBox 服务器（后台运行）..."
export RUST_LOG="soulbox=info,tower_http=debug"
export JWT_SECRET="demo-jwt-secret-change-in-production-environment"

# 启动服务器
cargo run --release > soulbox.log 2>&1 &
SERVER_PID=$!

echo "📋 3. 等待服务器启动..."
sleep 5

# 检查服务器是否成功启动
if ! kill -0 $SERVER_PID 2>/dev/null; then
    echo "❌ 服务器启动失败，查看日志:"
    cat soulbox.log
    exit 1
fi

echo "📋 4. 运行认证 API 演示..."
cargo run --example auth_api_demo

echo "📋 5. 清理：停止服务器..."
kill $SERVER_PID 2>/dev/null || true

echo "✅ 演示完成！"