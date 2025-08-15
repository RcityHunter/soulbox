#!/bin/bash

# 颜色定义
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m'

GRPC_ADDRESS="localhost:9080"

echo -e "\n${BLUE}=== SoulBox gRPC API 详细测试 ===${NC}\n"

# 安装 grpcurl 如果不存在
if ! command -v grpcurl &> /dev/null; then
    echo -e "${YELLOW}grpcurl 未安装，正在安装...${NC}"
    if [[ "$OSTYPE" == "darwin"* ]]; then
        brew install grpcurl
    else
        echo -e "${RED}请手动安装 grpcurl: https://github.com/fullstorydev/grpcurl${NC}"
        exit 1
    fi
fi

echo -e "${BLUE}[gRPC 服务发现]${NC}\n"

# 列出所有服务
echo -e "${YELLOW}可用的 gRPC 服务:${NC}"
grpcurl -plaintext $GRPC_ADDRESS list 2>&1

echo -e "\n${YELLOW}SoulBoxService 方法:${NC}"
grpcurl -plaintext $GRPC_ADDRESS list soulbox.v1.SoulBoxService 2>&1

echo -e "\n${BLUE}[gRPC API 测试]${NC}\n"

# 1. 健康检查
echo -e "${YELLOW}1. 健康检查${NC}"
grpcurl -plaintext -d '{"service":"soulbox"}' \
    $GRPC_ADDRESS soulbox.v1.SoulBoxService/HealthCheck 2>&1 | jq '.' 2>/dev/null || \
    grpcurl -plaintext -d '{"service":"soulbox"}' \
    $GRPC_ADDRESS soulbox.v1.SoulBoxService/HealthCheck

# 2. 创建沙盒
echo -e "\n${YELLOW}2. 创建沙盒${NC}"
sandbox_request='{
  "template_id": "ubuntu-22.04",
  "config": {
    "memory_mb": 512,
    "cpu_cores": 1,
    "enable_internet": true,
    "resource_limits": {
      "memory_mb": 512,
      "cpu_cores": 1
    }
  },
  "environment_variables": {
    "NODE_ENV": "development",
    "DEBUG": "true"
  }
}'

response=$(grpcurl -plaintext -d "$sandbox_request" \
    $GRPC_ADDRESS soulbox.v1.SoulBoxService/CreateSandbox 2>&1)

echo "$response" | jq '.' 2>/dev/null || echo "$response"

# 提取 sandbox_id（如果创建成功）
sandbox_id=$(echo "$response" | grep -o '"sandboxId": "[^"]*"' | cut -d'"' -f4)

if [ -n "$sandbox_id" ]; then
    echo -e "${GREEN}沙盒创建成功！ID: $sandbox_id${NC}"
    
    # 3. 获取沙盒信息
    echo -e "\n${YELLOW}3. 获取沙盒信息${NC}"
    grpcurl -plaintext -d "{\"sandbox_id\": \"$sandbox_id\"}" \
        $GRPC_ADDRESS soulbox.v1.SoulBoxService/GetSandbox 2>&1 | jq '.'
    
    # 4. 执行代码
    echo -e "\n${YELLOW}4. 在沙盒中执行代码${NC}"
    execute_request="{
      \"sandbox_id\": \"$sandbox_id\",
      \"language\": \"python\",
      \"code\": \"print('Hello from SoulBox!')\\nprint('Python execution test')\"
    }"
    
    grpcurl -plaintext -d "$execute_request" \
        $GRPC_ADDRESS soulbox.v1.SoulBoxService/ExecuteCode 2>&1 | jq '.'
fi

# 5. 列出所有沙盒
echo -e "\n${YELLOW}5. 列出所有沙盒${NC}"
grpcurl -plaintext -d '{"page": 1, "page_size": 10}' \
    $GRPC_ADDRESS soulbox.v1.SoulBoxService/ListSandboxes 2>&1 | jq '.'

# 6. 流式执行测试
echo -e "\n${YELLOW}6. 流式代码执行测试${NC}"
if [ -n "$sandbox_id" ]; then
    stream_request="{
      \"sandbox_id\": \"$sandbox_id\",
      \"language\": \"javascript\",
      \"code\": \"for(let i = 1; i <= 3; i++) { console.log('Step ' + i); }\"
    }"
    
    echo "发送流式执行请求..."
    grpcurl -plaintext -d "$stream_request" \
        $GRPC_ADDRESS soulbox.v1.SoulBoxService/StreamExecuteCode 2>&1
fi

echo -e "\n${BLUE}=== 测试完成 ===${NC}"