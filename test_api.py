#!/usr/bin/env python3
"""
SoulBox API 测试脚本
测试核心 REST 和 gRPC API 功能
"""

import json
import requests
import grpc
import sys
import time
from concurrent import futures

# 添加生成的 protobuf 路径
sys.path.append('./target/debug/build')

# REST API 基础 URL
REST_BASE_URL = "http://localhost:8080"
GRPC_ADDRESS = "localhost:9080"

class Colors:
    GREEN = '\033[92m'
    RED = '\033[91m'
    YELLOW = '\033[93m'
    BLUE = '\033[94m'
    END = '\033[0m'

def print_test(name, passed):
    status = f"{Colors.GREEN}✓ PASSED{Colors.END}" if passed else f"{Colors.RED}✗ FAILED{Colors.END}"
    print(f"{name}: {status}")

def test_rest_health():
    """测试 REST API 健康检查端点"""
    try:
        response = requests.get(f"{REST_BASE_URL}/health")
        passed = response.status_code == 200 and response.json()["status"] == "healthy"
        print_test("REST API Health Check", passed)
        if passed:
            print(f"  Response: {json.dumps(response.json(), indent=2)}")
        return passed
    except Exception as e:
        print_test("REST API Health Check", False)
        print(f"  Error: {e}")
        return False

def test_rest_auth_login():
    """测试 REST API 登录端点"""
    try:
        payload = {
            "username": "admin",
            "password": "admin123"
        }
        response = requests.post(f"{REST_BASE_URL}/auth/login", json=payload)
        passed = response.status_code in [200, 401]  # 允许未授权，因为还没有用户
        print_test("REST API Auth Login", passed)
        if response.status_code == 200:
            print(f"  Response: {json.dumps(response.json(), indent=2)}")
        return passed
    except Exception as e:
        print_test("REST API Auth Login", False)
        print(f"  Error: {e}")
        return False

def test_rest_sandboxes():
    """测试 REST API 沙盒端点"""
    try:
        # 创建沙盒
        create_payload = {
            "template_id": "ubuntu-22.04",
            "config": {
                "memory_mb": 512,
                "cpu_cores": 1
            }
        }
        response = requests.post(f"{REST_BASE_URL}/sandboxes", json=create_payload)
        create_passed = response.status_code in [200, 201, 401]  # 可能需要认证
        print_test("REST API Create Sandbox", create_passed)
        
        # 列出沙盒
        response = requests.get(f"{REST_BASE_URL}/sandboxes")
        list_passed = response.status_code in [200, 401]
        print_test("REST API List Sandboxes", list_passed)
        
        return create_passed and list_passed
    except Exception as e:
        print_test("REST API Sandbox Operations", False)
        print(f"  Error: {e}")
        return False

def test_grpc_connection():
    """测试 gRPC 连接"""
    try:
        # 尝试创建 gRPC 通道
        channel = grpc.insecure_channel(GRPC_ADDRESS)
        
        # 简单的连接测试
        try:
            grpc.channel_ready_future(channel).result(timeout=5)
            print_test("gRPC Connection", True)
            print(f"  Successfully connected to gRPC server at {GRPC_ADDRESS}")
            return True
        except grpc.FutureTimeoutError:
            print_test("gRPC Connection", False)
            print(f"  Timeout connecting to gRPC server")
            return False
    except Exception as e:
        print_test("gRPC Connection", False)
        print(f"  Error: {e}")
        return False

def test_websocket_connection():
    """测试 WebSocket 连接"""
    try:
        import websocket
        
        ws = websocket.WebSocket()
        ws.connect("ws://localhost:8080/ws")
        
        # 发送测试消息
        test_msg = json.dumps({
            "type": "ping",
            "payload": {}
        })
        ws.send(test_msg)
        
        # 设置超时接收响应
        ws.settimeout(2)
        response = ws.recv()
        ws.close()
        
        print_test("WebSocket Connection", True)
        print(f"  Response: {response}")
        return True
    except ImportError:
        print_test("WebSocket Connection", False)
        print(f"  Error: websocket-client not installed. Run: pip install websocket-client")
        return False
    except Exception as e:
        print_test("WebSocket Connection", False)
        print(f"  Error: {e}")
        return False

def main():
    print(f"\n{Colors.BLUE}=== SoulBox API 测试 ==={Colors.END}\n")
    
    total_tests = 0
    passed_tests = 0
    
    # REST API 测试
    print(f"{Colors.YELLOW}[REST API 测试]{Colors.END}")
    tests = [
        test_rest_health,
        test_rest_auth_login,
        test_rest_sandboxes
    ]
    
    for test in tests:
        total_tests += 1
        if test():
            passed_tests += 1
        print()
    
    # gRPC 测试
    print(f"\n{Colors.YELLOW}[gRPC API 测试]{Colors.END}")
    total_tests += 1
    if test_grpc_connection():
        passed_tests += 1
    print()
    
    # WebSocket 测试
    print(f"\n{Colors.YELLOW}[WebSocket 测试]{Colors.END}")
    total_tests += 1
    if test_websocket_connection():
        passed_tests += 1
    
    # 总结
    print(f"\n{Colors.BLUE}=== 测试总结 ==={Colors.END}")
    print(f"总测试数: {total_tests}")
    print(f"通过测试: {passed_tests}")
    print(f"失败测试: {total_tests - passed_tests}")
    
    if passed_tests == total_tests:
        print(f"\n{Colors.GREEN}所有测试通过！{Colors.END}")
    else:
        print(f"\n{Colors.RED}部分测试失败，请检查服务器日志。{Colors.END}")

if __name__ == "__main__":
    main()