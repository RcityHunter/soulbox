//! E2B Client - Client library for E2B-compatible API

use crate::e2b::models::*;
use crate::e2b::E2BError;
use crate::error::Result;
use reqwest::{Client, Response};
use serde::de::DeserializeOwned;
use std::time::Duration;
use tracing::debug;

/// E2B API client
pub struct E2BClient {
    client: Client,
    base_url: String,
    api_key: Option<String>,
}

impl E2BClient {
    /// Create a new E2B client
    pub fn new(base_url: impl Into<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");
        
        Self {
            client,
            base_url: base_url.into(),
            api_key: None,
        }
    }
    
    /// Set API key for authentication
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }
    
    /// Create a new sandbox
    pub async fn create_sandbox(
        &self,
        request: &E2BCreateSandboxRequest,
    ) -> Result<E2BCreateSandboxResponse> {
        let url = format!("{}/sandboxes", self.base_url);
        let response = self.post(&url, request).await?;
        self.parse_response(response).await
    }
    
    /// Get sandbox by ID
    pub async fn get_sandbox(&self, sandbox_id: &str) -> Result<E2BSandbox> {
        let url = format!("{}/sandboxes/{}", self.base_url, sandbox_id);
        let response = self.get(&url).await?;
        self.parse_response(response).await
    }
    
    /// Delete sandbox
    pub async fn delete_sandbox(&self, sandbox_id: &str) -> Result<()> {
        let url = format!("{}/sandboxes/{}", self.base_url, sandbox_id);
        let response = self.delete(&url).await?;
        
        if response.status().is_success() {
            Ok(())
        } else {
            let error: E2BErrorResponse = self.parse_response(response).await?;
            Err(E2BError::ClientError(error.error.message).into())
        }
    }
    
    /// Run process in sandbox
    pub async fn run_process(
        &self,
        sandbox_id: &str,
        request: &E2BRunProcessRequest,
    ) -> Result<E2BRunProcessResponse> {
        let url = format!("{}/sandboxes/{}/processes", self.base_url, sandbox_id);
        let response = self.post(&url, request).await?;
        self.parse_response(response).await
    }
    
    /// Upload file to sandbox
    pub async fn upload_file(
        &self,
        sandbox_id: &str,
        request: &E2BUploadFileRequest,
    ) -> Result<()> {
        let url = format!("{}/sandboxes/{}/files", self.base_url, sandbox_id);
        let response = self.post(&url, request).await?;
        
        if response.status().is_success() {
            Ok(())
        } else {
            let error: E2BErrorResponse = self.parse_response(response).await?;
            Err(E2BError::ClientError(error.error.message).into())
        }
    }
    
    /// Download file from sandbox
    pub async fn download_file(
        &self,
        sandbox_id: &str,
        path: &str,
    ) -> Result<E2BDownloadFileResponse> {
        let url = format!(
            "{}/sandboxes/{}/files?path={}",
            self.base_url,
            sandbox_id,
            urlencoding::encode(path)
        );
        let response = self.get(&url).await?;
        self.parse_response(response).await
    }
    
    /// List files in sandbox
    pub async fn list_files(
        &self,
        sandbox_id: &str,
        path: &str,
    ) -> Result<E2BListFilesResponse> {
        let url = format!(
            "{}/sandboxes/{}/files/list?path={}",
            self.base_url,
            sandbox_id,
            urlencoding::encode(path)
        );
        let response = self.get(&url).await?;
        self.parse_response(response).await
    }
    
    /// List available templates
    pub async fn list_templates(
        &self,
        page: usize,
        per_page: usize,
    ) -> Result<E2BListTemplatesResponse> {
        let url = format!(
            "{}/templates?page={}&per_page={}",
            self.base_url,
            page,
            per_page
        );
        let response = self.get(&url).await?;
        self.parse_response(response).await
    }
    
    /// Health check
    pub async fn health(&self) -> Result<E2BHealthResponse> {
        let url = format!("{}/health", self.base_url);
        let response = self.get(&url).await?;
        self.parse_response(response).await
    }
    
    /// Send GET request
    async fn get(&self, url: &str) -> Result<Response> {
        debug!("GET {}", url);
        
        let mut request = self.client.get(url);
        if let Some(api_key) = &self.api_key {
            request = request.header("X-API-Key", api_key);
        }
        
        request
            .send()
            .await
            .map_err(|e| E2BError::ClientError(e.to_string()).into())
    }
    
    /// Send POST request
    async fn post<T: serde::Serialize>(
        &self,
        url: &str,
        body: &T,
    ) -> Result<Response> {
        debug!("POST {}", url);
        
        let mut request = self.client.post(url);
        if let Some(api_key) = &self.api_key {
            request = request.header("X-API-Key", api_key);
        }
        
        request
            .json(body)
            .send()
            .await
            .map_err(|e| E2BError::ClientError(e.to_string()).into())
    }
    
    /// Send DELETE request
    async fn delete(&self, url: &str) -> Result<Response> {
        debug!("DELETE {}", url);
        
        let mut request = self.client.delete(url);
        if let Some(api_key) = &self.api_key {
            request = request.header("X-API-Key", api_key);
        }
        
        request
            .send()
            .await
            .map_err(|e| E2BError::ClientError(e.to_string()).into())
    }
    
    /// Parse response JSON
    async fn parse_response<T: DeserializeOwned>(
        &self,
        response: Response,
    ) -> Result<T> {
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| E2BError::ClientError(e.to_string()))?;
        
        if status.is_success() {
            serde_json::from_str(&text)
                .map_err(|e| E2BError::ClientError(format!("Failed to parse response: {}", e)).into())
        } else {
            // Try to parse as error response
            if let Ok(error_response) = serde_json::from_str::<E2BErrorResponse>(&text) {
                Err(E2BError::ClientError(error_response.error.message).into())
            } else {
                Err(E2BError::ClientError(format!("Request failed with status {}: {}", status, text)).into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_client_creation() {
        let client = E2BClient::new("http://localhost:49982");
        assert!(client.api_key.is_none());
        
        let client_with_key = client.with_api_key("test-key");
        assert_eq!(client_with_key.api_key, Some("test-key".to_string()));
    }
}