# SoulBox Troubleshooting Guide

## Common Issues and Solutions

### Installation Issues

#### Issue: Docker Connection Failed

**Symptoms:**
```
Error: Failed to connect to Docker daemon
```

**Solutions:**
1. Ensure Docker is running:
   ```bash
   sudo systemctl start docker
   # or on macOS
   open -a Docker
   ```

2. Check Docker daemon status:
   ```bash
   sudo systemctl status docker
   docker version
   ```

3. Verify user permissions:
   ```bash
   sudo usermod -aG docker $USER
   # Log out and back in
   ```

4. Check Docker socket permissions:
   ```bash
   sudo chmod 666 /var/run/docker.sock
   ```

#### Issue: Compilation Errors

**Symptoms:**
```
error: failed to compile soulbox
```

**Solutions:**
1. Update Rust toolchain:
   ```bash
   rustup update
   rustup default stable
   ```

2. Install required system dependencies:
   ```bash
   # Ubuntu/Debian
   sudo apt-get update
   sudo apt-get install build-essential pkg-config libssl-dev

   # CentOS/RHEL
   sudo yum groupinstall "Development Tools"
   sudo yum install openssl-devel
   ```

3. Clear Cargo cache:
   ```bash
   cargo clean
   rm -rf ~/.cargo/registry
   cargo build --release
   ```

### Runtime Issues

#### Issue: Container Startup Timeout

**Symptoms:**
```
Error: Container startup timeout after 30s
```

**Solutions:**
1. Increase startup timeout in configuration:
   ```toml
   [container]
   startup_timeout = 60
   ```

2. Check available system resources:
   ```bash
   free -h
   df -h
   docker system df
   ```

3. Optimize container images:
   ```bash
   # Pull latest base images
   docker pull python:3.11-slim
   docker pull node:18-alpine
   
   # Clean up unused images
   docker image prune -f
   ```

4. Check container logs:
   ```bash
   soulbox-cli logs --container-id <container-id>
   docker logs <container-id>
   ```

#### Issue: Out of Memory Errors

**Symptoms:**
```
Error: Container killed due to OOM
```

**Solutions:**
1. Increase memory limits:
   ```toml
   [security]
   memory_limit_mb = 1024
   ```

2. Monitor memory usage:
   ```bash
   soulbox-cli metrics memory
   curl http://localhost:8080/metrics | grep memory
   ```

3. Optimize code for memory efficiency:
   ```python
   # Use generators instead of lists for large datasets
   def process_data():
       for item in large_dataset:
           yield process_item(item)
   ```

4. Configure swap if needed:
   ```bash
   sudo swapon --show
   sudo fallocate -l 2G /swapfile
   sudo chmod 600 /swapfile
   sudo mkswap /swapfile
   sudo swapon /swapfile
   ```

#### Issue: Execution Timeouts

**Symptoms:**
```
Error: Execution timed out after 30s
```

**Solutions:**
1. Increase timeout limits:
   ```bash
   curl -X POST http://localhost:8080/api/execute \
     -d '{"code": "...", "timeout": 120}'
   ```

2. Optimize code performance:
   ```python
   # Use efficient algorithms and data structures
   import numpy as np  # for numerical operations
   import pandas as pd  # for data processing
   ```

3. Check for infinite loops:
   ```python
   # Add loop counters and break conditions
   count = 0
   while condition and count < MAX_ITERATIONS:
       # ... do work ...
       count += 1
   ```

4. Use asynchronous execution for long-running tasks:
   ```bash
   soulbox-cli exec --async --code "long_running_function()"
   ```

### Network Issues

#### Issue: Network Access Denied

**Symptoms:**
```
Error: Connection refused / Network unreachable
```

**Solutions:**
1. Check network policy configuration:
   ```toml
   [security]
   disable_network = false
   allowed_hosts = ["api.example.com", "*.trusted.com"]
   ```

2. Configure firewall rules:
   ```bash
   # Allow outbound HTTPS
   sudo ufw allow out 443
   
   # Allow specific domains
   sudo ufw allow out to api.example.com port 443
   ```

3. Use network debugging:
   ```bash
   # Test from within container
   soulbox-cli exec --code "
   import subprocess
   result = subprocess.run(['ping', '-c', '1', 'google.com'], 
                          capture_output=True, text=True)
   print(result.stdout)
   "
   ```

4. Check DNS resolution:
   ```bash
   # Add DNS servers to container
   docker run --dns=8.8.8.8 --dns=8.8.4.4 ...
   ```

#### Issue: WebSocket Connection Failures

**Symptoms:**
```
WebSocket connection failed: Connection refused
```

