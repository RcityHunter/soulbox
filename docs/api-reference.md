# SoulBox API Reference

## Overview

SoulBox provides both REST and WebSocket APIs for code execution, sandbox management, and monitoring. All API endpoints support JSON request/response format and use standard HTTP status codes.

## Base URL

```
http://localhost:8080/api
```

## Authentication

### API Key Authentication

Include the API key in the request header:

```http
X-API-Key: your-api-key-here
```

### JWT Authentication

Include the JWT token in the authorization header:

```http
Authorization: Bearer your-jwt-token-here
```

### Obtaining API Keys

```bash
# Create a new API key
POST /auth/api-keys
{
  "name": "my-application",
  "permissions": ["execute", "sandbox_read", "sandbox_write"],
  "expires_at": "2024-12-31T23:59:59Z"
}
```

## Code Execution API

### Execute Code

Execute code in a secure sandbox environment.

**Endpoint:** `POST /execute`

**Request Body:**
```json
{
  "language": "python",
  "code": "print('Hello, World!')",
  "timeout": 30,
  "memory_limit": "256M",
  "cpu_limit": 1.0,
  "files": [
    {
      "name": "input.txt",
      "content": "Sample input data"
    }
  ],
  "environment": {
    "CUSTOM_VAR": "value"
  },
  "network_access": false
}
```

**Parameters:**
- `language` (string, required): Programming language (`python`, `nodejs`, `rust`, `go`, `java`)
- `code` (string, required): Source code to execute
- `timeout` (integer, optional): Execution timeout in seconds (default: 30)
- `memory_limit` (string, optional): Memory limit (e.g., "256M", "1G")
- `cpu_limit` (number, optional): CPU limit as fraction of core (0.5 = half core)
- `files` (array, optional): Files to include in the execution environment
- `environment` (object, optional): Environment variables
- `network_access` (boolean, optional): Enable network access (default: false)

**Response:**
```json
{
  "execution_id": "exec_123456",
  "status": "completed",
  "output": "Hello, World!\n",
  "error": null,
  "exit_code": 0,
  "execution_time": 0.125,
  "memory_used": 12345678,
  "files_created": [
    {
      "name": "output.txt",
      "size": 1024
    }
  ]
}
```

**Status Codes:**
- `200`: Execution completed successfully
- `400`: Invalid request parameters
- `401`: Authentication required
- `403`: Insufficient permissions
- `429`: Rate limit exceeded
- `500`: Internal server error

### Get Execution Status

Check the status of an asynchronous execution.

**Endpoint:** `GET /execute/{execution_id}`

**Response:**
```json
{
  "execution_id": "exec_123456",
  "status": "running",
  "started_at": "2023-12-01T10:00:00Z",
  "completed_at": null,
  "progress": 0.5
}
```

### Cancel Execution

Cancel a running execution.

**Endpoint:** `DELETE /execute/{execution_id}`

**Response:**
```json
{
  "execution_id": "exec_123456",
  "status": "cancelled",
  "message": "Execution cancelled successfully"
}
```

## Sandbox Management API

### Create Sandbox

Create a new sandbox environment.

**Endpoint:** `POST /sandboxes`

**Request Body:**
```json
{
  "name": "my-sandbox",
  "runtime": "python",
  "template": "basic",
  "resources": {
    "memory_limit": "512M",
    "cpu_limit": 1.0,
    "disk_limit": "1G"
  },
  "network": {
    "enabled": false,
    "allowed_hosts": ["api.example.com"]
  },
  "environment": {
    "PYTHON_PATH": "/custom/path"
  }
}
```

**Response:**
```json
{
  "sandbox_id": "sb_123456",
  "name": "my-sandbox",
  "status": "creating",
  "created_at": "2023-12-01T10:00:00Z",
  "resources": {
    "memory_limit": "512M",
    "cpu_limit": 1.0,
    "disk_limit": "1G"
  }
}
```

### List Sandboxes

Retrieve a list of sandboxes.

**Endpoint:** `GET /sandboxes`

**Query Parameters:**
- `status` (string, optional): Filter by status (`creating`, `running`, `stopped`, `error`)
- `runtime` (string, optional): Filter by runtime
- `limit` (integer, optional): Maximum number of results (default: 50)
- `offset` (integer, optional): Pagination offset (default: 0)

