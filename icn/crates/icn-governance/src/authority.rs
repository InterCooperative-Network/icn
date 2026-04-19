//! Constitutional authority primitives — ADR-0014.
//!
//! This module introduces the typed application-layer substrate for
//! institutional authority in ICN. It is **types-first and behavior-neutral**:
//! no executor gating, no dispatcher wiring, no enforcement path. Future
//! tranches build on these types; this one only names them.
//!
//! # What lives here
//!
//! - [`AuthorityClass`] — closed enumeration of authority kinds
//!   (Representation / Execution / Attestation). Distinct by construction;
//!   the type system enforces the anti-capture rule that these three are
//!   not silently conflatable.
//! - [`AuthorityGrant`] — a bounded, auditable grant issued by a sovereign
//!   entity to a grantee, in exactly one [`AuthorityClass`].
//! - [`TypedScope`] — the typed replacement path for today's string-smeared
//!   `RoleAssignment.authority_scope: Vec<String>`. A conjunction of scope
//!   categories; absent categories are "unbounded along that axis" but a
//!   grant with no populated categories is malformed by construction.
//!
//! # Layer placement
//!
//! These types live at the **governance app layer** and must never be
//! imported by kernel crates. The Meaning Firewall remains intact: the
//! kernel enforces [`icn_kernel_api::authz::ConstraintSet`] and dispatches
//! [`icn_kernel_api::effects::KernelEffect`] without learning that
//! `AuthorityClass`, `AuthorityGrant`, or `TypedScope` exist.
//!
//! # What this is NOT
//!
//! - Not a replacement for `icn_kernel_api::authz::Capability` (the kernel
//!   bearer token). A future implementation may mint kernel capabilities
//!   from these grants, but they remain in distinct layers.
//! - Not a replacement for [`crate::delegation::Delegation`], which is
//!   strictly vote delegation (Representation authority, proposal-scoped).
//! - Not a replacement for [`crate::structure::RoleAssignment`], which
//!   continues to ship as a structural placement.
//! - Not a [`crate::mandate::Mandate`]. A grant is reusable authority;
//!   a mandate is a per-decision authorization that composes one or more
//!   grants with a specific decision's provenance.
//!
//! See `docs/adr/ADR-0014-constitutional-object-model.md` for full semantics.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::GovernanceDomainId;
use crate::proof::Hash;
use crate::Timestamp;
use icn_identity::Did;

/// Closed enumeration of constitutional authority kinds.
///
/// Exactly three variants, and only three, at the constitutional level.
/// These classes are **distinct and must not be silently conflated.** A
/// grant of one class does not confer the powers of another. Conflating
/// them is the usual mechanism of institutional capture; typing prevents
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityClass {
    /// Speaking or voting in place of another identity within a bounded
    /// scope. Does not permit executing effects or issuing attestations.
    Representation,
    /// Carrying out an action that mutates institutional state under a
    /// scope. Does not permit voting in place of others or issuing
    /// primary attestations.
    Execution,
    /// Issuing signed statements of fact or status that downstream
    /// consumers may rely on. Does not permit deciding policy or
    /// executing effects.
    Attestation,
}

/// Institutional unit for an amount-ceiling scope.
///
/// These are policy-governed institutional positions — not hosted balances,
/// not operator-routed value, not platform-issued currency. The enum is
/// closed over the two canonical cooperative units with an explicit
/// resource-type escape hatch for coop-defined resource allocations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "name")]
pub enum AmountUnit {
    /// Mutual-credit units carried on the ICN ledger as policy-governed
    /// positions.
    CreditUnits,
    /// Time-denominated labor contributions.
    LaborHours,
    /// Coop-defined resource allocation unit (e.g. `"bandwidth_gbps"`,
    /// `"storage_tib"`). The institutional meaning is defined by the
    /// grantor's charter, not by ICN.
    Resource(String),
}

/// A non-negative magnitude bound expressed in an institutional unit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmountCeiling {
    /// Ceiling value. Interpretation depends on `unit`.
    pub value: i64,
    /// The institutional unit the value is expressed in.
    pub unit: AmountUnit,
}

