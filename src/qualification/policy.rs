use serde::{Deserialize, Serialize};
use std::time::Duration;

/// `[qualification]` block in `kaptaind.toml`.
///
/// When `enabled = false` (the default) the entire qualification/release
/// pipeline is skipped, preserving the existing behaviour exactly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualificationConfig {
    /// Master switch. Defaults to false (backwards-compatible).
    #[serde(default)]
    pub enabled: bool,

    /// Minimum stability score required before a release is allowed.
    #[serde(default = "default_stability_threshold")]
    pub stability_threshold: f64,

    /// Number of consecutive passing commits required.
    #[serde(default = "default_min_pass_streak")]
    pub min_pass_streak: usize,

    /// Diff score above this value blocks a release (spike guard).
    #[serde(default = "default_max_allowed_diff")]
    pub max_allowed_diff: f64,

    /// Minimum wall-clock time between successive releases.
    #[serde(default = "default_cooldown", with = "duration_string")]
    pub cooldown: Duration,
}

fn default_stability_threshold() -> f64 {
    0.85
}

fn default_min_pass_streak() -> usize {
    3
}

fn default_max_allowed_diff() -> f64 {
    0.7
}

fn default_cooldown() -> Duration {
    Duration::from_secs(300) // 5 minutes
}

impl Default for QualificationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            stability_threshold: default_stability_threshold(),
            min_pass_streak: default_min_pass_streak(),
            max_allowed_diff: default_max_allowed_diff(),
            cooldown: default_cooldown(),
        }
    }
}

pub(crate) mod duration_string {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(d: &Duration, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let secs = d.as_secs();
        if secs % 3600 == 0 {
            s.serialize_str(&format!("{}h", secs / 3600))
        } else if secs % 60 == 0 {
            s.serialize_str(&format!("{}m", secs / 60))
        } else {
            s.serialize_str(&format!("{}s", secs))
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse_duration(&s).ok_or_else(|| {
            serde::de::Error::custom(format!(
                "invalid duration '{}'; expected e.g. '5m', '1h', '30s'",
                s
            ))
        })
    }

    pub fn parse_duration(s: &str) -> Option<Duration> {
        if let Some(v) = s.strip_suffix('h') {
            return v.trim().parse::<u64>().ok().map(|n| Duration::from_secs(n * 3600));
        }
        if let Some(v) = s.strip_suffix('m') {
            return v.trim().parse::<u64>().ok().map(|n| Duration::from_secs(n * 60));
        }
        if let Some(v) = s.strip_suffix('s') {
            return v.trim().parse::<u64>().ok().map(|n| Duration::from_secs(n));
        }
        // Plain number → seconds
        s.trim().parse::<u64>().ok().map(Duration::from_secs)
    }
}