**Response:**
```json
{
  "sandboxes": [
    {
      "sandbox_id": "sb_123456",
      "name": "my-sandbox",
      "status": "running",
      "runtime": "python",
      "created_at": "2023-12-01T10:00:00Z",
      "last_activity": "2023-12-01T10:15:30Z"
    }
  ],
  "total": 1,
  "limit": 50,
  "offset": 0
}
```

### Get Sandbox Details

Retrieve detailed information about a specific sandbox.

**Endpoint:** `GET /sandboxes/{sandbox_id}`

**Response:**
```json
{
  "sandbox_id": "sb_123456",
  "name": "my-sandbox",
  "status": "running",
  "runtime": "python",
  "template": "basic",
  "created_at": "2023-12-01T10:00:00Z",
  "resources": {
    "memory_limit": "512M",
    "memory_used": "128M",
    "cpu_limit": 1.0,
    "cpu_used": 0.25,
    "disk_limit": "1G",
    "disk_used": "256M"
  },
  "network": {
    "enabled": false,
    "allowed_hosts": []
  },
  "files": [
    {
      "name": "script.py",
      "size": 1024,
      "created_at": "2023-12-01T10:05:00Z"
    }
  ]
}
```

### Update Sandbox

Update sandbox configuration.

**Endpoint:** `PUT /sandboxes/{sandbox_id}`

**Request Body:**
```json
{
  "resources": {
    "memory_limit": "1G",
    "cpu_limit": 2.0
  },
  "environment": {
    "NEW_VAR": "new_value"
  }
}
```

### Delete Sandbox

Delete a sandbox and all its data.

**Endpoint:** `DELETE /sandboxes/{sandbox_id}`

**Query Parameters:**
- `force` (boolean, optional): Force deletion even if running (default: false)

**Response:**
```json
{
  "sandbox_id": "sb_123456",
  "status": "deleted",
  "message": "Sandbox deleted successfully"
}
```

## File Management API

### Upload Files

Upload files to a sandbox.

**Endpoint:** `POST /files/upload`

**Request (multipart/form-data):**
```
sandbox_id: sb_123456
file: [binary data]
path: /path/to/destination (optional)
```

**Response:**
```json
{
  "files": [
    {
      "name": "uploaded_file.py",
      "path": "/sandbox/uploaded_file.py",
      "size": 1024,
      "uploaded_at": "2023-12-01T10:00:00Z"
    }
  ]
}
```

### List Files

List files in a sandbox.

**Endpoint:** `GET /sandboxes/{sandbox_id}/files`

**Query Parameters:**
- `path` (string, optional): Directory path to list (default: root)
- `recursive` (boolean, optional): List files recursively (default: false)

**Response:**
```json
{
  "files": [
    {
      "name": "script.py",
      "path": "/sandbox/script.py",
      "size": 1024,
      "type": "file",
      "created_at": "2023-12-01T10:00:00Z",
      "modified_at": "2023-12-01T10:05:00Z"
    },
    {
      "name": "data",
      "path": "/sandbox/data",
      "type": "directory",
      "created_at": "2023-12-01T10:00:00Z"
    }
  ]
}
```

### Download File

Download a file from a sandbox.

**Endpoint:** `GET /sandboxes/{sandbox_id}/files/{file_path}`

**Response:** Binary file content with appropriate Content-Type header.

### Delete File

Delete a file from a sandbox.

**Endpoint:** `DELETE /sandboxes/{sandbox_id}/files/{file_path}`

**Response:**
```json
{
  "message": "File deleted successfully"
}
```

## Snapshot API

### Create Snapshot

Create a snapshot of a sandbox.

**Endpoint:** `POST /sandboxes/{sandbox_id}/snapshots`

**Request Body:**
```json
{
  "name": "after-setup",
  "description": "Snapshot after initial setup",
  "type": "full",
  "compress": true,
  "include_memory": true,
  "tags": {
    "version": "1.0",
    "environment": "production"
  }
}
```

**Response:**
```json
{
  "snapshot_id": "snap_123456",
  "name": "after-setup",
  "status": "creating",
  "created_at": "2023-12-01T10:00:00Z",
  "estimated_completion": "2023-12-01T10:05:00Z"
}
```

### List Snapshots

List snapshots for a sandbox.