/// A half-open institutional time window `[start, end)`.
///
/// This is a finer-grained bound *within* a grant's outer `valid_from` /
/// `valid_until` — e.g. a budget grant that only applies during a specific
/// fiscal window, or a meeting-quorum representation grant that only
/// applies while the meeting is in session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeWindow {
    /// Unix timestamp (seconds) when the window opens.
    pub start: Timestamp,
    /// Unix timestamp (seconds) when the window closes.
    pub end: Timestamp,
}

/// The typed replacement path for today's `RoleAssignment.authority_scope:
/// Vec<String>` capability strings.
///
/// A `TypedScope` is a **conjunction** of the categories that are present.
/// Absent categories are "unbounded along that axis." A grant must have at
/// least one populated category to be meaningful; the [`TypedScope::is_empty`]
/// helper exists to catch the "unbounded on everything" malformation.
///
/// The semantic categories are frozen by ADR-0014; exact encoding may
/// refine at future implementation time (e.g. moving from `Option<T>`
/// fields to a `Vec<ScopeClause>`), but the categories below are stable.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypedScope {
    /// Governance-domain scope: the grant applies only within this domain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<GovernanceDomainId>,
    /// Proposal-class scope: the grant applies only to proposals whose
    /// payload tag matches one of these labels. Stable labels are
    /// proposal-payload-variant names (e.g. `"Treasury"`, `"Membership"`,
    /// `"DeployCharter"`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proposal_class: Vec<String>,
    /// Action-kind scope: the grant authorizes only these kernel-effect
    /// variant names (e.g. `"Treasury::Spend"`, `"Membership::Freeze"`).
    /// Applies to [`AuthorityClass::Execution`] grants only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub action_kind: Vec<String>,
    /// Amount / ceiling scope: magnitude bound in an institutional unit.
    /// Applies to [`AuthorityClass::Execution`] grants bounded by size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount_ceiling: Option<AmountCeiling>,
    /// Time-window scope: finer-grained time bound within the outer
    /// grant validity window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_window: Option<TimeWindow>,
}

impl TypedScope {
    /// Return `true` if no scope category is populated.
    ///
    /// A grant whose scope satisfies `is_empty()` is malformed by the
    /// ADR-0014 rule that at least one category must be populated.
    /// Callers constructing grants should reject this case.
    pub fn is_empty(&self) -> bool {
        self.domain.is_none()
            && self.proposal_class.is_empty()
            && self.action_kind.is_empty()
            && self.amount_ceiling.is_none()
            && self.time_window.is_none()
    }
}

/// Provenance link from an [`AuthorityGrant`] or [`crate::mandate::Mandate`]
/// to a specific governance decision.
///
/// Always carried as the pair `(proposal_id, decision_hash)` — the
/// proposal identifier plus the blake3 binding hash of the decision
/// receipt. Exact hashing lives in [`crate::proof::GovernanceDecisionReceipt`];
/// this type only records the reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionProvenance {
    /// Originating proposal identifier.
    pub proposal_id: String,
    /// Blake3 binding hash of the decision receipt.
    pub decision_hash: Hash,
}

/// The identifier of a sovereign entity acting as the source of an
/// authority grant.
///
/// A valid grantor is a **cooperative, community, or federation** acting
/// under its own charter's process. The ICN runtime, daemon, gateway, or
/// any non-entity platform component is **never** a valid grantor. This
/// newtype exists to give the grantor field a distinct, searchable type —
/// construction is unconstrained here, but downstream code that mints
/// `AuthorityGrant` must enforce that the identifier resolves to a
/// sovereign entity under the receiving deployment's entity registry.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GrantorEntityId(pub String);

impl std::fmt::Display for GrantorEntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The recipient of an authority grant.
///
/// Most grants are issued to a person (a [`Did`]). Grants may also be
/// issued to another entity (e.g. a federation grants a cooperative
/// authority to clear on its behalf) or to a shared-service entity acting
/// as an institutional actor (e.g. a cooperative grants a commons service
/// an attestation-class grant to issue personhood attestations bounded by
/// scope). Shared-service entities are legitimate grantees precisely
/// because they must be legible institutional actors, not invisible
/// platform conveniences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum Grantee {
    /// A person identified by DID.
    Person(Did),
    /// Another entity identified by its entity identifier (cooperative,
    /// community, federation, or shared-service entity).
    Entity(String),
}

