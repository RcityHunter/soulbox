# SoulBox User Guide

## Overview

SoulBox is a high-performance, secure code execution sandbox designed for AI applications. It provides isolated environments for running code safely with comprehensive monitoring, snapshot capabilities, and advanced recovery mechanisms.

## Quick Start

### Prerequisites

- Docker Engine 20.10+ 
- Rust 1.75+ (for building from source)
- PostgreSQL 13+ or SQLite (for data persistence)
- Linux kernel 4.14+ (for optimal security features)

### Installation

#### Using Docker Compose (Recommended)

```bash
git clone https://github.com/RcityHunter/soulbox.git
cd soulbox
docker-compose up -d
```

#### Building from Source

```bash
git clone https://github.com/RcityHunter/soulbox.git
cd soulbox
cargo build --release
./target/release/soulbox
```

### Configuration

SoulBox can be configured via the `soulbox.toml` file or environment variables.

#### Basic Configuration

```toml
[server]
host = "0.0.0.0"
port = 8080
workers = 4

[security]
enable_isolation = true
max_execution_time = 300
memory_limit_mb = 512

[database]
url = "sqlite://soulbox.db"
max_connections = 20

[monitoring]
enabled = true
metrics_endpoint = "/metrics"
log_level = "info"
```

#### Environment Variables

```bash
export SOULBOX_HOST="0.0.0.0"
export SOULBOX_PORT="8080"
export SOULBOX_DATABASE_URL="postgresql://user:pass@localhost/soulbox"
export SOULBOX_LOG_LEVEL="debug"
```

## Core Features

### 1. Code Execution

SoulBox supports multiple programming languages with secure isolation:

#### Supported Languages
- Python 3.8+
- Node.js 16+
- Rust 1.75+
- Go 1.20+
- Java 11+

#### Basic Execution

```bash
# Via HTTP API
curl -X POST http://localhost:8080/api/execute \
  -H "Content-Type: application/json" \
  -d '{
    "language": "python",
    "code": "print(\"Hello, World!\")",
    "timeout": 30
  }'
```

#### Using the CLI

```bash
# Execute Python code
soulbox-cli exec --language python --code "print('Hello, World!')"

# Execute from file
soulbox-cli exec --language python --file script.py

# With custom resource limits
soulbox-cli exec --language python --file script.py \
  --memory-limit 256M --cpu-limit 0.5 --timeout 60
```

### 2. Sandbox Management

#### Creating Sandboxes

```bash
# Create a new sandbox
soulbox-cli create --name my-sandbox --runtime python --template basic

# List available templates
soulbox-cli templates list

# Create with custom configuration
soulbox-cli create --name my-sandbox --runtime python \
  --memory-limit 512M --disk-limit 1G --network isolated
```

#### Managing Sandboxes

```bash
# List all sandboxes
soulbox-cli list

# Get sandbox details
soulbox-cli inspect my-sandbox

# Stop a sandbox
soulbox-cli stop my-sandbox

# Remove a sandbox
soulbox-cli remove my-sandbox
```

### 3. File Management

#### Uploading Files

```bash
# Upload a single file
curl -X POST http://localhost:8080/api/files/upload \
  -F "file=@script.py" \
  -F "sandbox_id=my-sandbox"

# Upload multiple files
soulbox-cli files upload my-sandbox *.py requirements.txt
```

#### Managing Files

```bash
# List files in sandbox
soulbox-cli files list my-sandbox

# Download files from sandbox
soulbox-cli files download my-sandbox output.txt

# Delete files
soulbox-cli files delete my-sandbox temp.txt
```

### 4. Snapshots and Recovery

#### Creating Snapshots

```bash
# Create a full snapshot
soulbox-cli snapshot create my-sandbox --name "after-setup" \
  --description "Sandbox after initial setup"

# Create incremental snapshot
soulbox-cli snapshot create my-sandbox --name "after-changes" \
  --type incremental --base "after-setup"

# List snapshots
soulbox-cli snapshot list my-sandbox
```