**Endpoint:** `GET /sandboxes/{sandbox_id}/snapshots`

**Response:**
```json
{
  "snapshots": [
    {
      "snapshot_id": "snap_123456",
      "name": "after-setup",
      "status": "completed",
      "type": "full",
      "size": "256M",
      "created_at": "2023-12-01T10:00:00Z",
      "tags": {
        "version": "1.0"
      }
    }
  ]
}
```

### Restore Snapshot

Restore a sandbox from a snapshot.

**Endpoint:** `POST /snapshots/{snapshot_id}/restore`

**Request Body:**
```json
{
  "target_sandbox_id": "sb_789012",
  "restore_options": {
    "include_memory": true,
    "include_files": true,
    "preserve_network": false
  }
}
```

### Delete Snapshot

Delete a snapshot.

**Endpoint:** `DELETE /snapshots/{snapshot_id}`

## Monitoring API

### Get Metrics

Retrieve system metrics in Prometheus format.

**Endpoint:** `GET /metrics`

**Response:** Prometheus metrics format

### Get Health Status

Check system health.

**Endpoint:** `GET /health`

**Response:**
```json
{
  "status": "healthy",
  "version": "1.0.0",
  "uptime": 3600,
  "components": [
    {
      "name": "database",
      "status": "healthy",
      "last_check": "2023-12-01T10:00:00Z"
    },
    {
      "name": "container_manager",
      "status": "healthy",
      "last_check": "2023-12-01T10:00:00Z"
    }
  ]
}
```

### Get Performance Metrics

Retrieve detailed performance metrics.

**Endpoint:** `GET /metrics/performance`

**Query Parameters:**
- `duration` (string, optional): Time range (e.g., "1h", "24h", "7d")
- `component` (string, optional): Filter by component

**Response:**
```json
{
  "cpu_usage": {
    "current": 45.2,
    "average": 38.7,
    "peak": 78.1
  },
  "memory_usage": {
    "current": 67.3,
    "average": 55.8,
    "peak": 89.2
  },
  "container_metrics": {
    "total_containers": 15,
    "running_containers": 12,
    "avg_startup_time": 1.2
  },
  "execution_metrics": {
    "total_executions": 1234,
    "avg_execution_time": 2.5,
    "success_rate": 98.5
  }
}
```

## Alert Management API

### List Alerts

Get active alerts.

**Endpoint:** `GET /alerts`

**Query Parameters:**
- `status` (string, optional): Filter by status (`active`, `acknowledged`, `resolved`)
- `severity` (string, optional): Filter by severity (`info`, `warning`, `critical`, `emergency`)

**Response:**
```json
{
  "alerts": [
    {
      "alert_id": "alert_123456",
      "rule_name": "High CPU Usage",
      "severity": "warning",
      "status": "active",
      "message": "CPU usage is above threshold",
      "triggered_at": "2023-12-01T10:00:00Z",
      "value": 85.2,
      "threshold": 80.0
    }
  ]
}
```

### Acknowledge Alert

Acknowledge an alert.

**Endpoint:** `POST /alerts/{alert_id}/acknowledge`

**Request Body:**
```json
{
  "comment": "Investigating the issue"
}
```

### Resolve Alert

Resolve an alert.

**Endpoint:** `POST /alerts/{alert_id}/resolve`

**Request Body:**
```json
{
  "comment": "Issue has been fixed"
}
```

## Template Management API

### List Templates

Get available sandbox templates.

**Endpoint:** `GET /templates`

**Response:**
```json
{
  "templates": [
    {
      "template_id": "basic-python",
      "name": "Basic Python",
      "description": "Python 3.11 with basic libraries",
      "runtime": "python",
      "version": "1.0",
      "packages": ["numpy", "pandas", "requests"]
    }
  ]
}
```

### Get Template Details

Get detailed information about a template.

**Endpoint:** `GET /templates/{template_id}`

**Response:**
```json
{
  "template_id": "basic-python",
  "name": "Basic Python",
  "description": "Python 3.11 with basic libraries",
  "runtime": "python",
  "version": "1.0",
  "base_image": "python:3.11-slim",
  "packages": ["numpy", "pandas", "requests"],
  "environment": {
    "PYTHONPATH": "/workspace"
  },
  "files": [
    {
      "name": "requirements.txt",
      "content": "numpy==1.21.0\npandas==1.3.0"
    }
  ]
}
```

