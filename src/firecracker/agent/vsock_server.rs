use anyhow::{Result, Context};
use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::sync::Arc;
use std::path::Path;
use tracing::{info, warn, error, debug};

use super::protocol::{VsockMessage, ExecutionRequest, StreamEvent};
use super::executor::CodeExecutor;

/// Vsock 服务器（Guest VM 端）
/// 在 VM 内部运行，接收并执行代码
pub struct VsockServer {
    /// 监听地址
    socket_path: String,
    /// 代码执行器
    executor: Arc<CodeExecutor>,
}

impl VsockServer {
    /// 创建新的 Vsock 服务器
    pub fn new(socket_path: String) -> Self {
        Self {
            socket_path,
            executor: Arc::new(CodeExecutor::new()),
        }
    }

    /// 启动服务器
    pub async fn start(&self) -> Result<()> {
        // 删除旧的 socket 文件
        if Path::new(&self.socket_path).exists() {
            std::fs::remove_file(&self.socket_path)?;
        }

        // 创建 Unix socket 监听器
        let listener = UnixListener::bind(&self.socket_path)
            .context("Failed to bind vsock server")?;
        
        info!("Vsock server listening on: {}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let executor = self.executor.clone();
                    
                    // 为每个连接创建新任务
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, executor).await {
                            error!("Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }
}

/// 处理单个连接
async fn handle_connection(
    mut stream: UnixStream,
    executor: Arc<CodeExecutor>,
) -> Result<()> {
    debug!("New vsock connection established");

    loop {
        match receive_message(&mut stream).await {
            Ok(VsockMessage::ExecutionRequest(request)) => {
                info!("Received execution request: {}", request.execution_id);
                
                if request.stream_output {
                    // 流式执行
                    handle_stream_execution(&mut stream, request, executor.clone()).await?;
                } else {
                    // 普通执行
                    let response = executor.execute(request).await;
                    send_message(&mut stream, &VsockMessage::ExecutionResponse(response)).await?;
                }
            }
            Ok(VsockMessage::Heartbeat) => {
                debug!("Received heartbeat");
                send_message(&mut stream, &VsockMessage::Heartbeat).await?;
            }
            Ok(VsockMessage::Close) => {
                debug!("Client requested connection close");
                break;
            }
            Ok(_) => {
                warn!("Unexpected message type");
            }
            Err(e) => {
                error!("Failed to receive message: {}", e);
                break;
            }
        }
    }

    Ok(())
}

/// 处理流式执行
async fn handle_stream_execution(
    stream: &mut UnixStream,
    request: ExecutionRequest,
    executor: Arc<CodeExecutor>,
) -> Result<()> {
    let execution_id = request.execution_id.clone();
    
    // 发送开始事件
    send_message(
        stream,
        &VsockMessage::StreamEvent(StreamEvent::Started {
            execution_id: execution_id.clone(),
        }),
    )
    .await?;

    // 创建输出通道
    let (tx, mut rx) = tokio::sync::mpsc::channel(100);
    
    // 启动执行任务
    let executor_clone = executor.clone();
    let request_clone = request.clone();
    let execution_task = tokio::spawn(async move {
        executor_clone.execute_stream(request_clone, tx).await
    });

    // 转发输出事件
    while let Some(event) = rx.recv().await {
        send_message(stream, &VsockMessage::StreamEvent(event)).await?;
    }

    // 等待执行完成
    match execution_task.await {
        Ok(Ok(response)) => {
            // 发送完成事件
            send_message(
                stream,
                &VsockMessage::StreamEvent(StreamEvent::Completed {
                    execution_id: execution_id.clone(),
                    exit_code: response.exit_code.unwrap_or(-1),
                    execution_time_ms: response.execution_time_ms,
                }),
            )
            .await?;
        }
        Ok(Err(e)) => {
            // 发送错误事件
            send_message(
                stream,
                &VsockMessage::StreamEvent(StreamEvent::Error {
                    execution_id: execution_id.clone(),
                    error: e.to_string(),
                }),
            )
            .await?;
        }
        Err(e) => {
            // 任务崩溃
            send_message(
                stream,
                &VsockMessage::StreamEvent(StreamEvent::Error {
                    execution_id: execution_id.clone(),
                    error: format!("Execution task failed: {}", e),
                }),
            )
            .await?;
        }
    }

    Ok(())
}

/// 发送消息
async fn send_message(stream: &mut UnixStream, message: &VsockMessage) -> Result<()> {
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
async fn receive_message(stream: &mut UnixStream) -> Result<VsockMessage> {
    // 读取长度（4 字节）
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    
    // 防止过大的消息
    if len > 10 * 1024 * 1024 {
        return Err(anyhow::anyhow!("Message too large: {} bytes", len));
    }
    
    // 读取数据
    let mut data = vec![0u8; len];
    stream.read_exact(&mut data).await?;
    
    // 解析消息
    let message = serde_json::from_slice(&data)?;
    Ok(message)
}