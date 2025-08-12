use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// VM 内部代码执行请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRequest {
    /// 唯一执行 ID
    pub execution_id: String,
    /// 编程语言
    pub language: String,
    /// 要执行的代码
    pub code: String,
    /// 执行超时（秒）
    pub timeout_seconds: u64,
    /// 环境变量
    pub env_vars: HashMap<String, String>,
    /// 工作目录
    pub working_dir: Option<String>,
    /// 是否流式输出
    pub stream_output: bool,
}

/// VM 内部代码执行响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResponse {
    /// 执行 ID
    pub execution_id: String,
    /// 执行状态
    pub status: ExecutionStatus,
    /// 标准输出
    pub stdout: String,
    /// 标准错误
    pub stderr: String,
    /// 退出码
    pub exit_code: Option<i32>,
    /// 执行时间（毫秒）
    pub execution_time_ms: u64,
    /// 错误信息
    pub error: Option<String>,
}

/// 执行状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ExecutionStatus {
    /// 等待执行
    Pending,
    /// 正在执行
    Running,
    /// 执行完成
    Completed,
    /// 执行失败
    Failed,
    /// 超时
    TimedOut,
}

/// 流式输出事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamEvent {
    /// 输出数据
    Output {
        execution_id: String,
        stream_type: StreamType,
        data: String,
    },
    /// 执行开始
    Started {
        execution_id: String,
    },
    /// 执行完成
    Completed {
        execution_id: String,
        exit_code: i32,
        execution_time_ms: u64,
    },
    /// 执行错误
    Error {
        execution_id: String,
        error: String,
    },
}

/// 输出流类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamType {
    Stdout,
    Stderr,
}

/// Vsock 消息包装
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VsockMessage {
    /// 执行请求
    ExecutionRequest(ExecutionRequest),
    /// 执行响应
    ExecutionResponse(ExecutionResponse),
    /// 流式事件
    StreamEvent(StreamEvent),
    /// 心跳
    Heartbeat,
    /// 关闭连接
    Close,
}