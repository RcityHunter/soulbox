use anyhow::{Result, Context};
use tokio::net::UnixStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::path::Path;
use std::time::Duration;
use tracing::{info, warn, debug};

use super::protocol::{VsockMessage, ExecutionRequest, ExecutionResponse, StreamEvent};

/// Vsock 客户端（Host 端）
/// 用于与 VM 内部的执行代理通信
pub struct VsockClient {
    /// VM ID
    vm_id: String,
    /// Vsock 设备路径
    vsock_path: String,
    /// 连接超时
    connect_timeout: Duration,
}

impl VsockClient {
    /// 创建新的 Vsock 客户端
    pub fn new(vm_id: String, vsock_path: String) -> Self {
        Self {
            vm_id,
            vsock_path,
            connect_timeout: Duration::from_secs(10),
        }
    }

    /// 连接到 VM 内部的 Vsock 服务器
    async fn connect(&self) -> Result<UnixStream> {
        debug!("Connecting to vsock at: {}", self.vsock_path);
        
        // 等待 vsock 设备就绪
        let start = std::time::Instant::now();
        while start.elapsed() < self.connect_timeout {
            if Path::new(&self.vsock_path).exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // 连接到 Unix socket（Firecracker 将 vsock 暴露为 Unix socket）
        let stream = tokio::time::timeout(
            self.connect_timeout,
            UnixStream::connect(&self.vsock_path)
        )
        .await
        .context("Connection timeout")?
        .context("Failed to connect to vsock")?;

        info!("Connected to VM {} via vsock", self.vm_id);
        Ok(stream)
    }

    /// 发送执行请求并等待响应
    pub async fn execute(&self, request: ExecutionRequest) -> Result<ExecutionResponse> {
        let mut stream = self.connect().await?;
        
        // 发送请求
        let message = VsockMessage::ExecutionRequest(request.clone());
        self.send_message(&mut stream, &message).await?;
        
        // 接收响应
        let response = self.receive_response(&mut stream, &request.execution_id).await?;
        
        // 关闭连接
        let _ = self.send_message(&mut stream, &VsockMessage::Close).await;
        
        Ok(response)
    }

    /// 发送执行请求并流式接收输出
    pub async fn execute_stream<F>(&self, request: ExecutionRequest, mut callback: F) -> Result<ExecutionResponse>
    where
        F: FnMut(StreamEvent) + Send,
    {
        let mut stream = self.connect().await?;
        
        // 发送请求
        let message = VsockMessage::ExecutionRequest(request.clone());
        self.send_message(&mut stream, &message).await?;
        
        // 接收流式事件
        let mut final_response = None;
        loop {
            match self.receive_message(&mut stream).await? {
                VsockMessage::StreamEvent(event) => {
                    // 处理流式事件
                    match &event {
                        StreamEvent::Completed { execution_id, exit_code, execution_time_ms } => {
                            if execution_id == &request.execution_id {
                                // 构建最终响应
                                final_response = Some(ExecutionResponse {
                                    execution_id: execution_id.clone(),
                                    status: super::protocol::ExecutionStatus::Completed,
                                    stdout: String::new(), // 已通过流式事件发送
                                    stderr: String::new(), // 已通过流式事件发送
                                    exit_code: Some(*exit_code),
                                    execution_time_ms: *execution_time_ms,
                                    error: None,
                                });
                            }
                        }
                        StreamEvent::Error { execution_id, error } => {
                            if execution_id == &request.execution_id {
                                final_response = Some(ExecutionResponse {
                                    execution_id: execution_id.clone(),
                                    status: super::protocol::ExecutionStatus::Failed,
                                    stdout: String::new(),
                                    stderr: String::new(),
                                    exit_code: None,
                                    execution_time_ms: 0,
                                    error: Some(error.clone()),
                                });
                            }
                        }
                        _ => {}
                    }
                    
                    // 回调处理事件
                    callback(event);
                    
                    // 如果收到完成或错误事件，退出循环
                    if matches!(&final_response, Some(_)) {
                        break;
                    }
                }
                VsockMessage::ExecutionResponse(response) => {
                    // 非流式响应
                    final_response = Some(response);
                    break;
                }
                VsockMessage::Heartbeat => {
                    debug!("Received heartbeat from VM");
                }
                _ => {
                    warn!("Unexpected message type");
                }
            }
        }
        
        // 关闭连接
        let _ = self.send_message(&mut stream, &VsockMessage::Close).await;
        
        final_response.ok_or_else(|| anyhow::anyhow!("No response received"))
    }

    /// 发送消息
    async fn send_message(&self, stream: &mut UnixStream, message: &VsockMessage) -> Result<()> {
        let data = serde_json::to_vec(message)?;
        let len = data.len() as u32;
        
        // 发送长度（4 字节）
        stream.write_all(&len.to_be_bytes()).await?;
        // 发送数据
        stream.write_all(&data).await?;
        stream.flush().await?;
        
        Ok(())
    }

    /// 接收消息
    async fn receive_message(&self, stream: &mut UnixStream) -> Result<VsockMessage> {
        // 读取长度（4 字节）
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;
        
        // 读取数据
        let mut data = vec![0u8; len];
        stream.read_exact(&mut data).await?;
        
        // 解析消息
        let message = serde_json::from_slice(&data)?;
        Ok(message)
    }

    /// 接收执行响应
    async fn receive_response(&self, stream: &mut UnixStream, execution_id: &str) -> Result<ExecutionResponse> {
        loop {
            match self.receive_message(stream).await? {
                VsockMessage::ExecutionResponse(response) => {
                    if response.execution_id == execution_id {
                        return Ok(response);
                    }
                }
                VsockMessage::Heartbeat => {
                    debug!("Received heartbeat from VM");
                }
                _ => {
                    warn!("Unexpected message type while waiting for response");
                }
            }
        }
    }
}