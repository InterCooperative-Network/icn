//! **TEST-SUPPORT ONLY — not an ICN primitive, not production authorization.**
//!
//! `ScopedAccessActV0`: an application-layer payload convention carried inside an existing N4-A
//! [`DeviceActV1`](icn_identity::device_authority::DeviceActV1) (capability `Present`), and an
//! offline relying-party checker that interprets it. Anchored to the N4 device-authorization spec
//! (icn#2599); the A1–A13 acceptance contract is stated in the pull request that introduced it.
//!
//! This file lives under `tests/` and is compiled only into integration-test binaries. It adds no
//! module, type, route or behavior to `icn-identity`. It consumes the existing public API exactly as
//! a downstream application would: `EvidenceBundle::decode` → `verify_evidence_bundle` (N1
//! admission + N4-A verification, unchanged) → this file's payload rules.
//!
//! # Division of labor
//!
//! - **ICN (unchanged)** decides WHO: the act's signature verifies under the device it names, and
//!   that device held a `Present` grant from the Subject at the relying party's position.
//! - **This relying party** decides WHAT: whether the Subject it evaluated for owns the requested
//!   resource (from the relying party's *own* ownership evidence, never the bundle), and whether the
//!   payload binds this audience, this resource, this action, personal scope, a valid window under
//!   the relying party's clock, and an unused nonce.
//!
//! A verifying signature is never treated as authorization, and N4-A authority over a Subject is
//! never treated as authority over a resource the relying party does not attribute to that Subject.
//!
//! # Payload canonical bytes
//!
//! N1 framing conventions, reimplemented here because N1's `Reader`/`Writer` are crate-private:
//! `LP(x) := u32be(len(x)) || x`, integers big-endian, fixed-width fields raw.
//!
//! ```text
//! scoped_access_act_v0 :=
//!       LP(SCOPED_ACCESS_DOMAIN)
//!    || u16be(SCOPED_ACCESS_VERSION)         -- 0; any other value is refused (UnsupportedVersion)
//!    || LP(audience)                         -- label
//!    || LP(resource_kind) || LP(resource_id) -- labels, audience-defined namespace
//!    || LP(action)                           -- label, audience-defined verb
//!    || u8(scope_tag)                        -- 0 personal 1 household 2 workgroup 3 cooperative 4 federation
//!    || b32(context_id)                      -- personal: must equal the evaluated Subject
//!    || u64be(not_before) || u64be(not_after) -- UTC seconds, signer-asserted
//!    || b16(nonce)
//!    || LP(purpose)                          -- label
//!
//! label := 1..=MAX_LABEL bytes of [a-z0-9._:/-]   -- one spelling per value; no case folding
//! ```
//!
//! Strict decode: wrong domain, a bad label, an unknown scope tag, truncation or trailing bytes are
//! all `PayloadMalformed`. One logical act therefore has exactly one encoding.
//!
//! # Known limits (deliberately surfaced, not hidden)
//!
//! - **Presenter binding depends on the configured rung** (see [`Freshness`], [`PresenterBinding`]).
//!   Under M1's `SignerNonce`, and under M2's `IssuedChallenge` + `Unbound`, the act is a **bearer
//!   credential**: whoever presents it first is allowed, and the owner's own later presentation is
//!   refused. `SessionChannel` stops capture-and-present-elsewhere but not a relaying
//!   man-in-the-middle. `DeviceChannelBinding` (existing N4-D `bind_channel`, nonce = the access
//!   act's `act_id`) also exposes the relay. Each rung is demonstrated by a test.
//! - **The channel rungs rest on transport assumptions that no ICN profile states today:**
//!   (1) every connection has its OWN channel digest (e.g. a TLS exporter value). A per-server
//!   value such as a certificate digest is identical for every client, so an interceptor's
//!   connection matches the victim's and interception succeeds again (demonstrated by a test);
//!   (2) `observed_channel` must come from the RP's transport, never from the caller (in this
//!   fixture it is a request field); (3) the RP must observe the channel at the TLS endpoint the
//!   device talks to, not behind a terminating proxy. Same-channel injection (malicious client
//!   code on the device's own connection) and a compromised device are not addressed by any rung.
//! - **Challenge issuance is unauthenticated and unbounded here.** The table is never pruned, so
//!   any caller can grow it without limit (memory exhaustion). A real RP needs issuance rate limits
//!   and expiry-based pruning against a monotonic high-water mark.
//! - `not_before`/`not_after` are **signer-asserted**. They bound how long a captured act is
//!   useful; they do not prove when it was signed. A compromised device can mint fresh windows.
//! - Revocation is visible only through the relying party's own authority view, which pins a
//!   position **and the head `event_id` at that position**. The `prev_digest` chain then pins the
//!   whole prefix, so a presenter cannot pick a fork branch or omit a known revoke. A Revoke above
//!   the pinned position cannot be seen (N4-B bundles refuse facts above `E`), and an allow
//!   carries that caveat. How the relying party obtains a trustworthy head is outside this
//!   fixture. Fail-closed cost: a bundle that honestly carries an unselectable extra body at `E` is
//!   refused.
//! - The replay seen-set is [`InMemoryReplayStore`]: lost on restart, not shared between
//!   independent verifiers. Two relying parties, or one restarted, can each accept the same act
//!   once. It never prunes, so a clock rollback cannot re-admit an act; the trade is unbounded
//!   growth. A production store must be durable and shared (or audience-partitioned), and must
//!   prune against a monotonic high-water mark, never the current clock reading.
//! - `Present` is the capability N4-D also uses for channel admission. A device granted `Present`
//!   for that purpose can also sign access acts for every personal resource of its Subject. The
//!   payload domains are separated; the authority is not. A narrower capability would be an N1
//!   change and is out of scope.
//! - Refusal classes are detailed for the audit record. Returning them to an unauthenticated
//!   caller would leak which resources exist and their scope. A production relying party should
//!   answer the caller with a uniform refusal.
//!
//! # Namespace
//!
//! [`SCOPED_ACCESS_DOMAIN`] is deliberately **not** `icn`-namespaced. ICN never interprets this
//! payload (the Meaning Firewall), so its separator belongs to the application layer. If the
//! convention is ever promoted into ICN, it must take an `icn.` separator **and** a row in
//! `icn-kernel-api/tests/domain_separation_audit.rs` `REGISTRY`.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use icn_identity::authority_log::{DeviceCapability, EventId, SubjectId};
use icn_identity::device_authority::{
    DeviceActVerifyError, DeviceAuthorityRefusal, VerifiedDeviceAct,
};
use icn_identity::device_channel_binding::{
    verify_channel_binding_at, ChannelBindingError, ChannelBindingV1,
};
use icn_identity::evidence_bundle::{
    verify_evidence_bundle, BundleError, EvidenceBundle, EvidenceVerifyError,
};
use rand_core::{OsRng, RngCore};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// Domain separator of the payload. Application-layer, not `icn`-namespaced (see "Namespace");
/// LP-framed, so it cannot be mistaken for any ICN separator. `fixture` marks it non-production.
pub const SCOPED_ACCESS_DOMAIN: &[u8] = b"app-fixture.scoped-access-act";
/// The only accepted payload version.
pub const SCOPED_ACCESS_VERSION: u16 = 0;
/// Upper bound on any label field.
pub const MAX_LABEL: usize = 128;
/// Nonce width.
pub const NONCE_LEN: usize = 16;
/// Audit record schema tag.
pub const AUDIT_SCHEMA: &str = "icn.fixture.scoped-access-audit.v0";

