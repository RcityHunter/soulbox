-- SQLite 初始化架构

-- 启用外键约束（运行时需要）
PRAGMA foreign_keys = ON;

-- 租户表
CREATE TABLE IF NOT EXISTS tenants (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    description TEXT,
    settings TEXT, -- JSON
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 用户表
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    username TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'User' CHECK (role IN ('SuperAdmin', 'TenantAdmin', 'Developer', 'User', 'ReadOnly')),
    is_active INTEGER NOT NULL DEFAULT 1,
    tenant_id TEXT REFERENCES tenants(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_login TEXT
);

-- API密钥表
CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    key_hash TEXT NOT NULL UNIQUE,
    prefix TEXT NOT NULL,
    name TEXT NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    permissions TEXT NOT NULL DEFAULT '[]', -- JSON
    expires_at TEXT,
    last_used_at TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 会话表
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    refresh_token_hash TEXT UNIQUE,
    ip_address TEXT,
    user_agent TEXT,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_accessed_at TEXT NOT NULL DEFAULT (datetime('now')),
    is_active INTEGER NOT NULL DEFAULT 1
);

-- 审计日志表
CREATE TABLE IF NOT EXISTS audit_logs (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    event_type TEXT NOT NULL,
    severity TEXT NOT NULL CHECK (severity IN ('Info', 'Warning', 'Error', 'Critical')),
    result TEXT NOT NULL CHECK (result IN ('Success', 'Failure', 'Pending')),
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    
    -- 用户信息
    user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    username TEXT,
    user_role TEXT,
    tenant_id TEXT REFERENCES tenants(id) ON DELETE SET NULL,
    
    -- 请求信息
    request_id TEXT,
    ip_address TEXT,
    user_agent TEXT,
    request_path TEXT,
    http_method TEXT,
    
    -- 资源信息
    resource_type TEXT,
    resource_id TEXT,
    resource_name TEXT,
    
    -- 权限信息
    permission TEXT,
    old_role TEXT,
    new_role TEXT,
    
    -- 事件详情
    message TEXT NOT NULL,
    details TEXT, -- JSON
    error_code TEXT,
    error_message TEXT,
    
    -- 元数据
    session_id TEXT,
    correlation_id TEXT,
    tags TEXT -- JSON
);

-- 模板表
CREATE TABLE IF NOT EXISTS templates (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    description TEXT,
    runtime_type TEXT NOT NULL CHECK (runtime_type IN ('docker', 'firecracker')),
    base_image TEXT NOT NULL,
    default_command TEXT,
    environment_vars TEXT, -- JSON
    resource_limits TEXT, -- JSON
    is_public INTEGER NOT NULL DEFAULT 0,
    owner_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 沙盒表
CREATE TABLE IF NOT EXISTS sandboxes (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    name TEXT NOT NULL,
    runtime_type TEXT NOT NULL CHECK (runtime_type IN ('docker', 'firecracker')),
    template TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'created' CHECK (status IN ('created', 'starting', 'running', 'stopping', 'stopped', 'error')),
    owner_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tenant_id TEXT REFERENCES tenants(id) ON DELETE SET NULL,
    
    -- 资源限制
    cpu_limit INTEGER,
    memory_limit INTEGER,
    disk_limit INTEGER,
    
    -- 运行时信息
    container_id TEXT,
    vm_id TEXT,
    ip_address TEXT,
    port_mappings TEXT, -- JSON
    
    -- 时间戳
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    started_at TEXT,
    stopped_at TEXT,
    expires_at TEXT
);

-- 创建索引
CREATE INDEX idx_users_tenant_id ON users(tenant_id);
CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_email ON users(email);

CREATE INDEX idx_api_keys_user_id ON api_keys(user_id);
CREATE INDEX idx_api_keys_prefix ON api_keys(prefix);

CREATE INDEX idx_sessions_user_id ON sessions(user_id);
CREATE INDEX idx_sessions_expires_at ON sessions(expires_at);

CREATE INDEX idx_audit_logs_timestamp ON audit_logs(timestamp);
CREATE INDEX idx_audit_logs_user_id ON audit_logs(user_id);
CREATE INDEX idx_audit_logs_tenant_id ON audit_logs(tenant_id);
CREATE INDEX idx_audit_logs_event_type ON audit_logs(event_type);
CREATE INDEX idx_audit_logs_severity ON audit_logs(severity);
CREATE INDEX idx_audit_logs_result ON audit_logs(result);

CREATE INDEX idx_sandboxes_owner_id ON sandboxes(owner_id);
CREATE INDEX idx_sandboxes_tenant_id ON sandboxes(tenant_id);
CREATE INDEX idx_sandboxes_status ON sandboxes(status);
CREATE INDEX idx_sandboxes_runtime_type ON sandboxes(runtime_type);

CREATE INDEX idx_templates_slug ON templates(slug);
CREATE INDEX idx_templates_owner_id ON templates(owner_id);

-- SQLite 触发器：自动更新 updated_at
CREATE TRIGGER update_users_updated_at 
AFTER UPDATE ON users
BEGIN
    UPDATE users SET updated_at = datetime('now') WHERE id = NEW.id;
END;

CREATE TRIGGER update_tenants_updated_at 
AFTER UPDATE ON tenants
BEGIN
    UPDATE tenants SET updated_at = datetime('now') WHERE id = NEW.id;
END;

CREATE TRIGGER update_api_keys_updated_at 
AFTER UPDATE ON api_keys
BEGIN
    UPDATE api_keys SET updated_at = datetime('now') WHERE id = NEW.id;
END;

CREATE TRIGGER update_templates_updated_at 
AFTER UPDATE ON templates
BEGIN
    UPDATE templates SET updated_at = datetime('now') WHERE id = NEW.id;
END;

CREATE TRIGGER update_sandboxes_updated_at 
AFTER UPDATE ON sandboxes
BEGIN
    UPDATE sandboxes SET updated_at = datetime('now') WHERE id = NEW.id;
END;