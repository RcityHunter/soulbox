use std::path::PathBuf;
use tokio::sync::mpsc;
use notify::{Watcher, RecursiveMode, Event};

use crate::error::{Result, SoulBoxError};

#[derive(Debug)]
pub struct FileWatcher {
    receiver: mpsc::UnboundedReceiver<FileEvent>,
    _watcher: notify::RecommendedWatcher, // Keep watcher alive
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEvent {
    pub event_type: FileEventType,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileEventType {
    Created,
    Modified,
    Deleted,
    Moved,
    Other,
}

impl FileWatcher {
    pub async fn new(path: PathBuf, recursive: bool) -> Result<Self> {
        let (sender, receiver) = mpsc::unbounded_channel();
        
        let mut watcher = notify::recommended_watcher(move |event_result: notify::Result<Event>| {
            if let Ok(event) = event_result {
                for path in event.paths {
                    let file_event = FileEvent {
                        event_type: match event.kind {
                            notify::EventKind::Create(_) => FileEventType::Created,
                            notify::EventKind::Modify(_) => FileEventType::Modified,
                            notify::EventKind::Remove(_) => FileEventType::Deleted,
                            notify::EventKind::Any => FileEventType::Other,
                            _ => FileEventType::Other,
                        },
                        path: path.to_string_lossy().to_string(),
                    };
                    
                    let _ = sender.send(file_event);
                }
            }
        }).map_err(|e| SoulBoxError::filesystem(format!("Failed to create file watcher: {}", e)))?;

        let mode = if recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };

        watcher.watch(&path, mode)
            .map_err(|e| SoulBoxError::filesystem(format!("Failed to watch path: {}", e)))?;

        Ok(Self {
            receiver,
            _watcher: watcher,
        })
    }

    pub async fn next_event(&mut self) -> Result<FileEvent> {
        self.receiver.recv().await
            .ok_or_else(|| SoulBoxError::filesystem("File watcher closed".to_string()))
    }
}