// ---------------------------------------------------------------------------------------------
// Payload
// ---------------------------------------------------------------------------------------------

/// The five scope names the contract recognizes. Only `Personal` is accepted in v0; the others
/// are named so they are refused explicitly rather than parsed as malformed or silently widened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    Personal,
    Household,
    Workgroup,
    Cooperative,
    Federation,
}

impl Scope {
    pub fn tag(self) -> u8 {
        match self {
            Scope::Personal => 0,
            Scope::Household => 1,
            Scope::Workgroup => 2,
            Scope::Cooperative => 3,
            Scope::Federation => 4,
        }
    }

    pub fn from_tag(tag: u8) -> Option<Scope> {
        match tag {
            0 => Some(Scope::Personal),
            1 => Some(Scope::Household),
            2 => Some(Scope::Workgroup),
            3 => Some(Scope::Cooperative),
            4 => Some(Scope::Federation),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Scope::Personal => "personal",
            Scope::Household => "household",
            Scope::Workgroup => "workgroup",
            Scope::Cooperative => "cooperative",
            Scope::Federation => "federation",
        }
    }
}

/// One application-scoped access act. Public fields; [`ScopedAccessActV0::encode`] refuses values
/// that would not round-trip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedAccessActV0 {
    pub audience: String,
    pub resource_kind: String,
    pub resource_id: String,
    pub action: String,
    pub scope: Scope,
    pub context_id: [u8; 32],
    pub not_before: u64,
    pub not_after: u64,
    pub nonce: [u8; NONCE_LEN],
    pub purpose: String,
}

/// Why payload bytes could not be interpreted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PayloadError {
    /// Not a strict canonical v0 payload. The string names the failing field (never payload bytes).
    Malformed(&'static str),
    /// Well-framed, but a version this relying party does not implement.
    UnsupportedVersion(u16),
}

fn label_ok(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_LABEL
        && s.bytes().all(|b| {
            b.is_ascii_lowercase()
                || b.is_ascii_digit()
                || matches!(b, b'.' | b'_' | b':' | b'/' | b'-')
        })
}

fn put_lp(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), PayloadError> {
    let len = u32::try_from(bytes.len()).map_err(|_| PayloadError::Malformed("length"))?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

impl ScopedAccessActV0 {
    /// Canonical bytes. Refuses any label that the decoder would refuse.
    pub fn encode(&self) -> Result<Vec<u8>, PayloadError> {
        self.encode_with_version(SCOPED_ACCESS_VERSION)
    }

    /// Canonical framing under an arbitrary version number (adversarial tests only).
    pub fn encode_with_version(&self, version: u16) -> Result<Vec<u8>, PayloadError> {
        for (name, value) in [
            ("audience", &self.audience),
            ("resource_kind", &self.resource_kind),
            ("resource_id", &self.resource_id),
            ("action", &self.action),
            ("purpose", &self.purpose),
        ] {
            if !label_ok(value) {
                return Err(PayloadError::Malformed(name));
            }
        }
        let mut out = Vec::new();
        put_lp(&mut out, SCOPED_ACCESS_DOMAIN)?;
        out.extend_from_slice(&version.to_be_bytes());
        put_lp(&mut out, self.audience.as_bytes())?;
        put_lp(&mut out, self.resource_kind.as_bytes())?;
        put_lp(&mut out, self.resource_id.as_bytes())?;
        put_lp(&mut out, self.action.as_bytes())?;
        out.push(self.scope.tag());
        out.extend_from_slice(&self.context_id);
        out.extend_from_slice(&self.not_before.to_be_bytes());
        out.extend_from_slice(&self.not_after.to_be_bytes());
        out.extend_from_slice(&self.nonce);
        put_lp(&mut out, self.purpose.as_bytes())?;
        Ok(out)
    }

    /// Strict decode. See the module docs for the grammar.
    pub fn decode(bytes: &[u8]) -> Result<Self, PayloadError> {
        let mut c = Cursor { bytes, at: 0 };
        if c.lp("domain")? != SCOPED_ACCESS_DOMAIN {
            return Err(PayloadError::Malformed("domain"));
        }
        // An unknown version is refused before its body is interpreted at all.
        let version = c.u16("version")?;
        if version != SCOPED_ACCESS_VERSION {
            return Err(PayloadError::UnsupportedVersion(version));
        }
        let audience = c.label("audience")?;
        let resource_kind = c.label("resource_kind")?;
        let resource_id = c.label("resource_id")?;
        let action = c.label("action")?;
        let scope = Scope::from_tag(c.u8("scope")?).ok_or(PayloadError::Malformed("scope"))?;
        let context_id = c.array::<32>("context_id")?;
        let not_before = u64::from_be_bytes(c.array::<8>("not_before")?);
        let not_after = u64::from_be_bytes(c.array::<8>("not_after")?);
        let nonce = c.array::<NONCE_LEN>("nonce")?;
        let purpose = c.label("purpose")?;
        if c.at != bytes.len() {
            return Err(PayloadError::Malformed("trailing bytes"));
        }
        Ok(ScopedAccessActV0 {
            audience,
            resource_kind,
            resource_id,
            action,
            scope,
            context_id,
            not_before,
            not_after,
            nonce,
            purpose,
        })
    }
}

/// Bounds-checked reader. Every read is `get`-based: no indexing, no panics on untrusted bytes.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize, field: &'static str) -> Result<&'a [u8], PayloadError> {
        let end = self
            .at
            .checked_add(n)
            .ok_or(PayloadError::Malformed(field))?;
        let slice = self
            .bytes
            .get(self.at..end)
            .ok_or(PayloadError::Malformed(field))?;
        self.at = end;
        Ok(slice)
    }

    fn array<const N: usize>(&mut self, field: &'static str) -> Result<[u8; N], PayloadError> {
        self.take(N, field)?
            .try_into()
            .map_err(|_| PayloadError::Malformed(field))
    }

    fn u8(&mut self, field: &'static str) -> Result<u8, PayloadError> {
        Ok(self.array::<1>(field)?[0])
    }

    fn u16(&mut self, field: &'static str) -> Result<u16, PayloadError> {
        Ok(u16::from_be_bytes(self.array::<2>(field)?))
    }

    fn lp(&mut self, field: &'static str) -> Result<&'a [u8], PayloadError> {
        let len = u32::from_be_bytes(self.array::<4>(field)?);
        let len = usize::try_from(len).map_err(|_| PayloadError::Malformed(field))?;
        self.take(len, field)
    }

    fn label(&mut self, field: &'static str) -> Result<String, PayloadError> {
        let raw = self.lp(field)?;
        let s = std::str::from_utf8(raw).map_err(|_| PayloadError::Malformed(field))?;
        if !label_ok(s) {
            return Err(PayloadError::Malformed(field));
        }
        Ok(s.to_string())
    }
}