/// Unique identifier for an [`AuthorityGrant`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AuthorityGrantId(pub Uuid);

impl AuthorityGrantId {
    /// Generate a fresh v4 identifier.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Wrap an existing UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for AuthorityGrantId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AuthorityGrantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A bounded, auditable grant of authority issued by a sovereign entity to
/// a grantee, in exactly one [`AuthorityClass`].
///
/// Grants descend from entities acting under their charters. They do not
/// descend from the platform, the runtime, the gateway, a daemon instance,
/// or a shared service. Every grant carries validity bounds and an
/// explicit revocation field: revocation is always possible by the
/// grantor entity.
///
/// This type is app-layer constitutional meaning. It is never imported by
/// kernel crates. See ADR-0014 for full semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityGrant {
    /// Stable identifier for the grant.
    pub id: AuthorityGrantId,
    /// Exactly one of Representation / Execution / Attestation.
    pub class: AuthorityClass,
    /// The sovereign entity that issues the grant.
    pub grantor: GrantorEntityId,
    /// The recipient of the grant.
    pub grantee: Grantee,
    /// The typed scope that bounds the grant. Must not be
    /// [`TypedScope::is_empty`].
    pub scope: TypedScope,
    /// Optional provenance back to the governance decision that produced
    /// this grant. `None` for charter-direct grants (e.g. ratified at
    /// charter adoption); `Some(_)` for decision-produced grants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub granted_by: Option<DecisionProvenance>,
    /// Unix timestamp when the grant takes effect.
    pub valid_from: Timestamp,
    /// Optional Unix timestamp when the grant expires. `None` means
    /// charter- or entity-lifetime-bounded; no grant is eternal without
    /// a named charter clause sustaining it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<Timestamp>,
    /// Unix timestamp of explicit revocation, if any. Revocation is
    /// always possible by the grantor entity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<Timestamp>,
}

impl AuthorityGrant {
    /// Return `true` if the grant is active at the given Unix timestamp.
    ///
    /// Active means: not revoked at or before `now`, `now >= valid_from`,
    /// and (if `valid_until` is set) `now < valid_until`.
    ///
    /// This is a time-check helper, **not** an enforcement gate. It does
    /// not consult any registry, charter, or state store. Future
    /// enforcement work will layer additional checks on top.
    pub fn is_active_at(&self, now: Timestamp) -> bool {
        if let Some(revoked_at) = self.revoked_at {
            if revoked_at <= now {
                return false;
            }
        }
        if now < self.valid_from {
            return false;
        }
        if let Some(valid_until) = self.valid_until {
            if now >= valid_until {
                return false;
            }
        }
        true
    }

    /// Return `true` if the grant has been explicitly revoked by `now`.
    pub fn is_revoked_at(&self, now: Timestamp) -> bool {
        matches!(self.revoked_at, Some(t) if t <= now)
    }
}

// ---------------------------------------------------------------------------
// Compatibility bridge: `authority_scope: Vec<String>` → `TypedScope`
// ---------------------------------------------------------------------------

