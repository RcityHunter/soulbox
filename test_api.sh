#!/bin/bash

# 颜色定义
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# API 地址
REST_BASE_URL="http://localhost:8080"
GRPC_ADDRESS="localhost:9080"

echo -e "\n${BLUE}=== SoulBox API 测试 ===${NC}\n"

# 测试计数器
total_tests=0
passed_tests=0

# 测试函数
test_endpoint() {
    local name=$1
    local method=$2
    local url=$3
    local data=$4
    local expected_status=$5
    
    ((total_tests++))
    
    echo -e "${YELLOW}测试: ${name}${NC}"
    
    if [ -z "$data" ]; then
        response=$(curl -s -w "\n%{http_code}" -X $method "$url")
    else
        response=$(curl -s -w "\n%{http_code}" -X $method -H "Content-Type: application/json" -d "$data" "$url")
    fi
    
    # 分离响应体和状态码
    body=$(echo "$response" | sed '$d')
    status=$(echo "$response" | tail -n1)
    
    # 检查状态码
    if [[ " $expected_status " =~ " $status " ]]; then
        echo -e "  状态码: ${GREEN}$status ✓${NC}"
        ((passed_tests++))
        
        # 格式化输出 JSON
        if [ -n "$body" ]; then
            echo "  响应:"
            echo "$body" | jq '.' 2>/dev/null || echo "$body"
        fi
    else
        echo -e "  状态码: ${RED}$status ✗ (期望: $expected_status)${NC}"
        if [ -n "$body" ]; then
            echo "  响应: $body"
        fi
    fi
    echo
}

# REST API 测试
echo -e "${BLUE}[REST API 测试]${NC}\n"

# 1. 健康检查
test_endpoint "健康检查" "GET" "$REST_BASE_URL/health" "" "200"

# 2. 登录测试
login_data='{"username":"admin","password":"admin123"}'
test_endpoint "用户登录" "POST" "$REST_BASE_URL/auth/login" "$login_data" "200 401"

# 3. 沙盒创建
sandbox_data='{
  "template_id": "ubuntu-22.04",
  "config": {
    "memory_mb": 512,
    "cpu_cores": 1,
    "enable_internet": true
  },
  "environment_variables": {
    "NODE_ENV": "development"
  }
}'
test_endpoint "创建沙盒" "POST" "$REST_BASE_URL/sandboxes" "$sandbox_data" "200 201 401"

# 4. 列出沙盒
test_endpoint "列出沙盒" "GET" "$REST_BASE_URL/sandboxes" "" "200 401"

# 5. 代码执行测试
execute_data='{
  "sandbox_id": "test-sandbox",
  "language": "python",
  "code": "print(\"Hello from SoulBox!\")"
}'
test_endpoint "执行代码" "POST" "$REST_BASE_URL/execute" "$execute_data" "200 404 401"

# gRPC 测试
echo -e "\n${BLUE}[gRPC 测试]${NC}\n"

((total_tests++))
echo -e "${YELLOW}测试: gRPC 健康检查${NC}"

# 使用 grpcurl 如果可用
if command -v grpcurl &> /dev/null; then
    grpcurl -plaintext -d '{"service":"soulbox"}' $GRPC_ADDRESS soulbox.v1.SoulBoxService/HealthCheck &> /dev/null
    if [ $? -eq 0 ]; then
        echo -e "  gRPC 服务: ${GREEN}正常 ✓${NC}"
        ((passed_tests++))
    else
        echo -e "  gRPC 服务: ${RED}失败 ✗${NC}"
    fi
else
    # 使用 nc 检查端口
    nc -zv localhost 9080 &> /dev/null
    if [ $? -eq 0 ]; then
        echo -e "  gRPC 端口 9080: ${GREEN}开放 ✓${NC}"
        ((passed_tests++))
    else
        echo -e "  gRPC 端口 9080: ${RED}关闭 ✗${NC}"
    fi
fi

# WebSocket 测试
echo -e "\n${BLUE}[WebSocket 测试]${NC}\n"

((total_tests++))
echo -e "${YELLOW}测试: WebSocket 连接${NC}"

# 检查 WebSocket 端点
ws_response=$(curl -s -o /dev/null -w "%{http_code}" -H "Upgrade: websocket" -H "Connection: Upgrade" "$REST_BASE_URL/ws")
if [ "$ws_response" = "426" ] || [ "$ws_response" = "101" ]; then
    echo -e "  WebSocket 端点: ${GREEN}可用 ✓${NC}"
    ((passed_tests++))
else
    echo -e "  WebSocket 端点: ${RED}不可用 ✗ (状态码: $ws_response)${NC}"
fi

# 测试总结
echo -e "\n${BLUE}=== 测试总结 ===${NC}"
echo "总测试数: $total_tests"
echo "通过测试: $passed_tests"
echo "失败测试: $((total_tests - passed_tests))"

if [ $passed_tests -eq $total_tests ]; then
    echo -e "\n${GREEN}所有测试通过！${NC}"
else
    echo -e "\n${RED}部分测试失败，请检查服务器日志。${NC}"
fi

# 显示服务器日志的最后几行
echo -e "\n${BLUE}[服务器日志 (最后10行)]${NC}"
tail -n 10 server.log 2>/dev/null || echo "无法读取服务器日志"