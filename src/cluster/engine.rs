use crate::watcher::FsEvent;
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Cluster {
    pub id: Uuid,
    pub events: Vec<FsEvent>,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
}

impl Cluster {
    pub fn new(event: FsEvent) -> Self {
        let ts = event.timestamp;
        Self {
            id: Uuid::new_v4(),
            events: vec![event],
            started_at: ts,
            ended_at: ts,
        }
    }

    pub fn add_event(&mut self, mut event: FsEvent) {
        self.ended_at = event.timestamp;
        self.events.push(event);

        if self.events.len() >= 1000 {
            self.compact();
        }
    }

    fn compact(&mut self) {
        let mut grouped: std::collections::HashMap<crate::watcher::FsEventKind, Vec<std::path::PathBuf>> = std::collections::HashMap::new();
        let mut latest_ts = self.started_at;
        
        for event in self.events.drain(..) {
            if event.timestamp > latest_ts {
                latest_ts = event.timestamp;
            }
            let entry = grouped.entry(event.kind).or_default();
            for path in event.paths {
                entry.push(path);
            }
        }
        
        for (kind, mut paths) in grouped {
            paths.sort_unstable();
            paths.dedup();
            self.events.push(FsEvent {
                paths,
                kind,
                timestamp: latest_ts,
            });
        }
    }
}

pub struct ClusterEngine {
    current: Option<Cluster>,
    last_event: Option<DateTime<Utc>>,
    window: chrono::Duration,
}

impl ClusterEngine {
    pub fn new(window: std::time::Duration) -> Self {
        let secs = i64::try_from(window.as_secs()).unwrap_or(i64::MAX);
        Self {
            current: None,
            last_event: None,
            window: chrono::Duration::seconds(secs),
        }
    }

    pub fn ingest(&mut self, event: FsEvent) -> Option<Cluster> {
        let now = event.timestamp;

        match &mut self.current {
            Some(cluster) => {
                let should_merge = self
                    .last_event
                    .map(|last| now - last < self.window)
                    .unwrap_or(false);

                if should_merge {
                    cluster.add_event(event);
                    self.last_event = Some(now);
                    None
                } else {
                    let finished = self.current.take();
                    self.current = Some(Cluster::new(event));
                    self.last_event = Some(now);
                    finished
                }
            }
            None => {
                self.current = Some(Cluster::new(event));
                self.last_event = Some(now);
                None
            }
        }
    }

    pub fn flush(&mut self) -> Option<Cluster> {
        self.last_event = None;
        self.current.take()
    }
}

#[cfg(test)]
mod tests {
    use super::ClusterEngine;
    use crate::watcher::{FsEvent, FsEventKind};
    use chrono::{Duration as ChronoDuration, Utc};
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn clusters_events_within_window() {
        let mut engine = ClusterEngine::new(Duration::from_secs(2));
        let first = event("src/main.rs", Utc::now());
        let second = event("src/lib.rs", first.timestamp + ChronoDuration::milliseconds(500));

        assert!(engine.ingest(first).is_none());
        assert!(engine.ingest(second).is_none());

        let flushed = engine.flush().expect("cluster should exist");
        assert_eq!(flushed.events.len(), 2);
    }

    #[test]
    fn emits_cluster_when_window_expires() {
        let mut engine = ClusterEngine::new(Duration::from_secs(1));
        let first = event("src/main.rs", Utc::now());
        let second = event("src/lib.rs", first.timestamp + ChronoDuration::seconds(2));

        assert!(engine.ingest(first).is_none());
        let emitted = engine.ingest(second).expect("expected previous cluster");
        assert_eq!(emitted.events.len(), 1);
        assert!(engine.flush().is_some());
    }

    fn event(path: &str, timestamp: chrono::DateTime<Utc>) -> FsEvent {
        FsEvent {
            paths: vec![PathBuf::from(path)],
            kind: FsEventKind::Modify,
            timestamp,
        }
    }
}