**Solutions:**
1. Check WebSocket endpoint:
   ```javascript
   const ws = new WebSocket('ws://localhost:8080/ws/execute');
   ws.onerror = function(error) {
     console.log('WebSocket error:', error);
   };
   ```

2. Verify reverse proxy configuration:
   ```nginx
   location /ws/ {
     proxy_pass http://localhost:8080;
     proxy_http_version 1.1;
     proxy_set_header Upgrade $http_upgrade;
     proxy_set_header Connection "upgrade";
   }
   ```

3. Check firewall settings:
   ```bash
   sudo ufw allow 8080/tcp
   sudo iptables -L | grep 8080
   ```

### Performance Issues

#### Issue: Slow Execution Performance

**Symptoms:**
- High execution times
- High CPU usage
- Slow response times

**Solutions:**
1. Enable CPU optimization:
   ```toml
   [performance]
   enable_cpu_optimization = true
   target_cpu_utilization = 40
   ```

2. Check system resource usage:
   ```bash
   htop
   iotop
   soulbox-cli metrics performance
   ```

3. Optimize container resources:
   ```toml
   [container]
   cpu_limit = 2.0
   memory_limit_mb = 2048
   enable_container_pooling = true
   pool_size = 5
   ```

4. Use performance profiling:
   ```bash
   soulbox-cli profile --enable --duration 300
   soulbox-cli profile --report
   ```

#### Issue: High Memory Usage

**Symptoms:**
```
System memory usage > 90%
```

**Solutions:**
1. Monitor memory usage patterns:
   ```bash
   soulbox-cli metrics memory --watch
   curl http://localhost:8080/metrics | grep -E "(memory|cache)"
   ```

2. Configure garbage collection:
   ```toml
   [performance]
   enable_gc_optimization = true
   gc_interval_seconds = 60
   ```

3. Implement memory cleanup:
   ```bash
   # Clean up old containers
   docker container prune -f
   
   # Clean up unused images
   docker image prune -a -f
   
   # Clean up build cache
   docker builder prune -f
   ```

4. Configure memory limits per container:
   ```toml
   [security]
   memory_limit_mb = 512
   swap_limit_mb = 256
   ```

### Database Issues

#### Issue: Database Connection Failed

**Symptoms:**
```
Error: database connection failed
```

**Solutions:**
1. Check database status:
   ```bash
   # PostgreSQL
   sudo systemctl status postgresql
   sudo -u postgres psql -c "SELECT 1;"
   
   # SQLite
   sqlite3 soulbox.db ".schema"
   ```

2. Verify connection string:
   ```toml
   [database]
   url = "postgresql://user:password@localhost:5432/soulbox"
   # or
   url = "sqlite://soulbox.db"
   ```

3. Check database permissions:
   ```sql
   -- PostgreSQL
   GRANT ALL PRIVILEGES ON DATABASE soulbox TO soulbox_user;
   
   -- Check user permissions
   \du
   ```

4. Run database migrations:
   ```bash
   soulbox-cli migrate --up
   ```

#### Issue: Database Migration Errors

**Symptoms:**
```
Error: migration failed at version 003
```

**Solutions:**
1. Check migration status:
   ```bash
   soulbox-cli migrate --status
   ```

2. Fix corrupted migrations:
   ```bash
   # Rollback to previous version
   soulbox-cli migrate --down --to 002
   
   # Re-run migrations
   soulbox-cli migrate --up
   ```

3. Reset database (development only):
   ```bash
   soulbox-cli migrate --reset
   soulbox-cli migrate --up
   ```

### Monitoring and Alerting Issues

#### Issue: Metrics Not Appearing

**Symptoms:**
- Empty metrics endpoint
- Missing dashboard data

**Solutions:**
1. Enable metrics collection:
   ```toml
   [monitoring]
   enabled = true
   collection_interval = 15
   ```

2. Check metrics endpoint:
   ```bash
   curl http://localhost:8080/metrics
   curl http://localhost:8080/health
   ```

3. Verify Prometheus configuration:
   ```yaml
   # prometheus.yml
   scrape_configs:
     - job_name: 'soulbox'
       static_configs:
         - targets: ['localhost:8080']
       metrics_path: '/metrics'
   ```

4. Check logs for errors:
   ```bash
   soulbox-cli logs --level error --component monitoring
   ```

#### Issue: Alerts Not Triggering

**Symptoms:**
- No alert notifications
- Missing alert events

**Solutions:**
1. Check alert configuration:
   ```bash
   soulbox-cli alerts list
   soulbox-cli alerts test --rule high-cpu
   ```

2. Verify notification channels:
   ```toml
   [alerts]
   enabled = true
   
   [[alerts.channels]]
   type = "webhook"
   url = "https://hooks.slack.com/..."
   ```