// ---------------------------------------------------------------------------------------------
// Relying-party inputs: its own evidence, policy, clock and replay state
// ---------------------------------------------------------------------------------------------

/// A resource in the audience's namespace.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ResourceKey {
    pub kind: String,
    pub id: String,
}

impl ResourceKey {
    pub fn new(kind: &str, id: &str) -> Self {
        ResourceKey {
            kind: kind.to_string(),
            id: id.to_string(),
        }
    }
}

/// The relying party's own record of who controls a resource. **Fixture evidence**: in a real
/// application this is the application's own data, never something the requester supplies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnershipEvidence {
    pub owner: SubjectId,
    pub scope: Scope,
}

/// The relying party's own last-observed view of a Subject's authority log: the position it
/// evaluates at, the `event_id` of the selected body at that position, and when (by its own
/// clock) it last confirmed nothing newer was retained. Its revocation horizon. Never taken from
/// the bundle. Pinning `head` (not only `position`) is what stops a presenter from choosing which
/// facts at or below `E` the verifier folds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrustedAuthorityView {
    pub position: u64,
    pub head: EventId,
    pub observed_at: u64,
}

/// Synthetic-demo policy values. **Test policy, not a production security guarantee.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelyingPartyPolicy {
    /// This relying party's own audience id.
    pub audience: String,
    /// Verbs this audience defines, per resource kind.
    pub allowed_actions: BTreeMap<String, BTreeSet<String>>,
    /// Purpose codes this audience accepts.
    pub allowed_purposes: BTreeSet<String>,
    /// Maximum `not_after - not_before` (v0 demo: 300 s).
    pub max_lifetime_secs: u64,
    /// Clock-skew allowance (v0 demo: 60 s).
    pub clock_skew_secs: u64,
    /// How old the relying party's authority view may be before it refuses (revocation horizon).
    pub max_authority_view_age_secs: u64,
    /// How freshness is established (M1 signer nonce, or M2 RP-issued challenge).
    pub freshness: Freshness,
    /// What ties the act to its presenter (meaningful only with [`Freshness::IssuedChallenge`]).
    pub presenter: PresenterBinding,
    /// Lifetime of an issued challenge, by the RP's own clock (M2 experiment: 120 s).
    pub challenge_ttl_secs: u64,
}

/// How the relying party establishes that an access act is fresh and single-use.
///
/// **[EXPERIMENT]** Both modes are test policies. `SignerNonce` is kept only as the M1 baseline the
/// M2 attacks are measured against; it is not a recommended production mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// M1: the signer chooses the nonce; a per-verifier seen-set rejects repeats.
    SignerNonce,
    /// M2: the payload nonce must be a challenge THIS relying party issued, still outstanding and
    /// unexpired. The challenge table is the replay state; a challenge is consumed only on allow.
    IssuedChallenge,
}

/// What ties an access act to the party presenting it. Three rungs, each closing one more attack;
/// the tests demonstrate what each rung leaves open rather than asserting a rung is sufficient.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenterBinding {
    /// The challenge is a bearer value issued to any caller: the first presenter of a signed act
    /// over it wins (interception).
    Unbound,
    /// The RP ties each challenge to the channel it issued it on and accepts the act only on that
    /// channel. Stops capture-and-present-elsewhere. Does NOT stop a relaying man-in-the-middle,
    /// which obtains its own challenge on its own channel and relays it to the device.
    SessionChannel,
    /// `SessionChannel`, plus an existing N4-D channel binding (`bind_channel`) by the SAME device,
    /// over the channel the DEVICE sees, whose nonce is the access act's `act_id`. A relay is
    /// exposed because the device's channel differs from the RP's. Only as strong as the transport
    /// profile that defines the channel digest (none exists for application connections).
    DeviceChannelBinding,
}