#### Restoring from Snapshots

```bash
# Restore to a previous snapshot
soulbox-cli snapshot restore my-sandbox after-setup

# Restore to new sandbox
soulbox-cli snapshot restore my-sandbox after-setup \
  --target new-sandbox-name
```

### 5. Monitoring and Alerts

#### Viewing Metrics

```bash
# Get system metrics
curl http://localhost:8080/metrics

# View dashboard
open http://localhost:8080/dashboard

# Check health status
curl http://localhost:8080/health
```

#### Setting Up Alerts

```bash
# Configure CPU usage alert
soulbox-cli alerts create --name "high-cpu" \
  --metric "cpu_usage_percent" --threshold 80 \
  --operator "gt" --severity warning

# List active alerts
soulbox-cli alerts list

# Acknowledge an alert
soulbox-cli alerts ack alert-id
```

## API Reference

### Authentication

SoulBox supports API key and JWT-based authentication:

```bash
# Using API key
curl -H "X-API-Key: your-api-key" http://localhost:8080/api/execute

# Using JWT token
curl -H "Authorization: Bearer your-jwt-token" http://localhost:8080/api/execute
```

### Core Endpoints

#### Execute Code

```http
POST /api/execute
Content-Type: application/json

{
  "language": "python",
  "code": "print('Hello, World!')",
  "timeout": 30,
  "memory_limit": "256M",
  "files": [
    {
      "name": "input.txt",
      "content": "Hello, World!"
    }
  ]
}
```

#### Create Sandbox

```http
POST /api/sandboxes
Content-Type: application/json

{
  "name": "my-sandbox",
  "runtime": "python",
  "template": "basic",
  "resources": {
    "memory_limit": "512M",
    "cpu_limit": 1.0,
    "disk_limit": "1G"
  }
}
```

#### Upload Files

```http
POST /api/files/upload
Content-Type: multipart/form-data

sandbox_id: my-sandbox
file: [binary data]
```

### WebSocket API

For real-time execution monitoring:

```javascript
const ws = new WebSocket('ws://localhost:8080/ws/execute');

ws.onmessage = function(event) {
  const data = JSON.parse(event.data);
  console.log('Execution output:', data.output);
};

ws.send(JSON.stringify({
  type: 'execute',
  language: 'python',
  code: 'print("Hello, World!")'
}));
```

## Security Features

### Isolation Mechanisms

1. **Container Isolation**: Each execution runs in a separate Docker container
2. **Network Isolation**: Configurable network policies and firewalls
3. **Filesystem Isolation**: Sandboxed filesystem with read-only system areas
4. **Resource Limits**: CPU, memory, and disk quotas per sandbox
5. **Time Limits**: Automatic termination of long-running processes

### Security Best Practices

1. **Use Resource Limits**: Always set appropriate CPU, memory, and time limits
2. **Network Isolation**: Disable network access for untrusted code
3. **Input Validation**: Validate all inputs before execution
4. **Regular Updates**: Keep SoulBox and container images updated
5. **Monitoring**: Enable comprehensive logging and monitoring

### Configuration Examples

#### High Security Mode

```toml
[security]
enable_isolation = true
disable_network = true
max_execution_time = 60
memory_limit_mb = 256
cpu_limit_cores = 0.5
read_only_filesystem = true
enable_seccomp = true
enable_apparmor = true
```

#### Development Mode

```toml
[security]
enable_isolation = true
disable_network = false
max_execution_time = 300
memory_limit_mb = 1024
cpu_limit_cores = 2.0
read_only_filesystem = false
```

## Performance Optimization

### Resource Management

1. **Container Pooling**: Pre-warm containers for faster execution
2. **Image Caching**: Cache container images locally
3. **Snapshot Optimization**: Use incremental snapshots for faster recovery
4. **CPU Optimization**: Enable CPU optimization features

```toml
[performance]
enable_container_pooling = true
pool_size = 10
enable_cpu_optimization = true
target_cpu_utilization = 40
enable_snapshot_compression = true
```