3. Test alert rules:
   ```bash
   # Trigger test alert
   soulbox-cli alerts trigger --name test-alert --value 100
   ```

4. Check alert logs:
   ```bash
   grep -i alert /var/log/soulbox/soulbox.log
   ```

### Authentication Issues

#### Issue: API Key Authentication Failed

**Symptoms:**
```
Error: 401 Unauthorized
```

**Solutions:**
1. Verify API key configuration:
   ```bash
   # Check if API key exists
   soulbox-cli auth list-keys
   
   # Create new API key
   soulbox-cli auth create-key --name client-key
   ```

2. Use correct authentication header:
   ```bash
   curl -H "X-API-Key: your-api-key" http://localhost:8080/api/execute
   ```

3. Check API key permissions:
   ```bash
   soulbox-cli auth inspect-key --key your-api-key
   ```

#### Issue: JWT Token Expired

**Symptoms:**
```
Error: Token expired
```

**Solutions:**
1. Refresh JWT token:
   ```bash
   # Get new token
   curl -X POST http://localhost:8080/auth/refresh \
     -H "Authorization: Bearer expired-token"
   ```

2. Configure token expiration:
   ```toml
   [auth]
   jwt_expiration_hours = 24
   refresh_token_expiration_days = 30
   ```

### Snapshot and Recovery Issues

#### Issue: Snapshot Creation Failed

**Symptoms:**
```
Error: Failed to create snapshot
```

**Solutions:**
1. Check available disk space:
   ```bash
   df -h /var/lib/soulbox/snapshots
   du -sh /var/lib/soulbox/snapshots/*
   ```

2. Verify snapshot permissions:
   ```bash
   sudo chown -R soulbox:soulbox /var/lib/soulbox/snapshots
   sudo chmod -R 755 /var/lib/soulbox/snapshots
   ```

3. Clean up old snapshots:
   ```bash
   soulbox-cli snapshot cleanup --older-than 7d
   soulbox-cli snapshot list --show-size
   ```

4. Check container state:
   ```bash
   soulbox-cli inspect --container-id <id> --show-processes
   ```

#### Issue: Snapshot Restoration Failed

**Symptoms:**
```
Error: Failed to restore from snapshot
```

**Solutions:**
1. Verify snapshot integrity:
   ```bash
   soulbox-cli snapshot verify --snapshot-id <id>
   ```

2. Check restoration logs:
   ```bash
   soulbox-cli logs --operation restore --snapshot-id <id>
   ```

3. Try alternative restoration method:
   ```bash
   # Restore to new container
   soulbox-cli snapshot restore --snapshot-id <id> --target new-container
   ```

## Debugging Tools

### Log Analysis

```bash
# View real-time logs
soulbox-cli logs --follow

# Filter by component
soulbox-cli logs --component container --level error

# Search logs
soulbox-cli logs --grep "timeout" --since 1h

# Export logs
soulbox-cli logs --since 24h --format json > debug.log
```

### System Diagnostics

```bash
# Run system diagnostics
soulbox-cli diagnose

# Check component health
soulbox-cli health --all

# Generate support bundle
soulbox-cli support-bundle --output /tmp/soulbox-debug.tar.gz
```

### Performance Analysis

```bash
# CPU profiling
soulbox-cli profile cpu --duration 60

# Memory profiling
soulbox-cli profile memory --track-allocations

# I/O profiling
soulbox-cli profile io --track-operations

# Generate performance report
soulbox-cli profile report --format html
```

## Getting Help

### Documentation
- [User Guide](user-guide.md)
- [API Reference](api-reference.md)
- [Configuration Guide](configuration.md)

### Community Support
- GitHub Issues: [Report bugs and request features](https://github.com/RcityHunter/soulbox/issues)
- Discussions: [Community discussions](https://github.com/RcityHunter/soulbox/discussions)
- Stack Overflow: Tag questions with `soulbox`

### Professional Support
- Enterprise support available
- Custom integration assistance
- Performance optimization consulting

### Collecting Debug Information

When reporting issues, please include:

1. **System Information:**
   ```bash
   soulbox-cli info --system
   uname -a
   docker version
   docker info
   ```

2. **Configuration:**
   ```bash
   soulbox-cli config --show-sanitized
   ```

3. **Logs:**
   ```bash
   soulbox-cli logs --since 1h --format json > issue-logs.json
   ```

4. **Performance Metrics:**
   ```bash
   curl http://localhost:8080/metrics > metrics.txt
   ```

5. **Support Bundle:**
   ```bash
   soulbox-cli support-bundle --include-logs --include-config --include-metrics
   ```

This comprehensive troubleshooting guide should help resolve most common issues. If you continue to experience problems, please file an issue with the debug information above.