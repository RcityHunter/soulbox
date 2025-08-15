// SoulBox Simple Implementation - Linus Style
// "Good taste" principle: Simple data structures, no special cases

pub mod core;
pub mod api;
pub mod execution;

pub use core::{SandboxManager, Sandbox, SandboxStatus};
pub use api::{SimpleAPI, CreateRequest, ExecuteRequest};
pub use execution::ExecutionResult;
pub use execution::{CodeExecution, LanguageConfig};