impl Freshness {
    pub fn name(self) -> &'static str {
        match self {
            Freshness::SignerNonce => "signer_nonce",
            Freshness::IssuedChallenge => "issued_challenge",
        }
    }
}

impl PresenterBinding {
    pub fn name(self) -> &'static str {
        match self {
            PresenterBinding::Unbound => "unbound",
            PresenterBinding::SessionChannel => "session_channel",
            PresenterBinding::DeviceChannelBinding => "device_channel_binding",
        }
    }

    fn requires_channel(self) -> bool {
        !matches!(self, PresenterBinding::Unbound)
    }
}

/// An RP-issued challenge: carried in the payload's 16-byte nonce slot (v0 codec unchanged).
pub type Challenge = [u8; NONCE_LEN];

/// Where challenges come from. Production-like issuance uses [`OsChallengeSource`]; deterministic
/// audit tests inject [`FixedChallengeSource`]. `None` means the source cannot produce a value.
pub trait ChallengeSource {
    fn next_challenge(&mut self) -> Option<Challenge>;
}

/// 128 bits from the operating system CSPRNG (`rand_core::OsRng`). No scheme of its own.
#[derive(Debug, Default)]
pub struct OsChallengeSource;

impl ChallengeSource for OsChallengeSource {
    fn next_challenge(&mut self) -> Option<Challenge> {
        let mut c = [0u8; NONCE_LEN];
        OsRng.try_fill_bytes(&mut c).ok()?;
        Some(c)
    }
}

/// A fixed sequence, for deterministic tests only. **Predictable by construction.**
#[derive(Debug, Default)]
pub struct FixedChallengeSource(pub Vec<Challenge>);

impl ChallengeSource for FixedChallengeSource {
    fn next_challenge(&mut self) -> Option<Challenge> {
        if self.0.is_empty() {
            None
        } else {
            Some(self.0.remove(0))
        }
    }
}

/// One issued challenge, as the issuing RP recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChallengeRecord {
    /// The audience that issued it. A record from another audience is never accepted.
    pub audience: String,
    /// The channel the RP issued it on, when the presenter mode binds sessions.
    pub channel: Option<[u8; 32]>,
    pub issued_at: u64,
    pub expires_at: u64,
    pub consumed: bool,
}

/// **Limitation, disclosed:** in-memory challenge table. Lost on restart, which invalidates every
/// outstanding challenge (fail-closed). Never pruned. Not shared with any other verifier, which
/// is exactly why another verifier refuses an act over this verifier's challenge.
#[derive(Debug, Default)]
pub struct InMemoryChallengeTable {
    pub records: BTreeMap<Challenge, ChallengeRecord>,
    /// When true, the table cannot be read or written (models an unavailable durable store).
    pub unavailable: bool,
}

/// An explicitly injected verifier clock. `None` means no trustworthy reading is available.
pub trait VerifierClock {
    fn now_unix_secs(&self) -> Option<u64>;
}

/// A fixed fixture clock.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock(pub Option<u64>);

impl VerifierClock for FixedClock {
    fn now_unix_secs(&self) -> Option<u64> {
        self.0
    }
}

/// The replay seen-set key.
pub type ReplayKey = ([u8; 32], [u8; NONCE_LEN]);

/// Result of an atomic check-and-record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayOutcome {
    Fresh,
    Seen,
}

/// The seen-set could not be consulted. Refuses; never degrades open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayUnavailable;

/// A relying party's replay seen-set.
pub trait ReplayStore {
    /// Atomically: if `key` is present, `Seen`; otherwise record it (with the time after which
    /// the act would be refused as expired anyway) and return `Fresh`.
    fn consume(
        &mut self,
        key: ReplayKey,
        expires_at: u64,
    ) -> Result<ReplayOutcome, ReplayUnavailable>;

    /// The audit caveat naming this store's durability and sharing scope.
    fn caveat(&self) -> &'static str;
}

/// **Limitation, disclosed:** process memory only. Lost on restart; not shared with any other
/// verifier. Never prunes (see the module's known limits). Sufficient to demonstrate the rule,
/// insufficient for production.
#[derive(Debug, Default)]
pub struct InMemoryReplayStore {
    pub seen: BTreeMap<ReplayKey, u64>,
    /// When true, every call reports [`ReplayUnavailable`] (models a store that cannot be read).
    pub unavailable: bool,
}

impl ReplayStore for InMemoryReplayStore {
    fn consume(
        &mut self,
        key: ReplayKey,
        expires_at: u64,
    ) -> Result<ReplayOutcome, ReplayUnavailable> {
        if self.unavailable {
            return Err(ReplayUnavailable);
        }
        if self.seen.contains_key(&key) {
            return Ok(ReplayOutcome::Seen);
        }
        self.seen.insert(key, expires_at);
        Ok(ReplayOutcome::Fresh)
    }

    fn caveat(&self) -> &'static str {
        "replay_state:in_memory_single_verifier"
    }
}

// ---------------------------------------------------------------------------------------------
// Decision and audit
// ---------------------------------------------------------------------------------------------

