//! Example of using the improved template system

use soulbox::template::{TemplateBuilder, TemplatePresets};
use soulbox::runtime::RuntimeType;
use std::collections::HashMap;

fn main() {
    println!("=== SoulBox Template System Examples ===\n");

    // Example 1: Using preset templates
    println!("1. Creating a FastAPI template from preset:");
    let fastapi_template = TemplatePresets::python_fastapi()
        .with_author("developer@example.com")
        .add_tag("production")
        .build()
        .unwrap();
    
    println!("   - Name: {}", fastapi_template.name);
    println!("   - Runtime: {:?}", fastapi_template.runtime_type);
    println!("   - Tags: {:?}", fastapi_template.tags.as_ref().unwrap_or(&vec![]));
    println!("   - Base Image: {}", fastapi_template.base_image);
    println!();

    // Example 2: Creating custom template with builder
    println!("2. Creating custom Python ML template:");
    let ml_template = TemplateBuilder::new("Custom ML Pipeline", RuntimeType::Python)
        .with_description("Custom machine learning pipeline with TensorFlow")
        .with_base_image("tensorflow/tensorflow:2.14.0")
        .add_tag("machine-learning")
        .add_tag("tensorflow")
        .add_tag("gpu")
        .add_file("requirements.txt", "numpy==1.24.3\npandas==2.0.3\nscikit-learn==1.3.0")
        .add_file("train.py", r#"
import tensorflow as tf
import numpy as np

def train_model():
    # Your ML training code here
    print("Training model...")
    return tf.keras.Sequential([
        tf.keras.layers.Dense(128, activation='relu'),
        tf.keras.layers.Dense(10, activation='softmax')
    ])

if __name__ == "__main__":
    model = train_model()
    print("Model trained successfully!")
"#)
        .add_setup_command("pip install --no-cache-dir -r requirements.txt")
        .add_env_var("TF_CPP_MIN_LOG_LEVEL", "2")
        .add_env_var("PYTHONUNBUFFERED", "1")
        .expose_port(6006)  // TensorBoard
        .add_volume("/data")
        .build()
        .unwrap();

    println!("   - Name: {}", ml_template.name);
    println!("   - Base Image: {}", ml_template.base_image);
    println!("   - Environment: {:?}", ml_template.environment_vars.as_ref().unwrap_or(&HashMap::new()));
    println!("   - Files: {} files", ml_template.files.as_ref().map(|f| f.len()).unwrap_or(0));
    println!();

    // Example 3: Creating a microservice template
    println!("3. Creating Node.js microservice template:");
    let microservice = TemplateBuilder::new("Microservice API", RuntimeType::NodeJS)
        .with_description("RESTful microservice with Express and MongoDB")
        .add_tag("microservice")
        .add_tag("rest-api")
        .add_tag("mongodb")
        .add_file("package.json", r#"{
  "name": "microservice-api",
  "version": "1.0.0",
  "scripts": {
    "start": "node src/server.js",
    "dev": "nodemon src/server.js"
  },
  "dependencies": {
    "express": "^4.18.2",
    "mongoose": "^7.5.0",
    "dotenv": "^16.3.1",
    "cors": "^2.8.5",
    "helmet": "^7.0.0"
  },
  "devDependencies": {
    "nodemon": "^3.0.1"
  }
}"#)
        .add_file("src/server.js", r#"
const express = require('express');
const mongoose = require('mongoose');
const cors = require('cors');
const helmet = require('helmet');
require('dotenv').config();

const app = express();
const PORT = process.env.PORT || 3000;

// Middleware
app.use(helmet());
app.use(cors());
app.use(express.json());

// Routes
app.get('/health', (req, res) => {
    res.json({ status: 'healthy', timestamp: new Date() });
});

app.get('/api/v1/status', (req, res) => {
    res.json({ 
        service: 'microservice-api',
        version: '1.0.0',
        uptime: process.uptime()
    });
});

// Start server
app.listen(PORT, () => {
    console.log(`Microservice running on port ${PORT}`);
});
"#)
        .add_env_var("NODE_ENV", "production")
        .add_env_var("MONGODB_URI", "mongodb://localhost:27017/microservice")
        .expose_port(3000)
        .build()
        .unwrap();

    println!("   - Name: {}", microservice.name);
    println!("   - Runtime: {:?}", microservice.runtime_type);
    println!("   - Files: {} files", microservice.files.as_ref().map(|f| f.len()).unwrap_or(0));
    println!();

    // Example 4: List all available presets
    println!("4. Available Template Presets:");
    println!("   - Python FastAPI: Web API with async support");
    println!("   - Node.js Express: Express.js web application");
    println!("   - Rust Actix-web: High-performance HTTP server");
    println!("   - Python Data Science: Jupyter with ML libraries");
    println!("   - Go Gin: Gin web framework application");
    println!();

    // Example 5: Show template details
    println!("5. Template Details for FastAPI:");
    println!("   - Slug: {}", fastapi_template.slug);
    println!("   - Description: {:?}", fastapi_template.description);
    println!("   - Setup Commands: {} commands", fastapi_template.setup_commands.as_ref().map(|c| c.len()).unwrap_or(0));
    println!();

    println!("=== Template System Examples Complete ===");
}