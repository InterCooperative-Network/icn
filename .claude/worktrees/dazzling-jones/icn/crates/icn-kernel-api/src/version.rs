//! Semantic versioning for protocol versioning
//!
//! This is a kernel-level type used by upgrade coordination and version
//! tracking. It lives here so that `icn-core` can use it without importing
//! `icn-governance`.

use serde::{Deserialize, Serialize};

/// Semantic version for protocol versioning
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    /// Create a new version
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Parse version from string (e.g., "1.2.3")
    pub fn parse(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(format!("Invalid version format: {s}"));
        }

        let major = parts[0]
            .parse::<u32>()
            .map_err(|_| format!("Invalid major version: {}", parts[0]))?;
        let minor = parts[1]
            .parse::<u32>()
            .map_err(|_| format!("Invalid minor version: {}", parts[1]))?;
        let patch = parts[2]
            .parse::<u32>()
            .map_err(|_| format!("Invalid patch version: {}", parts[2]))?;

        Ok(Self::new(major, minor, patch))
    }

    /// Check if this version is compatible with another
    ///
    /// Compatible means:
    /// - Same major version (no breaking changes)
    /// - Minor/patch can differ
    pub fn is_compatible_with(&self, other: &Version) -> bool {
        self.major == other.major
    }

    /// Check if this version has breaking changes vs another
    pub fn has_breaking_changes_vs(&self, other: &Version) -> bool {
        self.major != other.major
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_new() {
        let v = Version::new(1, 2, 3);
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test]
    fn test_version_parse() {
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(v, Version::new(1, 2, 3));
    }

    #[test]
    fn test_version_parse_invalid() {
        assert!(Version::parse("1.2").is_err());
        assert!(Version::parse("abc").is_err());
        assert!(Version::parse("1.2.abc").is_err());
    }

    #[test]
    fn test_version_display() {
        assert_eq!(Version::new(1, 2, 3).to_string(), "1.2.3");
    }

    #[test]
    fn test_version_compatibility() {
        let v1 = Version::new(1, 0, 0);
        let v2 = Version::new(1, 5, 0);
        let v3 = Version::new(2, 0, 0);

        assert!(v1.is_compatible_with(&v2));
        assert!(!v1.is_compatible_with(&v3));
    }

    #[test]
    fn test_version_breaking_changes() {
        let v1 = Version::new(1, 0, 0);
        let v2 = Version::new(1, 5, 0);
        let v3 = Version::new(2, 0, 0);

        assert!(!v1.has_breaking_changes_vs(&v2));
        assert!(v1.has_breaking_changes_vs(&v3));
    }

    #[test]
    fn test_version_ordering() {
        assert!(Version::new(1, 0, 0) < Version::new(2, 0, 0));
        assert!(Version::new(1, 0, 0) < Version::new(1, 1, 0));
        assert!(Version::new(1, 0, 0) < Version::new(1, 0, 1));
        assert!(Version::new(1, 0, 0) == Version::new(1, 0, 0));
    }
}