/// Every refusal class. Stable names; never carry payload bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    // ---- malformed (cannot be evaluated) ----
    /// The caller's requested resource or action is not a valid label.
    RequestMalformed,
    /// The presented bytes are not a strict canonical N4-B bundle.
    BundleMalformed,
    /// The verified act's payload is not a strict canonical v0 payload.
    PayloadMalformed,
    // ---- relying party's own evidence missing / unusable ----
    /// No ownership evidence for the requested resource.
    UnknownResource,
    /// Ownership evidence exists but is not personal-scoped (v0).
    UnsupportedOwnershipScope,
    /// No trusted authority view for the owner: revocation state unknown.
    AuthorityViewUnavailable,
    /// The authority view is older than policy allows (or from the future).
    AuthorityViewStale,
    /// No trustworthy clock reading.
    ClockUnavailable,
    // ---- N1 / N4-A (unchanged ICN verdicts) ----
    InadmissibleFact,
    N4(&'static str),
    /// N4-A verified, but the bundle's body at `E` is not exactly the relying party's pinned head.
    AuthorityHeadMismatch,
    // ---- payload policy ----
    WrongCapability,
    UnsupportedVersion,
    WrongAudience,
    ScopeNotSupported(Scope),
    ContextMismatch,
    ResourceMismatch,
    ActionMismatch,
    UnknownAction,
    PurposeNotAllowed,
    InvalidWindow,
    LifetimeExceeded,
    NotYetValid,
    Expired,
    ReplayStateUnavailable,
    Replayed,
    // ---- M2: issued challenge ----
    /// The challenge table cannot be consulted.
    ChallengeStateUnavailable,
    /// The challenge source could not produce a value, or produced one already issued.
    ChallengeIssuanceFailed,
    /// The payload nonce is not a challenge this RP issued (includes signer-chosen nonces).
    ChallengeUnknown,
    /// The challenge was already consumed by an allowed request.
    ChallengeConsumed,
    /// The record was issued under another audience.
    ChallengeAudienceMismatch,
    /// The RP's clock is past the challenge's expiry.
    ChallengeExpired,
    /// The RP's clock reads earlier than the challenge's issuance (clock moved back).
    ChallengeNotYetIssued,
    /// The presenter mode needs the channel the request arrived on, and none was supplied.
    ChannelUnavailable,
    /// The policy asks for presenter binding under signer-chosen nonces, which cannot provide it.
    /// Refused rather than silently ignored.
    PolicyInconsistent,
    /// The request arrived on a channel other than the one the challenge was issued on.
    ChallengeChannelMismatch,
    // ---- M2: device channel binding ----
    PresenterBindingMissing,
    PresenterBindingMalformed,
    PresenterInadmissibleFact,
    /// The binding act is not a `Present` act.
    PresenterWrongCapability,
    /// N4-A refused the binding act (class name as N4).
    PresenterN4(&'static str),
    PresenterHeadMismatch,
    /// The device bound a channel other than the one this RP sees (relay / MITM).
    PresenterChannelMismatch,
    /// The binding was signed by a different device than the access act.
    PresenterDeviceMismatch,
    /// The binding's nonce is not this access act's `act_id`.
    PresenterBindingNotForThisAct,
}

impl Refusal {
    pub fn class(&self) -> String {
        match self {
            Refusal::N4(name) => (*name).to_string(),
            Refusal::PresenterN4(name) => format!("Presenter:{name}"),
            Refusal::ScopeNotSupported(scope) => format!("ScopeNotSupported:{}", scope.name()),
            other => format!("{other:?}"),
        }
    }

    /// `malformed` for bytes that could not be evaluated, `refused` for everything else.
    pub fn verdict(&self) -> &'static str {
        match self {
            Refusal::RequestMalformed
            | Refusal::BundleMalformed
            | Refusal::PayloadMalformed
            | Refusal::PresenterBindingMalformed => "malformed",
            _ => "refused",
        }
    }
}

/// What the relying party is asked to permit right now.
#[derive(Debug, Clone)]
pub struct AccessRequest<'a> {
    pub bundle: &'a [u8],
    pub resource: ResourceKey,
    pub action: String,
    /// The channel digest THIS relying party observes the request on (transport-supplied).
    pub observed_channel: Option<[u8; 32]>,
    /// A canonical N4-B bundle carrying the device's N4-D channel-binding act.
    pub presenter_binding: Option<&'a [u8]>,
}

/// One decision with its audit record.
#[derive(Debug, Clone)]
pub struct Decision {
    pub outcome: Result<(), Refusal>,
    pub audit: AuditRecord,
}

impl Decision {
    pub fn allowed(&self) -> bool {
        self.outcome.is_ok()
    }

    pub fn refusal(&self) -> Option<Refusal> {
        self.outcome.err()
    }
}

/// A deterministic, append-only, hash-chained audit record. Contains digests, identifiers and
/// classes only: never secret key material or signatures, never the raw payload, never the nonce.
/// It does carry the device's public key. Requested labels are logged only if they are valid
/// labels; otherwise only their digest is logged. The chain detects a rewrite only relative to a
/// trusted head; this fixture's log is an unanchored `Vec`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRecord {
    /// Canonical JSON bytes of the record body.
    pub bytes: Vec<u8>,
    /// `SHA-256(prev_hash || bytes)`.
    pub hash: [u8; 32],
}

impl AuditRecord {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.bytes).unwrap_or(Value::Null)
    }
}

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().into()
}

/// Facts gathered during one evaluation, rendered into the audit record.
#[derive(Debug, Default)]
struct Trace {
    bundle_id: Option<[u8; 32]>,
    evaluated_subject: Option<SubjectId>,
    evaluation_position: Option<u64>,
    clock_reading: Option<u64>,
    verified: Option<VerifiedSummary>,
    challenge_sha256: Option<[u8; 32]>,
    presenter_act_id: Option<[u8; 32]>,
}

#[derive(Debug)]
struct VerifiedSummary {
    act_id: [u8; 32],
    device: [u8; 32],
    granted_at: u64,
    generation: u64,
    payload_sha256: [u8; 32],
}

// ---------------------------------------------------------------------------------------------
// The relying party
// ---------------------------------------------------------------------------------------------

/// An offline relying party: no network, no daemon, no ICN store of its own. Everything it relies
/// on is either in the presented bundle (verified by unchanged N1/N4-A code) or its own state.
pub struct RelyingParty<C: VerifierClock, R: ReplayStore> {
    pub policy: RelyingPartyPolicy,
    pub ownership: BTreeMap<ResourceKey, OwnershipEvidence>,
    pub authority_views: BTreeMap<SubjectId, TrustedAuthorityView>,
    pub clock: C,
    pub replay: R,
    pub challenges: InMemoryChallengeTable,
    pub challenge_source: Box<dyn ChallengeSource>,
    pub audit_log: Vec<AuditRecord>,
}