### Monitoring Performance

```bash
# Check system performance
soulbox-cli metrics performance

# View optimization recommendations
soulbox-cli optimize --analyze

# Enable performance profiling
soulbox-cli profile --enable --duration 300
```

## Integration Examples

### Python Integration

```python
import requests

def execute_code(code, language="python"):
    response = requests.post(
        "http://localhost:8080/api/execute",
        json={
            "language": language,
            "code": code,
            "timeout": 30
        },
        headers={"X-API-Key": "your-api-key"}
    )
    return response.json()

result = execute_code("print('Hello from SoulBox!')")
print(result["output"])
```

### Node.js Integration

```javascript
const axios = require('axios');

async function executeCode(code, language = 'python') {
  try {
    const response = await axios.post(
      'http://localhost:8080/api/execute',
      {
        language,
        code,
        timeout: 30
      },
      {
        headers: { 'X-API-Key': 'your-api-key' }
      }
    );
    return response.data;
  } catch (error) {
    console.error('Execution failed:', error.message);
    throw error;
  }
}

executeCode("console.log('Hello from SoulBox!')", 'nodejs')
  .then(result => console.log(result.output))
  .catch(console.error);
```

### cURL Examples

```bash
# Execute Python code
curl -X POST http://localhost:8080/api/execute \
  -H "Content-Type: application/json" \
  -H "X-API-Key: your-api-key" \
  -d '{
    "language": "python",
    "code": "import sys; print(sys.version)",
    "timeout": 30
  }'

# Create sandbox with files
curl -X POST http://localhost:8080/api/sandboxes \
  -H "Content-Type: application/json" \
  -H "X-API-Key: your-api-key" \
  -d '{
    "name": "data-processing",
    "runtime": "python",
    "template": "data-science",
    "files": [
      {
        "name": "data.csv",
        "content": "name,age\nAlice,30\nBob,25"
      }
    ]
  }'
```

## Advanced Features

### Custom Templates

Create custom sandbox templates:

```bash
# Create template from existing sandbox
soulbox-cli template create --from-sandbox my-sandbox --name my-template

# List templates
soulbox-cli template list

# Use custom template
soulbox-cli create --template my-template --name new-sandbox
```

### Batch Execution

Execute multiple code snippets:

```bash
# Batch execution from file
soulbox-cli batch execute --file batch_jobs.json

# Parallel execution
soulbox-cli batch execute --file batch_jobs.json --parallel 4
```

### Plugin System

Extend SoulBox with plugins:

```bash
# Install plugin
soulbox-cli plugin install security-scanner

# List installed plugins
soulbox-cli plugin list

# Configure plugin
soulbox-cli plugin config security-scanner --enable-deep-scan
```

## Best Practices

### Development Workflow

1. **Start with Templates**: Use pre-configured templates for common use cases
2. **Test Locally**: Test code execution locally before deploying
3. **Use Snapshots**: Create snapshots before major changes
4. **Monitor Performance**: Keep an eye on resource usage and optimization recommendations
5. **Regular Backups**: Backup important sandboxes and configurations

### Production Deployment

1. **Resource Planning**: Plan CPU, memory, and storage requirements
2. **High Availability**: Deploy multiple SoulBox instances with load balancing
3. **Monitoring**: Set up comprehensive monitoring and alerting
4. **Security Hardening**: Enable all security features and regular updates
5. **Backup Strategy**: Implement automated backup and disaster recovery

### Troubleshooting

See the [Troubleshooting Guide](troubleshooting.md) for common issues and solutions.

## Support and Community

- **Documentation**: [https://docs.soulbox.dev](https://docs.soulbox.dev)
- **GitHub**: [https://github.com/RcityHunter/soulbox](https://github.com/RcityHunter/soulbox)
- **Issues**: Report bugs and feature requests on GitHub
- **Discussions**: Join community discussions on GitHub Discussions

## License

SoulBox is licensed under the MIT License. See [LICENSE](../LICENSE) for details.