## WebSocket API

### Real-time Execution

Execute code with real-time output streaming.

**Endpoint:** `ws://localhost:8080/ws/execute`

**Message Format:**
```json
{
  "type": "execute",
  "language": "python",
  "code": "for i in range(5):\n    print(f'Count: {i}')\n    time.sleep(1)",
  "timeout": 30
}
```

**Response Messages:**
```json
{
  "type": "output",
  "data": "Count: 0\n"
}

{
  "type": "output",
  "data": "Count: 1\n"
}

{
  "type": "completion",
  "execution_id": "exec_123456",
  "status": "completed",
  "exit_code": 0,
  "execution_time": 5.2
}
```

### Real-time Monitoring

Monitor system metrics in real-time.

**Endpoint:** `ws://localhost:8080/ws/monitor`

**Message Format:**
```json
{
  "type": "subscribe",
  "metrics": ["cpu_usage", "memory_usage", "active_containers"]
}
```

**Response Messages:**
```json
{
  "type": "metric_update",
  "metric": "cpu_usage",
  "value": 45.2,
  "timestamp": "2023-12-01T10:00:00Z"
}
```

## Error Handling

### Standard Error Response

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Invalid request parameters",
    "details": {
      "field": "language",
      "reason": "Unsupported language: unknown"
    },
    "request_id": "req_123456"
  }
}
```

### Common Error Codes

- `AUTHENTICATION_REQUIRED`: Authentication is required
- `INSUFFICIENT_PERMISSIONS`: User lacks required permissions
- `VALIDATION_ERROR`: Request validation failed
- `RESOURCE_NOT_FOUND`: Requested resource not found
- `RESOURCE_LIMIT_EXCEEDED`: Resource quota exceeded
- `EXECUTION_TIMEOUT`: Code execution timed out
- `EXECUTION_ERROR`: Runtime error during execution
- `SYSTEM_ERROR`: Internal system error

## Rate Limiting

API requests are rate-limited per API key:

- **Standard limits:** 100 requests per minute
- **Execution endpoints:** 20 executions per minute
- **File uploads:** 10 uploads per minute

Rate limit headers are included in responses:

```http
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 95
X-RateLimit-Reset: 1638360000
```

## Pagination

List endpoints support pagination:

**Query Parameters:**
- `limit`: Maximum number of items per page (default: 50, max: 1000)
- `offset`: Number of items to skip
- `cursor`: Cursor-based pagination token (alternative to offset)

**Response Headers:**
```http
X-Pagination-Total: 1234
X-Pagination-Limit: 50
X-Pagination-Offset: 100
X-Pagination-Next: /api/sandboxes?offset=150&limit=50
```

## SDKs and Libraries

### Python

```python
from soulbox import SoulBoxClient

client = SoulBoxClient(
    base_url="http://localhost:8080",
    api_key="your-api-key"
)

result = client.execute(
    language="python",
    code="print('Hello, World!')"
)
print(result.output)
```

### JavaScript/Node.js

```javascript
const { SoulBoxClient } = require('soulbox-sdk');

const client = new SoulBoxClient({
  baseUrl: 'http://localhost:8080',
  apiKey: 'your-api-key'
});

const result = await client.execute({
  language: 'python',
  code: "print('Hello, World!')"
});
console.log(result.output);
```

### cURL Examples

```bash
# Execute code
curl -X POST http://localhost:8080/api/execute \
  -H "Content-Type: application/json" \
  -H "X-API-Key: your-api-key" \
  -d '{
    "language": "python",
    "code": "print(\"Hello, World!\")"
  }'

# Create sandbox
curl -X POST http://localhost:8080/api/sandboxes \
  -H "Content-Type: application/json" \
  -H "X-API-Key: your-api-key" \
  -d '{
    "name": "my-sandbox",
    "runtime": "python"
  }'

# Upload file
curl -X POST http://localhost:8080/api/files/upload \
  -H "X-API-Key: your-api-key" \
  -F "sandbox_id=sb_123456" \
  -F "file=@script.py"
```

This comprehensive API reference covers all major endpoints and functionality of SoulBox. For more detailed examples and advanced usage, see the [User Guide](user-guide.md).