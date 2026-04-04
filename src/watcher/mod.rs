pub mod fs;

use chrono::{DateTime, Utc};
use notify::{event::EventKind, Event};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsEvent {
    pub paths: Vec<PathBuf>,
    pub kind: FsEventKind,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum FsEventKind {
    Create,
    Modify,
    Remove,
    Other,
}

impl From<Event> for FsEvent {
    fn from(event: Event) -> Self {
        let kind = match event.kind {
            EventKind::Create(_) => FsEventKind::Create,
            EventKind::Modify(_) => FsEventKind::Modify,
            EventKind::Remove(_) => FsEventKind::Remove,
            _ => FsEventKind::Other,
        };

        Self {
            paths: event.paths,
            kind,
            timestamp: Utc::now(),
        }
    }
}
