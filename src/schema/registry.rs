//! Registry of known `.kaptaind` schema versions.
//!
//! The format is a maintained artifact of kaptaind development: every
//! released schema version is described here, and the compatibility surface
//! (reads / writes / migrates-to) is explicit.

use std::sync::LazyLock;

use super::version::SchemaVersion;

/// A known schema version and its description.
#[derive(Debug, Clone)]
pub struct SchemaInfo {
    pub version: SchemaVersion,
    pub family: &'static str,
    pub description: &'static str,
}

/// Every schema version this kaptaind knows about, in ascending order.
pub fn schemas() -> &'static [SchemaInfo] {
    &SCHEMAS
}

static SCHEMAS: LazyLock<Vec<SchemaInfo>> = LazyLock::new(|| {
    vec![
        SchemaInfo {
            version: SchemaVersion::new(1, 0),
            family: "legacy",
            description: "implicit pre-semantic state: no .kaptaind/state.toml document",
        },
        SchemaInfo {
            version: SchemaVersion::new(2, 0),
            family: "semantic-state",
            description: "initial semantic-state document: surfaces, invariants, exceptions, versioning policy, baseline, memory budget",
        },
        SchemaInfo {
            version: SchemaVersion::new(2, 1),
            family: "semantic-state",
            description: "adds surface audience/stability/compatibility (unknown is a valid state)",
        },
        SchemaInfo {
            version: SchemaVersion::new(2, 2),
            family: "semantic-state",
            description: "declares branch lifecycle topology and stable/bleeding consumer channels",
        },
    ]
});

/// Latest schema version this kaptaind can read, write, and migrate to.
pub fn latest_version() -> SchemaVersion {
    schemas()
        .iter()
        .map(|s| s.version)
        .max()
        .unwrap_or_else(|| {
            tracing::error!(
                registry = "schemas",
                "schema registry is unexpectedly empty"
            );
            SchemaVersion::new(0, 0)
        })
}

/// Look up a registered schema by version.
pub fn find(version: SchemaVersion) -> Option<&'static SchemaInfo> {
    schemas().iter().find(|s| s.version == version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_ordered_and_complete() {
        let all = schemas();
        assert!(all.windows(2).all(|w| w[0].version < w[1].version));
        assert_eq!(latest_version(), SchemaVersion::new(2, 2));
        assert!(find(SchemaVersion::new(2, 0)).is_some());
        assert!(find(SchemaVersion::new(9, 9)).is_none());
    }
}