impl<C: VerifierClock, R: ReplayStore> RelyingParty<C, R> {
    /// Decide one request and append its audit record. Never panics; every path is a decision.
    pub fn decide(&mut self, request: &AccessRequest<'_>) -> Decision {
        let mut trace = Trace::default();
        let outcome = self.evaluate(request, &mut trace);
        let audit = self.append_audit(request, &trace, outcome);
        Decision { outcome, audit }
    }

    /// Order of checks, each fail-closed. The relying party's own evidence (ownership, authority
    /// view, clock) is established before the bundle's act is trusted for anything; the act's
    /// payload is read only from the bytes whose signature and authority N4-A just verified.
    fn evaluate(&mut self, request: &AccessRequest<'_>, trace: &mut Trace) -> Result<(), Refusal> {
        // 0. The caller's own request must be well-formed labels (bounds what the audit logs).
        if !request_labels_ok(request) {
            return Err(Refusal::RequestMalformed);
        }
        // 1. Structure: a strict canonical N4-B bundle (unchanged ICN decoder).
        let bundle =
            EvidenceBundle::decode(request.bundle).map_err(|_| Refusal::BundleMalformed)?;
        trace.bundle_id = Some(bundle.bundle_id());

        // 2. Resource ownership from the RP's OWN evidence. The Subject to evaluate for is the
        //    resource's owner — never the Subject the bundle or act names.
        let ownership = self
            .ownership
            .get(&request.resource)
            .ok_or(Refusal::UnknownResource)?;
        if ownership.scope != Scope::Personal {
            return Err(Refusal::UnsupportedOwnershipScope);
        }
        let owner = ownership.owner;
        trace.evaluated_subject = Some(owner);

        // 3. The RP's revocation horizon for that owner, and its clock. Unknown is not "fine".
        let view = *self
            .authority_views
            .get(&owner)
            .ok_or(Refusal::AuthorityViewUnavailable)?;
        trace.evaluation_position = Some(view.position);
        let now = self
            .clock
            .now_unix_secs()
            .ok_or(Refusal::ClockUnavailable)?;
        trace.clock_reading = Some(now);
        let skew = self.policy.clock_skew_secs;
        if view.observed_at > now.saturating_add(skew)
            || now.saturating_sub(view.observed_at) > self.policy.max_authority_view_age_secs
        {
            return Err(Refusal::AuthorityViewStale);
        }

        // 4. WHO: N1 admission + N4-A verification, unchanged, at the RP's (owner, position).
        let verified = match verify_evidence_bundle(&bundle, owner, view.position) {
            Ok(verified) => verified,
            Err(EvidenceVerifyError::Bundle(BundleError::Inadmissible { .. })) => {
                return Err(Refusal::InadmissibleFact)
            }
            Err(EvidenceVerifyError::Bundle(_)) => return Err(Refusal::BundleMalformed),
            Err(EvidenceVerifyError::Act(err)) => return Err(Refusal::N4(n4_class(&err))),
        };
        // 4b. The facts N4-A folded must be the RP's facts, not the presenter's choice. The bundle
        //     must carry exactly one body at E and it must be the pinned head. N4-A just proved
        //     the prefix is Live through E, so the fold selected that body at E. `prev_digest`
        //     linkage then pins every position below it to the RP's own chain.
        if !head_pinned(&bundle, view) {
            return Err(Refusal::AuthorityHeadMismatch);
        }
        let act = verified.act();
        trace.verified = Some(VerifiedSummary {
            act_id: verified.act_id(),
            device: act.device.as_bytes(),
            granted_at: verified.evidence().grant.granted_at,
            generation: verified.evidence().generation,
            payload_sha256: sha256(&act.payload),
        });

        // 5. WHAT: the payload of the verified bytes, against this RP's request and policy.
        if act.capability != DeviceCapability::Present {
            return Err(Refusal::WrongCapability);
        }
        let p = ScopedAccessActV0::decode(&act.payload).map_err(|e| match e {
            PayloadError::Malformed(_) => Refusal::PayloadMalformed,
            PayloadError::UnsupportedVersion(_) => Refusal::UnsupportedVersion,
        })?;
        if p.audience != self.policy.audience {
            return Err(Refusal::WrongAudience);
        }
        if p.scope != Scope::Personal {
            return Err(Refusal::ScopeNotSupported(p.scope));
        }
        if p.context_id != *owner.as_bytes() {
            return Err(Refusal::ContextMismatch);
        }
        if p.resource_kind != request.resource.kind || p.resource_id != request.resource.id {
            return Err(Refusal::ResourceMismatch);
        }
        if p.action != request.action {
            return Err(Refusal::ActionMismatch);
        }
        let known_action = self
            .policy
            .allowed_actions
            .get(&p.resource_kind)
            .is_some_and(|verbs| verbs.contains(&p.action));
        if !known_action {
            return Err(Refusal::UnknownAction);
        }
        if !self.policy.allowed_purposes.contains(&p.purpose) {
            return Err(Refusal::PurposeNotAllowed);
        }

        // 6. Window under the RP's clock. Signer-asserted bounds; see module docs.
        if p.not_after <= p.not_before {
            return Err(Refusal::InvalidWindow);
        }
        if p.not_after - p.not_before > self.policy.max_lifetime_secs {
            return Err(Refusal::LifetimeExceeded);
        }
        if now.saturating_add(skew) < p.not_before {
            return Err(Refusal::NotYetValid);
        }
        if now > p.not_after.saturating_add(skew) {
            return Err(Refusal::Expired);
        }

        // 7. Freshness and presenter binding, last: only an otherwise-allowed act consumes its
        //    nonce or challenge, so a refused presentation (e.g. by an observer, against the wrong
        //    action) cannot burn it.
        match self.policy.freshness {
            Freshness::SignerNonce if self.policy.presenter != PresenterBinding::Unbound => {
                Err(Refusal::PolicyInconsistent)
            }
            Freshness::SignerNonce => {
                let key = (*owner.as_bytes(), p.nonce);
                match self.replay.consume(key, p.not_after.saturating_add(skew)) {
                    Err(ReplayUnavailable) => Err(Refusal::ReplayStateUnavailable),
                    Ok(ReplayOutcome::Seen) => Err(Refusal::Replayed),
                    Ok(ReplayOutcome::Fresh) => Ok(()),
                }
            }
            Freshness::IssuedChallenge => {
                trace.challenge_sha256 = Some(sha256(&p.nonce));
                self.check_challenge(&p.nonce, request, now)?;
                if self.policy.presenter == PresenterBinding::DeviceChannelBinding {
                    self.check_presenter_binding(request, owner, view, &verified, trace)?;
                }
                self.consume_challenge(&p.nonce)
            }
        }
    }