/// Parse a vector of string capability labels into a [`TypedScope`].
///
/// This is the non-breaking bridge from today's
/// `RoleAssignment.authority_scope: Vec<String>` world toward the typed
/// scope model. It is intentionally conservative:
///
/// - Recognized label prefixes are merged into the returned `TypedScope`.
/// - Unrecognized labels are silently ignored (no error). A future
///   implementation is expected to count these as a metric for visibility
///   into migration progress.
/// - Returns `None` if and only if no label matched any recognized form.
///   This preserves the invariant that a `Some(scope)` result has at
///   least one populated scope category.
///
/// Recognized grammar (extensible in future tranches):
///
/// | Label form                          | Meaning                              |
/// |------------------------------------|--------------------------------------|
/// | `domain:<domain_id>`               | Sets `domain` scope.                 |
/// | `proposal_class:<class_name>`      | Appends to `proposal_class` scope.   |
/// | `action_kind:<kind_name>`          | Appends to `action_kind` scope.      |
/// | `amount_ceiling:<int>:<unit>`      | Sets `amount_ceiling` (credit_units / |
/// |                                    | labor_hours / `resource:<name>`).    |
///
/// Duplicate prefixes accumulate for `proposal_class` and `action_kind`;
/// the last `domain:` and `amount_ceiling:` entries win.
///
/// This function is intentionally not a behavioral enforcement gate. It
/// exists to let future consumers read a typed projection of existing
/// string scopes without forcing any call site to rewrite today.
pub fn parse_authority_scope_strings(labels: &[String]) -> Option<TypedScope> {
    let mut scope = TypedScope::default();
    let mut any_recognized = false;

    for label in labels {
        let Some((prefix, rest)) = label.split_once(':') else {
            continue;
        };
        match prefix {
            "domain" => {
                scope.domain = Some(GovernanceDomainId(rest.to_string()));
                any_recognized = true;
            }
            "proposal_class" => {
                if !rest.is_empty() {
                    scope.proposal_class.push(rest.to_string());
                    any_recognized = true;
                }
            }
            "action_kind" => {
                if !rest.is_empty() {
                    scope.action_kind.push(rest.to_string());
                    any_recognized = true;
                }
            }
            "amount_ceiling" => {
                // Expect `<int>:<unit>` in `rest`.
                let Some((value_str, unit_str)) = rest.split_once(':') else {
                    continue;
                };
                let Ok(value) = value_str.parse::<i64>() else {
                    continue;
                };
                let unit = match unit_str {
                    "credit_units" => AmountUnit::CreditUnits,
                    "labor_hours" => AmountUnit::LaborHours,
                    other => {
                        if let Some(res) = other.strip_prefix("resource:") {
                            AmountUnit::Resource(res.to_string())
                        } else {
                            continue;
                        }
                    }
                };
                scope.amount_ceiling = Some(AmountCeiling { value, unit });
                any_recognized = true;
            }
            _ => {}
        }
    }

    if any_recognized {
        Some(scope)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn did(seed: u8) -> Did {
        Did::from_anchor_id(&[seed; 32])
    }

    #[test]
    fn authority_class_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&AuthorityClass::Representation).unwrap(),
            "\"representation\""
        );
        assert_eq!(
            serde_json::to_string(&AuthorityClass::Execution).unwrap(),
            "\"execution\""
        );
        assert_eq!(
            serde_json::to_string(&AuthorityClass::Attestation).unwrap(),
            "\"attestation\""
        );

        let round: AuthorityClass = serde_json::from_str("\"execution\"").unwrap();
        assert_eq!(round, AuthorityClass::Execution);
    }

    #[test]
    fn typed_scope_default_is_empty() {
        let s = TypedScope::default();
        assert!(s.is_empty());
    }

    #[test]
    fn typed_scope_with_any_field_populated_is_not_empty() {
        let s = TypedScope {
            domain: Some(GovernanceDomainId("demo".into())),
            ..TypedScope::default()
        };
        assert!(!s.is_empty());

        let s = TypedScope {
            action_kind: vec!["Treasury::Spend".into()],
            ..TypedScope::default()
        };
        assert!(!s.is_empty());

        let s = TypedScope {
            amount_ceiling: Some(AmountCeiling {
                value: 100,
                unit: AmountUnit::CreditUnits,
            }),
            ..TypedScope::default()
        };
        assert!(!s.is_empty());
    }

    #[test]
    fn amount_unit_resource_variant_serde() {
        let u = AmountUnit::Resource("bandwidth_gbps".into());
        let json = serde_json::to_string(&u).unwrap();
        let back: AmountUnit = serde_json::from_str(&json).unwrap();
        assert_eq!(u, back);
    }

    #[test]
    fn grantee_person_and_entity_serde_roundtrip() {
        let p = Grantee::Person(did(1));
        let e = Grantee::Entity("coop:tech".into());
        let p2: Grantee = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        let e2: Grantee = serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
        assert_eq!(p, p2);
        assert_eq!(e, e2);
    }

    fn sample_grant() -> AuthorityGrant {
        AuthorityGrant {
            id: AuthorityGrantId::new(),
            class: AuthorityClass::Execution,
            grantor: GrantorEntityId("coop:tech".into()),
            grantee: Grantee::Person(did(1)),
            scope: TypedScope {
                action_kind: vec!["Treasury::Spend".into()],
                amount_ceiling: Some(AmountCeiling {
                    value: 1000,
                    unit: AmountUnit::CreditUnits,
                }),
                ..TypedScope::default()
            },
            granted_by: Some(DecisionProvenance {
                proposal_id: "prop-1".into(),
                decision_hash: [0u8; 32],
            }),
            valid_from: 100,
            valid_until: Some(1000),
            revoked_at: None,
        }
    }

    #[test]
    fn authority_grant_active_window() {
        let g = sample_grant();
        assert!(!g.is_active_at(50)); // before valid_from
        assert!(g.is_active_at(100)); // at valid_from
        assert!(g.is_active_at(500)); // in-window
        assert!(!g.is_active_at(1000)); // at valid_until (half-open)
        assert!(!g.is_active_at(2000)); // after valid_until
    }

    #[test]
    fn authority_grant_revocation_blocks_activity() {
        let mut g = sample_grant();
        g.revoked_at = Some(200);
        assert!(g.is_active_at(150)); // before revocation
        assert!(!g.is_active_at(200)); // at revocation
        assert!(!g.is_active_at(500)); // after revocation, still in-window
        assert!(g.is_revoked_at(200));
        assert!(!g.is_revoked_at(199));
    }

    #[test]
    fn authority_grant_roundtrip_serde() {
        let g = sample_grant();
        let json = serde_json::to_string(&g).unwrap();
        let back: AuthorityGrant = serde_json::from_str(&json).unwrap();
        assert_eq!(g, back);
    }

    #[test]
    fn parse_scope_strings_recognizes_domain_and_action_kind() {
        let labels = vec![
            "domain:treasury".to_string(),
            "action_kind:Treasury::Spend".to_string(),
            "action_kind:Treasury::Allocate".to_string(),
        ];
        let scope = parse_authority_scope_strings(&labels).unwrap();
        assert_eq!(scope.domain, Some(GovernanceDomainId("treasury".into())));
        assert_eq!(
            scope.action_kind,
            vec![
                "Treasury::Spend".to_string(),
                "Treasury::Allocate".to_string()
            ]
        );
        assert!(scope.proposal_class.is_empty());
        assert!(scope.amount_ceiling.is_none());
    }

    #[test]
    fn parse_scope_strings_recognizes_amount_ceiling() {
        let scope = parse_authority_scope_strings(&["amount_ceiling:500:credit_units".to_string()])
            .unwrap();
        assert_eq!(
            scope.amount_ceiling,
            Some(AmountCeiling {
                value: 500,
                unit: AmountUnit::CreditUnits,
            })
        );
    }

    #[test]
    fn parse_scope_strings_recognizes_resource_unit() {
        let scope = parse_authority_scope_strings(&[
            "amount_ceiling:10:resource:bandwidth_gbps".to_string()
        ])
        .unwrap();
        assert_eq!(
            scope.amount_ceiling,
            Some(AmountCeiling {
                value: 10,
                unit: AmountUnit::Resource("bandwidth_gbps".into()),
            })
        );
    }

    #[test]
    fn parse_scope_strings_ignores_unknown_and_returns_none_if_nothing_parsed() {
        let result = parse_authority_scope_strings(&[
            "unrecognized:foo".to_string(),
            "bareword".to_string(),
        ]);
        assert!(result.is_none());
    }

    #[test]
    fn parse_scope_strings_ignores_malformed_amount_ceiling() {
        // Missing unit
        assert!(parse_authority_scope_strings(&["amount_ceiling:100".to_string()]).is_none());
        // Non-integer value
        assert!(
            parse_authority_scope_strings(&["amount_ceiling:abc:credit_units".to_string()])
                .is_none()
        );
        // Unknown unit (no resource: prefix, no known bare unit)
        assert!(parse_authority_scope_strings(&["amount_ceiling:100:usd".to_string()]).is_none());
    }

    #[test]
    fn parse_scope_strings_mixes_recognized_and_unrecognized() {
        let labels = vec![
            "domain:coop-a".to_string(),
            "unknown:thing".to_string(),
            "proposal_class:Treasury".to_string(),
        ];
        let scope = parse_authority_scope_strings(&labels).unwrap();
        assert_eq!(scope.domain, Some(GovernanceDomainId("coop-a".into())));
        assert_eq!(scope.proposal_class, vec!["Treasury".to_string()]);
    }
}
