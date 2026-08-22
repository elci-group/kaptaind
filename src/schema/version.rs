//! Schema version type for the `.kaptaind` semantic-state format.
//!
//! Schema versions are independent of the kaptaind software version and the
//! project version: three distinct version domains.

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A `major.minor` schema version (e.g. `2.1`). Patch position is
/// intentionally excluded: format revisions always bump at least `minor`.
/// Serializes as the plain string form (`"2.1"`) so documents stay readable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchemaVersion {
    pub major: u32,
    pub minor: u32,
}

impl Serialize for SchemaVersion {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for SchemaVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

impl SchemaVersion {
    pub const fn new(major: u32, minor: u32) -> Self {
        Self { major, minor }
    }
}

impl fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

impl PartialOrd for SchemaVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SchemaVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor).cmp(&(other.major, other.minor))
    }
}

impl FromStr for SchemaVersion {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (major, minor) = s
            .split_once('.')
            .ok_or_else(|| format!("expected `<major>.<minor>`, got `{s}`"))?;
        let major = major
            .parse()
            .map_err(|e| format!("invalid major version in `{s}`: {e}"))?;
        let minor = minor
            .parse()
            .map_err(|e| format!("invalid minor version in `{s}`: {e}"))?;
        Ok(Self { major, minor })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_orders() {
        let v20 = SchemaVersion::from_str("2.0").unwrap();
        let v21 = SchemaVersion::from_str("2.1").unwrap();
        let v10 = SchemaVersion::from_str("1.0").unwrap();
        assert!(v20 < v21);
        assert!(v10 < v20);
        assert_eq!(v21.to_string(), "2.1");
    }

    #[test]
    fn rejects_malformed_versions() {
        assert!(SchemaVersion::from_str("2").is_err());
        assert!(SchemaVersion::from_str("two.one").is_err());
        assert!(SchemaVersion::from_str("2.x").is_err());
    }
}