    /// Issue a fresh challenge. `session_channel` is the channel this RP sees the requesting
    /// session on; it is required (and recorded) when the presenter mode binds sessions.
    pub fn issue_challenge(
        &mut self,
        session_channel: Option<[u8; 32]>,
    ) -> Result<Challenge, Refusal> {
        let now = self
            .clock
            .now_unix_secs()
            .ok_or(Refusal::ClockUnavailable)?;
        if self.policy.freshness != Freshness::IssuedChallenge {
            return Err(Refusal::PolicyInconsistent);
        }
        if self.challenges.unavailable {
            return Err(Refusal::ChallengeStateUnavailable);
        }
        if self.policy.presenter.requires_channel() && session_channel.is_none() {
            return Err(Refusal::ChannelUnavailable);
        }
        let challenge = self
            .challenge_source
            .next_challenge()
            .ok_or(Refusal::ChallengeIssuanceFailed)?;
        if self.challenges.records.contains_key(&challenge) {
            return Err(Refusal::ChallengeIssuanceFailed);
        }
        let channel = if self.policy.presenter.requires_channel() {
            session_channel
        } else {
            None
        };
        self.challenges.records.insert(
            challenge,
            ChallengeRecord {
                audience: self.policy.audience.clone(),
                channel,
                issued_at: now,
                expires_at: now.saturating_add(self.policy.challenge_ttl_secs),
                consumed: false,
            },
        );
        Ok(challenge)
    }

    fn check_challenge(
        &self,
        challenge: &Challenge,
        request: &AccessRequest<'_>,
        now: u64,
    ) -> Result<(), Refusal> {
        if self.challenges.unavailable {
            return Err(Refusal::ChallengeStateUnavailable);
        }
        let record = self
            .challenges
            .records
            .get(challenge)
            .ok_or(Refusal::ChallengeUnknown)?;
        if record.audience != self.policy.audience {
            return Err(Refusal::ChallengeAudienceMismatch);
        }
        if record.consumed {
            return Err(Refusal::ChallengeConsumed);
        }
        if now < record.issued_at {
            return Err(Refusal::ChallengeNotYetIssued);
        }
        if now > record.expires_at {
            return Err(Refusal::ChallengeExpired);
        }
        if self.policy.presenter.requires_channel() {
            let observed = request
                .observed_channel
                .ok_or(Refusal::ChannelUnavailable)?;
            if record.channel != Some(observed) {
                return Err(Refusal::ChallengeChannelMismatch);
            }
        }
        Ok(())
    }

    /// Existing primitives only: N4-B decode + N1 admission, N4-D `verify_channel_binding_at` at
    /// the RP's pinned position (which runs N4-A and the channel equality), the same head pin as
    /// the access act, then two equalities linking the binding to THIS access act.
    fn check_presenter_binding(
        &self,
        request: &AccessRequest<'_>,
        owner: SubjectId,
        view: TrustedAuthorityView,
        access: &VerifiedDeviceAct,
        trace: &mut Trace,
    ) -> Result<(), Refusal> {
        let observed = request
            .observed_channel
            .ok_or(Refusal::ChannelUnavailable)?;
        let bytes = request
            .presenter_binding
            .ok_or(Refusal::PresenterBindingMissing)?;
        let binding_bundle =
            EvidenceBundle::decode(bytes).map_err(|_| Refusal::PresenterBindingMalformed)?;
        let store = binding_bundle
            .admit()
            .map_err(|_| Refusal::PresenterInadmissibleFact)?;
        let verified = verify_channel_binding_at(
            binding_bundle.act(),
            owner,
            &store,
            view.position,
            &observed,
        )
        .map_err(|e| match e {
            ChannelBindingError::ChannelMismatch => Refusal::PresenterChannelMismatch,
            ChannelBindingError::NotAPresentation => Refusal::PresenterWrongCapability,
            ChannelBindingError::Payload(_) => Refusal::PresenterBindingMalformed,
            ChannelBindingError::Act(err) => Refusal::PresenterN4(n4_class(&err)),
            ChannelBindingError::Admission(_) => Refusal::PresenterN4("Admission"),
        })?;
        if !head_pinned(&binding_bundle, view) {
            return Err(Refusal::PresenterHeadMismatch);
        }
        if verified.act().device != access.act().device {
            return Err(Refusal::PresenterDeviceMismatch);
        }
        let binding = ChannelBindingV1::decode(&verified.act().payload)
            .map_err(|_| Refusal::PresenterBindingMalformed)?;
        if binding.nonce != access.act_id() {
            return Err(Refusal::PresenterBindingNotForThisAct);
        }
        trace.presenter_act_id = Some(verified.act_id());
        Ok(())
    }

    /// Atomic only because this fixture is single-threaded and in-memory (`&mut self` for the whole
    /// decision). A durable or shared store needs ONE compare-and-set over the record state,
    /// committed with the audit append; this re-check of `consumed` is that shape, not that guarantee.
    fn consume_challenge(&mut self, challenge: &Challenge) -> Result<(), Refusal> {
        if self.challenges.unavailable {
            return Err(Refusal::ChallengeStateUnavailable);
        }
        match self.challenges.records.get_mut(challenge) {
            None => Err(Refusal::ChallengeUnknown),
            Some(record) if record.consumed => Err(Refusal::ChallengeConsumed),
            Some(record) => {
                record.consumed = true;
                Ok(())
            }
        }
    }

