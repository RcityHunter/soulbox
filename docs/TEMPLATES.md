# SoulBox Template System

## Overview

The SoulBox template system provides a powerful and user-friendly way to create, manage, and deploy containerized development environments. Templates define the complete environment configuration including runtime, dependencies, and initial files.

## Quick Start

### Using Preset Templates

SoulBox comes with several pre-configured templates for common use cases:

```rust
use soulbox::template::TemplatePresets;

// Create a Python FastAPI template
let fastapi = TemplatePresets::python_fastapi()
    .with_author("your-email@example.com")
    .build()
    .unwrap();

// Create a Node.js Express template
let express = TemplatePresets::nodejs_express()
    .build()
    .unwrap();

// Create a Rust Actix-web template
let actix = TemplatePresets::rust_actix()
    .build()
    .unwrap();
```

### Available Presets

| Preset | Runtime | Description | Default Port |
|--------|---------|-------------|--------------|
| `python_fastapi()` | Python 3.11 | FastAPI async web framework | 8000 |
| `nodejs_express()` | Node.js 20 | Express.js web application | 3000 |
| `rust_actix()` | Rust 1.75 | Actix-web HTTP server | 8080 |
| `python_data_science()` | Python 3.11 | Jupyter with ML libraries | 8888 |
| `go_gin()` | Go 1.21 | Gin web framework | 8080 |

## Creating Custom Templates

### Basic Template

```rust
use soulbox::template::TemplateBuilder;
use soulbox::runtime::RuntimeType;

let template = TemplateBuilder::new("My App", RuntimeType::Python)
    .with_description("My custom Python application")
    .with_author("developer@example.com")
    .add_tag("custom")
    .add_env_var("APP_ENV", "production")
    .expose_port(8000)
    .build()
    .unwrap();
```

### Advanced Template with Files

```rust
let template = TemplateBuilder::new("ML Pipeline", RuntimeType::Python)
    .with_description("Machine learning training pipeline")
    .with_base_image("tensorflow/tensorflow:latest")
    .add_file("requirements.txt", "numpy\npandas\nscikit-learn")
    .add_file("train.py", r#"
import numpy as np
import pandas as pd
from sklearn.model_selection import train_test_split

# Your ML code here
print("Training started...")
"#)
    .add_setup_command("pip install -r requirements.txt")
    .add_env_var("PYTHONUNBUFFERED", "1")
    .expose_port(6006)  // TensorBoard
    .add_volume("/data")
    .build()
    .unwrap();
```

## Template Builder API

### Core Methods

| Method | Description | Example |
|--------|-------------|---------|
| `new(name, runtime)` | Create new template builder | `TemplateBuilder::new("App", RuntimeType::Python)` |
| `with_description(desc)` | Set template description | `.with_description("Web API")` |
| `with_base_image(image)` | Set Docker base image | `.with_base_image("node:20-alpine")` |
| `with_author(author)` | Set template author | `.with_author("team@example.com")` |

### File Management

| Method | Description | Example |
|--------|-------------|---------|
| `add_file(path, content)` | Add a file to template | `.add_file("app.py", "print('Hello')")` |
| `add_executable(path, content)` | Add executable file | `.add_executable("start.sh", "#!/bin/bash...")` |

### Configuration

| Method | Description | Example |
|--------|-------------|---------|
| `add_env_var(key, value)` | Add environment variable | `.add_env_var("DEBUG", "false")` |
| `expose_port(port)` | Expose a port | `.expose_port(8080)` |
| `add_volume(path)` | Add volume mount | `.add_volume("/data")` |
| `add_setup_command(cmd)` | Add setup command | `.add_setup_command("npm install")` |

### Tags and Metadata

| Method | Description | Example |
|--------|-------------|---------|
| `add_tag(tag)` | Add a single tag | `.add_tag("api")` |
| `with_tags(tags)` | Set all tags | `.with_tags(vec!["web", "api"])` |

## Runtime Support

SoulBox supports the following runtime environments:

- **Python** (3.8, 3.9, 3.10, 3.11)
- **Node.js** (16, 18, 20)
- **Rust** (stable, beta, nightly)
- **Go** (1.19, 1.20, 1.21)
- **Java** (11, 17, 21)
- **Shell** (Bash, Sh)

## Dockerfile Generation

Templates automatically generate optimized Dockerfiles based on the runtime:

### Python Example
```dockerfile
FROM python:3.11-slim
WORKDIR /workspace
RUN pip install --upgrade pip
COPY requirements.txt .
RUN pip install -r requirements.txt
COPY . .
ENV PYTHONUNBUFFERED=1
EXPOSE 8000
```

