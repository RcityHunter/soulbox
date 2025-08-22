//! Zero-copy architecture for high performance
//! 
//! This module implements zero-copy patterns to minimize memory allocations
//! and data copying throughout the system.

use bytes::{Bytes, BytesMut};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};

/// Zero-copy execution request using Bytes for immutable data
#[derive(Debug, Clone)]
pub struct ZeroCopyExecutionRequest {
    /// Code content as zero-copy bytes
    pub code: Bytes,
    /// Language identifier (stack allocated)
    pub language: Language,
    /// Static configuration reference
    pub config: Arc<ExecutionConfig>,
}

/// Language enum (stack allocated, no heap allocation)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Python,
    JavaScript,
    TypeScript,
    Rust,
    Go,
    Shell,
}

impl Language {
    pub fn as_str(&self) -> &'static str {
        match self {
            Language::Python => "python",
            Language::JavaScript => "javascript",
            Language::TypeScript => "typescript",
            Language::Rust => "rust",
            Language::Go => "go",
            Language::Shell => "shell",
        }
    }
}

/// Execution configuration (shared via Arc)
#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    pub max_memory: u64,
    pub max_cpu_time: std::time::Duration,
    pub max_wall_time: std::time::Duration,
    pub max_processes: u32,
    pub enable_network: bool,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            max_memory: 512 * 1024 * 1024, // 512MB
            max_cpu_time: std::time::Duration::from_secs(30),
            max_wall_time: std::time::Duration::from_secs(60),
            max_processes: 10,
            enable_network: false,
        }
    }
}

/// Zero-copy stream processor for handling large outputs
pub struct ZeroCopyStreamProcessor {
    buffer: BytesMut,
    chunk_size: usize,
}

impl ZeroCopyStreamProcessor {
    pub fn new(chunk_size: usize) -> Self {
        Self {
            buffer: BytesMut::with_capacity(chunk_size),
            chunk_size,
        }
    }

    /// Process stream data without copying
    pub async fn process_stream<R: AsyncRead + Unpin>(
        &mut self,
        mut reader: R,
    ) -> std::io::Result<Vec<Bytes>> {
        let mut chunks = Vec::new();
        
        loop {
            // Reserve capacity if needed
            if self.buffer.capacity() < self.chunk_size {
                self.buffer.reserve(self.chunk_size - self.buffer.capacity());
            }

            // Read directly into buffer
            let n = reader.read_buf(&mut self.buffer).await?;
            if n == 0 {
                // End of stream
                if !self.buffer.is_empty() {
                    chunks.push(self.buffer.split().freeze());
                }
                break;
            }

            // Split buffer when it reaches chunk size
            if self.buffer.len() >= self.chunk_size {
                chunks.push(self.buffer.split_to(self.chunk_size).freeze());
            }
        }

        Ok(chunks)
    }
}

/// Zero-copy result wrapper
#[derive(Debug, Clone)]
pub struct ZeroCopyResult {
    /// Output data as immutable bytes
    pub stdout: Bytes,
    pub stderr: Bytes,
    pub exit_code: i32,
    pub execution_time: std::time::Duration,
}

impl ZeroCopyResult {
    /// Convert to owned strings only when necessary
    pub fn stdout_string(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    pub fn stderr_string(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

/// Zero-copy file handler for efficient file operations
pub struct ZeroCopyFileHandler;

impl ZeroCopyFileHandler {
    /// Read file into zero-copy bytes
    pub async fn read_file(path: &std::path::Path) -> std::io::Result<Bytes> {
        use tokio::fs::File;
        
        let mut file = File::open(path).await?;
        let metadata = file.metadata().await?;
        let file_size = metadata.len() as usize;
        
        let mut buffer = BytesMut::with_capacity(file_size);
        file.read_buf(&mut buffer).await?;
        
        Ok(buffer.freeze())
    }

    /// Write bytes to file without copying
    pub async fn write_file(path: &std::path::Path, data: &Bytes) -> std::io::Result<()> {
        use tokio::fs::File;
        
        let mut file = File::create(path).await?;
        file.write_all(data).await?;
        file.sync_all().await?;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_copy_request() {
        let code = Bytes::from_static(b"print('Hello, World!')");
        let config = Arc::new(ExecutionConfig::default());
        
        let request = ZeroCopyExecutionRequest {
            code: code.clone(),
            language: Language::Python,
            config: config.clone(),
        };

        // Clone should be cheap (no deep copy)
        let request2 = request.clone();
        assert_eq!(request.code.len(), request2.code.len());
    }

    #[tokio::test]
    async fn test_stream_processor() {
        use std::io::Cursor;
        
        let data = b"Hello, World! This is a test of zero-copy streaming.";
        let cursor = Cursor::new(data);
        
        let mut processor = ZeroCopyStreamProcessor::new(16);
        let chunks = processor.process_stream(cursor).await.unwrap();
        
        assert!(!chunks.is_empty());
        
        // Verify data integrity
        let combined: Vec<u8> = chunks.iter().flat_map(|b| b.to_vec()).collect();
        assert_eq!(&combined[..], data);
    }
}