    fn append_audit(
        &mut self,
        request: &AccessRequest<'_>,
        trace: &Trace,
        outcome: Result<(), Refusal>,
    ) -> AuditRecord {
        let prev_hash = self.audit_log.last().map(|r| r.hash).unwrap_or([0u8; 32]);
        let (verdict, class) = match outcome {
            Ok(()) => ("allow", Value::Null),
            Err(refusal) => (refusal.verdict(), Value::String(refusal.class())),
        };
        let caveats: Vec<String> = match (outcome, trace.evaluation_position) {
            (Ok(()), Some(position)) => {
                let mut c = vec![
                    format!("authority_as_of_position:{position}"),
                    "revocation_above_position_not_visible".to_string(),
                    "window_is_signer_asserted".to_string(),
                ];
                match (self.policy.freshness, self.policy.presenter) {
                    (Freshness::SignerNonce, _) => {
                        c.push(self.replay.caveat().to_string());
                        c.push("bearer_act_not_bound_to_presenter".to_string());
                    }
                    (Freshness::IssuedChallenge, presenter) => {
                        c.push("challenge_state:in_memory_single_verifier".to_string());
                        c.push(
                            match presenter {
                                PresenterBinding::Unbound => {
                                    "challenge_is_bearer:first_presenter_wins"
                                }
                                PresenterBinding::SessionChannel => {
                                    "session_channel_only:relay_not_prevented"
                                }
                                PresenterBinding::DeviceChannelBinding => {
                                    "channel_binding_depends_on_transport_profile"
                                }
                            }
                            .to_string(),
                        );
                    }
                }
                c
            }
            _ => Vec::new(),
        };
        let hex32 = |b: &[u8; 32]| Value::String(hex::encode(b));
        let body = json!({
            "schema": AUDIT_SCHEMA,
            "sequence": self.audit_log.len(),
            "prev_hash": hex::encode(prev_hash),
            "audience": self.policy.audience,
            "verdict": verdict,
            "class": class,
            "presented_sha256": hex::encode(sha256(request.bundle)),
            "bundle_id": trace.bundle_id.as_ref().map(hex32),
            "requested": if request_labels_ok(request) {
                json!({
                    "resource_kind": request.resource.kind,
                    "resource_id": request.resource.id,
                    "action": request.action,
                })
            } else {
                let mut joined = Vec::new();
                for field in [&request.resource.kind, &request.resource.id, &request.action] {
                    joined.extend_from_slice(&(field.len() as u64).to_be_bytes());
                    joined.extend_from_slice(field.as_bytes());
                }
                json!({ "invalid_labels_sha256": hex::encode(sha256(&joined)) })
            },
            "evaluated_subject": trace.evaluated_subject.as_ref().map(|s| hex32(s.as_bytes())),
            "evaluation_position": trace.evaluation_position,
            "clock_reading": trace.clock_reading,
            "verified_act": trace.verified.as_ref().map(|v| json!({
                "act_id": hex::encode(v.act_id),
                "device": hex::encode(v.device),
                "grant_granted_at": v.granted_at,
                "generation": v.generation,
                "payload_sha256": hex::encode(v.payload_sha256),
            })),
            "freshness": self.policy.freshness.name(),
            "presenter_binding": self.policy.presenter.name(),
            "challenge_sha256": trace.challenge_sha256.as_ref().map(hex32),
            "observed_channel_sha256": request.observed_channel.as_ref().map(|c| hex::encode(sha256(c))),
            "presenter_binding_act_id": trace.presenter_act_id.as_ref().map(hex32),
            "caveats": caveats,
        });
        // serde_json serialization of a fixed-shape Value is deterministic for identical inputs.
        let bytes = serde_json::to_vec(&body).unwrap_or_default();
        let mut chained = Vec::with_capacity(32 + bytes.len());
        chained.extend_from_slice(&prev_hash);
        chained.extend_from_slice(&bytes);
        let record = AuditRecord {
            hash: sha256(&chained),
            bytes,
        };
        self.audit_log.push(record.clone());
        record
    }
}

/// The bundle carries exactly one body at the RP's position, and it is the RP's pinned head. With
/// N4-A having proved the prefix Live through `E`, the fold therefore selected that body, and
/// `prev_digest` linkage pins every earlier position to the RP's own chain.
fn head_pinned(bundle: &EvidenceBundle, view: TrustedAuthorityView) -> bool {
    let at_e: Vec<EventId> = bundle
        .summarize()
        .into_iter()
        .filter(|fact| fact.position == view.position)
        .map(|fact| fact.event_id)
        .collect();
    at_e == [view.head]
}

fn request_labels_ok(request: &AccessRequest<'_>) -> bool {
    label_ok(&request.resource.kind) && label_ok(&request.resource.id) && label_ok(&request.action)
}

/// The stable N4-A class name, matching `icnctl device-authority verify`'s vocabulary.
pub fn n4_class(err: &DeviceActVerifyError) -> &'static str {
    match err {
        DeviceActVerifyError::MalformedAct(_) => "MalformedAct",
        DeviceActVerifyError::ActSubjectMismatch => "ActSubjectMismatch",
        DeviceActVerifyError::EvaluationPositionMismatch { .. } => "EvaluationPositionMismatch",
        DeviceActVerifyError::BadSignature => "BadSignature",
        DeviceActVerifyError::Refused(refusal) => match refusal {
            DeviceAuthorityRefusal::PositionOutOfRange(_) => "PositionOutOfRange",
            DeviceAuthorityRefusal::SubjectUnknown => "SubjectUnknown",
            DeviceAuthorityRefusal::AuthorityHalted { .. } => "AuthorityHalted",
            DeviceAuthorityRefusal::PrefixIncomplete { .. } => "PrefixIncomplete",
            DeviceAuthorityRefusal::DeviceIsEstablishmentAuthority => {
                "DeviceIsEstablishmentAuthority"
            }
            DeviceAuthorityRefusal::DeviceNotAuthorized => "DeviceNotAuthorized",
            DeviceAuthorityRefusal::GrantNotInForce { .. } => "GrantNotInForce",
            DeviceAuthorityRefusal::CapabilityNotGranted(_) => "CapabilityNotGranted",
        },
    }
}
