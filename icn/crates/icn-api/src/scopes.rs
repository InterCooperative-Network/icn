//! API permission scopes
//!
//! This module defines the permission scopes used across all API layers
//! (RPC, Gateway, etc.). Scopes follow the pattern: `domain:action`.

/// Compute permission scopes
pub mod compute {
    /// Submit compute tasks
    pub const SUBMIT: &str = "compute:submit";
    /// Query task status
    pub const STATUS: &str = "compute:status";
    /// Cancel tasks
    pub const CANCEL: &str = "compute:cancel";
    /// List tasks
    pub const LIST: &str = "compute:list";
    /// Wildcard for all compute operations
    pub const WILDCARD: &str = "compute:*";
}

/// Ledger permission scopes
pub mod ledger {
    /// Read balance information
    pub const READ: &str = "ledger:read";
    /// Create payment transactions
    pub const WRITE: &str = "ledger:write";
    /// Query transaction history
    pub const HISTORY: &str = "ledger:history";
    /// Manage ledger entries (admin)
    pub const ADMIN: &str = "ledger:admin";
    /// Wildcard for all ledger operations
    pub const WILDCARD: &str = "ledger:*";
}

/// Governance permission scopes
pub mod governance {
    /// Create governance domains
    pub const CREATE_DOMAIN: &str = "governance:create_domain";
    /// Create proposals
    pub const CREATE_PROPOSAL: &str = "governance:create_proposal";
    /// Cast votes
    pub const VOTE: &str = "governance:vote";
    /// Query proposals and votes
    pub const READ: &str = "governance:read";
    /// Wildcard for all governance operations
    pub const WILDCARD: &str = "governance:*";
}

/// Trust permission scopes
pub mod trust {
    /// Read trust graph
    pub const READ: &str = "trust:read";
    /// Modify trust edges
    pub const WRITE: &str = "trust:write";
    /// Query trust scores
    pub const QUERY: &str = "trust:query";
    /// Wildcard for all trust operations
    pub const WILDCARD: &str = "trust:*";
}

/// Identity permission scopes
pub mod identity {
    /// Read identity information
    pub const READ: &str = "identity:read";
    /// Modify identity
    pub const WRITE: &str = "identity:write";
    /// Rotate keys
    pub const ROTATE: &str = "identity:rotate";
    /// Wildcard for all identity operations
    pub const WILDCARD: &str = "identity:*";
}

/// Federation permission scopes
pub mod federation {
    /// Register with federation
    pub const REGISTER: &str = "federation:register";
    /// Query federation state
    pub const READ: &str = "federation:read";
    /// Modify federation membership
    pub const WRITE: &str = "federation:write";
    /// Wildcard for all federation operations
    pub const WILDCARD: &str = "federation:*";
}

/// Cooperative permission scopes
pub mod coop {
    /// Create cooperative
    pub const CREATE: &str = "coop:create";
    /// Read cooperative information
    pub const READ: &str = "coop:read";
    /// Modify cooperative
    pub const WRITE: &str = "coop:write";
    /// Manage members
    pub const MANAGE_MEMBERS: &str = "coop:manage_members";
    /// Wildcard for all coop operations
    pub const WILDCARD: &str = "coop:*";
}

/// Network permission scopes
pub mod network {
    /// Query network peers
    pub const READ: &str = "network:read";
    /// Manage connections
    pub const MANAGE: &str = "network:manage";
    /// Wildcard for all network operations
    pub const WILDCARD: &str = "network:*";
}

/// Admin permission scopes
pub mod admin {
    /// Full system access
    pub const WILDCARD: &str = "*";
}

/// Check if a granted scope matches a required scope
///
/// Supports wildcard matching:
/// - `compute:*` matches `compute:submit`, `compute:status`, etc.
/// - `*` matches everything
pub fn matches(granted: &str, required: &str) -> bool {
    if granted == admin::WILDCARD {
        return true;
    }

    if granted == required {
        return true;
    }

    // Check wildcard match (e.g., "compute:*" matches "compute:submit")
    if let Some(prefix) = granted.strip_suffix(":*") {
        if let Some(req_prefix) = required.split(':').next() {
            return prefix == req_prefix;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        assert!(matches(compute::SUBMIT, compute::SUBMIT));
        assert!(matches(ledger::READ, ledger::READ));
        assert!(!matches(compute::SUBMIT, compute::CANCEL));
    }

    #[test]
    fn test_wildcard_match() {
        assert!(matches(compute::WILDCARD, compute::SUBMIT));
        assert!(matches(compute::WILDCARD, compute::CANCEL));
        assert!(matches(ledger::WILDCARD, ledger::READ));
        assert!(!matches(compute::WILDCARD, ledger::READ));
    }

    #[test]
    fn test_admin_wildcard() {
        assert!(matches(admin::WILDCARD, compute::SUBMIT));
        assert!(matches(admin::WILDCARD, ledger::READ));
        assert!(matches(admin::WILDCARD, governance::VOTE));
    }
}