### Node.js Example
```dockerfile
FROM node:20-alpine
WORKDIR /workspace
COPY package*.json ./
RUN npm ci --only=production
COPY . .
EXPOSE 3000
```

## Best Practices

### 1. Use Specific Base Images
```rust
.with_base_image("python:3.11-slim")  // Good: specific and slim
.with_base_image("python:latest")      // Avoid: unpredictable
```

### 2. Minimize Layers
```rust
// Good: combine related commands
.add_setup_command("apt-get update && apt-get install -y curl git && rm -rf /var/lib/apt/lists/*")

// Avoid: multiple separate commands
.add_setup_command("apt-get update")
.add_setup_command("apt-get install -y curl")
.add_setup_command("apt-get install -y git")
```

### 3. Use .dockerignore
```rust
.add_file(".dockerignore", "
*.pyc
__pycache__
.git
.env
node_modules
*.log
")
```

### 4. Set Resource Limits
```rust
// When creating sandbox from template
let sandbox = manager.create_sandbox_from_template(
    template,
    ResourceLimits {
        cpu_cores: 2.0,
        memory_mb: 1024,
        disk_mb: 5120,
        ..Default::default()
    }
).await?;
```

## Template Validation

Templates are automatically validated for:

- Valid runtime configuration
- Dockerfile syntax
- Port conflicts
- Security best practices
- Resource requirements

## Examples

### Web API Template
```rust
let api = TemplateBuilder::new("REST API", RuntimeType::Python)
    .with_description("RESTful API with FastAPI")
    .add_file("requirements.txt", "fastapi\nuvicorn\nsqlalchemy")
    .add_file("main.py", "from fastapi import FastAPI...")
    .add_env_var("DATABASE_URL", "postgresql://...")
    .expose_port(8000)
    .build()?;
```

### Data Processing Template
```rust
let processor = TemplateBuilder::new("Data Processor", RuntimeType::Python)
    .with_description("Batch data processing pipeline")
    .add_file("requirements.txt", "pandas\nnumpy\napache-beam")
    .add_file("process.py", "import pandas as pd...")
    .add_volume("/input")
    .add_volume("/output")
    .build()?;
```

### Microservice Template
```rust
let service = TemplateBuilder::new("Microservice", RuntimeType::Go)
    .with_description("Go microservice with gRPC")
    .add_file("go.mod", "module service...")
    .add_file("main.go", "package main...")
    .expose_port(50051)  // gRPC
    .expose_port(8080)   // HTTP
    .build()?;
```

## CLI Usage

```bash
# List available templates
soulbox template list

# Create sandbox from template
soulbox template run python-fastapi

# Create custom template
soulbox template create --name "My App" --runtime python

# Export template
soulbox template export my-app > template.json

# Import template
soulbox template import < template.json
```

## API Integration

### REST API
```bash
# Get all templates
GET /api/v1/templates

# Get specific template
GET /api/v1/templates/{id}

# Create new template
POST /api/v1/templates

# Create sandbox from template
POST /api/v1/sandboxes/from-template/{template_id}
```

### gRPC API
```protobuf
service TemplateService {
    rpc ListTemplates(ListTemplatesRequest) returns (ListTemplatesResponse);
    rpc GetTemplate(GetTemplateRequest) returns (Template);
    rpc CreateTemplate(CreateTemplateRequest) returns (Template);
    rpc CreateSandboxFromTemplate(CreateFromTemplateRequest) returns (Sandbox);
}
```

## Troubleshooting

### Common Issues

1. **Template build fails**
   - Check runtime is supported
   - Validate Dockerfile syntax
   - Ensure base image exists

2. **Files not found in container**
   - Verify file paths are relative
   - Check .dockerignore settings
   - Ensure COPY commands are correct

3. **Ports not accessible**
   - Confirm ports are exposed
   - Check for port conflicts
   - Verify network configuration

## Advanced Features

### Template Versioning
```rust
let v2 = template.create_version("2.0.0")
    .with_changelog("Added new features")
    .build()?;
```

### Template Inheritance
```rust
let base = TemplatePresets::python_fastapi();
let extended = base.extend()
    .add_file("custom.py", "...")
    .add_env_var("CUSTOM", "true")
    .build()?;
```

### Template Composition
```rust
let frontend = TemplatePresets::nodejs_express();
let backend = TemplatePresets::python_fastapi();
let fullstack = Template::compose(vec![frontend, backend])?;
```

## Contributing

To contribute a new template preset:

1. Add template to `src/template/builder.rs`
2. Add tests to verify template
3. Update documentation
4. Submit pull request

Example:
```rust
impl TemplatePresets {
    pub fn my_preset() -> TemplateBuilder {
        TemplateBuilder::new("My Preset", RuntimeType::Python)
            // ... configuration
    }
}
```