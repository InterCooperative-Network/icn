//! Session authority — the gateway's authority composition boundary.
//!
//! # Why this module exists
//!
//! Gateway session authority was previously assembled implicitly: [`AuthManager`]
//! was constructed inline in `GatewayServer::run`, minting was reachable from
//! five call sites, verification consulted nothing but the JWT signature, and
//! the configured token lifetime never reached the issuer. Nothing in the
//! runtime could answer "which authority guarantees does this deployment
//! actually have?" — the library contained revocation machinery (in `icn-rpc`)
//! that the assembled daemon never installed (issues #2436, #2437).
//!
//! [`SessionAuthority`] is the single value through which the gateway issues and
//! verifies session credentials. It owns four responsibilities that were
//! previously scattered or absent:
//!
//! 1. **Attenuation** — no delegation may mint authority its issuer lacks
//!    ([`attenuate_scopes`]).
//! 2. **Lifetime** — the *configured* TTL bounds the credentials actually issued
//!    ([`TokenLifetimePolicy`]).
//! 3. **Revocation** — an issued credential can be individually withdrawn and is
//!    rejected thereafter ([`RevocationAuthority`]).
//! 4. **Truth** — what is installed is reportable, and a profile that requires a
//!    guarantee refuses to assemble without it ([`AuthorityCapabilities`],
//!    [`AuthorityProfile::validate`]).
//!
//! # What this module deliberately does NOT decide
//!
//! It enforces *technical* bounds on authority; it does not decide who is
//! institutionally legitimate. Which DIDs may approve a session, which scopes a
//! role carries, and how long a cooperative wants sessions to live are
//! institutional questions that belong to governance/charter policy above this
//! layer. This module guarantees only that whatever the institution decides
//! cannot be *exceeded* by the software: attenuation is a ceiling, never a
//! grant. See `docs/architecture/AUTHORITY_SPINE.md`.

use std::collections::{BTreeSet, HashSet};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use crate::auth::{AuthManager, TokenClaims};
use crate::error::{GatewayError, Result};

/// Key prefix for persisted revocation entries.
///
/// **Deliberately identical to `icn-rpc`'s `TokenRevocationList` prefix.** The
/// gateway and the RPC server are signed with the same secret and their claim
/// shapes are mutually decodable, so a credential presented to one surface can
/// be presented to the other. Namespacing the two revocation lists separately
/// would mean "revoked" on the gateway and "valid" on RPC for the same
/// credential — the exact divergence sharing a store is meant to prevent. The
/// key IS the revocation fact; the value is advisory metadata.
const REVOKED_JTI_PREFIX: &[u8] = b"auth:revoked:";

/// Upper bound on any configured session lifetime.
///
/// A credential that cannot be revoked for longer than this is an unbounded
/// liability; 30 days is the outer edge at which an operator should be issuing
/// a new credential rather than extending an old one.
pub const MAX_TOKEN_TTL: Duration = Duration::from_secs(30 * 24 * 3600);

/// Canonical session-credential lifetime when no explicit configuration is supplied.
///
/// The daemon's `GatewayConfig`, embedded gateways, and direct [`AuthManager`]
/// construction all use this value. Keeping one default prevents an embedded
/// router from silently issuing shorter-lived credentials than the daemon.
pub const DEFAULT_TOKEN_TTL: Duration = Duration::from_secs(24 * 3600);

// ---------------------------------------------------------------------------
// Scope attenuation
// ---------------------------------------------------------------------------

/// Compute the scopes a delegation may legitimately mint.
///
/// The attenuation rule, in one line:
///
/// ```text
/// issued ⊆ issuer ∩ flow_allowed ∩ (requested, when the caller asks for a subset)
/// ```
///
/// Every term is a *ceiling*. An issuer can only pass on authority it already
/// holds; a flow can only pass on authority its design permits; a caller may ask
/// for less but never for more. This is the invariant that makes #2436
/// structurally impossible rather than patched: there is no argument to this
/// function that yields a scope the issuer did not hold.
///
/// Returns [`GatewayError::AuthorizationFailed`] when the intersection is empty
/// — an issuer with nothing to delegate must be told so, not handed a valid
/// credential carrying no authority (which reads as success to a client and
/// fails confusingly later).
///
/// Duplicates and ordering are normalized; unknown scope strings are not
/// special-cased because an unknown scope cannot survive intersection with the
/// issuer's set unless the issuer already held it.
pub fn attenuate_scopes(
    issuer_scopes: &[String],
    flow_allowed: &[&str],
    requested: Option<&[String]>,
) -> Result<Vec<String>> {
    let issuer: BTreeSet<&str> = issuer_scopes.iter().map(String::as_str).collect();
    let allowed: BTreeSet<&str> = flow_allowed.iter().copied().collect();

    let mut granted: BTreeSet<&str> = issuer.intersection(&allowed).copied().collect();

    if let Some(req) = requested {
        // An explicit request narrows further. Asking for something outside the
        // ceiling is not an error the caller can exploit — it is simply dropped —
        // but a request that survives to an empty set is reported below.
        let req_set: BTreeSet<&str> = req.iter().map(String::as_str).collect();
        granted = granted.intersection(&req_set).copied().collect();
    }

    if granted.is_empty() {
        return Err(GatewayError::AuthorizationFailed(
            "no delegable authority: the approving credential holds none of the \
             scopes this flow may grant"
                .to_string(),
        ));
    }

    Ok(granted.into_iter().map(str::to_string).collect())
}

// ---------------------------------------------------------------------------
// Token lifetime
// ---------------------------------------------------------------------------

/// The lifetime actually applied to issued credentials.
///
/// Constructed from deployment configuration and *consumed* by the issuer, so a
/// configured value can never again be silently ignored (the prior state of
/// `token_expiry_hours`, issue #2437).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenLifetimePolicy {
    ttl: Duration,
}

impl TokenLifetimePolicy {
    /// Build a policy from a configured hour count.
    ///
    /// Rejects `0` (a credential expiring at issuance is a misconfiguration that
    /// would otherwise present as "auth is broken") and anything beyond
    /// [`MAX_TOKEN_TTL`]. Both are reported rather than clamped: silently
    /// shortening an operator's configured lifetime is the same class of bug as
    /// silently ignoring it.
    pub fn from_hours(hours: u64) -> Result<Self> {
        if hours == 0 {
            return Err(GatewayError::InternalError(
                "token_expiry_hours = 0 would issue already-expired credentials; \
                 set a positive lifetime"
                    .to_string(),
            ));
        }
        let ttl = Duration::from_secs(hours.saturating_mul(3600));
        if ttl > MAX_TOKEN_TTL {
            return Err(GatewayError::InternalError(format!(
                "token_expiry_hours = {hours} exceeds the {} hour maximum session lifetime",
                MAX_TOKEN_TTL.as_secs() / 3600
            )));
        }
        Ok(Self { ttl })
    }

    /// The applied lifetime.
    pub fn ttl(&self) -> Duration {
        self.ttl
    }
}

impl Default for TokenLifetimePolicy {
    fn default() -> Self {
        Self {
            ttl: DEFAULT_TOKEN_TTL,
        }
    }
}

// ---------------------------------------------------------------------------
// Revocation
// ---------------------------------------------------------------------------

/// Whether revocation state survives a restart.
///
/// This distinction is load-bearing: a volatile revocation authority silently
/// resurrects every revoked credential on restart, which is acceptable for a
/// disposable offline evaluator and unacceptable for an institution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevocationDurability {
    /// Survives process restart (persistent store).
    Durable,
    /// Lost on restart (process memory only).
    Volatile,
}

impl RevocationDurability {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Durable => "durable",
            Self::Volatile => "volatile",
        }
    }
}

/// Authority to withdraw an issued credential.
///
/// Implementations MUST return `Err` rather than `Ok(false)` when they cannot
/// determine revocation state. Callers treat an error as "assume revoked" — an
/// unreadable revocation store must never read as authorization.
pub trait RevocationAuthority: Send + Sync {
    /// Is this credential id revoked?
    fn is_revoked(&self, jti: &str) -> Result<bool>;

    /// Withdraw a credential until `expires_at` (unix seconds), after which the
    /// entry may be reclaimed because the credential is expired anyway.
    fn revoke(&self, jti: &str, expires_at: u64) -> Result<()>;

    /// Whether this authority's state survives restart.
    fn durability(&self) -> RevocationDurability;

    /// Implementation name, for the capability report.
    fn backend(&self) -> &'static str;
}

/// Durable revocation backed by a persistent [`icn_store::Store`].
///
/// # Read semantics: cache is a fast YES, the store is the authority
///
/// A cache hit answers "revoked" immediately. A cache *miss* falls through to
/// the store rather than answering "not revoked", because the cache only knows
/// about revocations this process performed or loaded at startup — a credential
/// revoked through the RPC `auth.revoke` surface (which shares this store) would
/// otherwise stay valid on the gateway until a restart. The one-sided fallthrough
/// keeps the dangerous answer authoritative while leaving the common path a
/// single hash lookup plus one point read.
///
/// Writes go to the store first and are cached only after the durable write
/// succeeds, so a cached "revoked" always implies a persisted "revoked" — never
/// the reverse.
pub struct StoreRevocationAuthority {
    store: Arc<dyn icn_store::Store>,
    cache: RwLock<HashSet<String>>,
}

impl StoreRevocationAuthority {
    /// Open a durable revocation authority, loading existing revocations.
    ///
    /// A store that cannot be scanned at startup is a hard error: booting with
    /// an unreadable revocation list would mean serving requests while unable to
    /// honor prior revocations.
    pub fn new(store: Arc<dyn icn_store::Store>) -> Result<Self> {
        let entries = store.scan(REVOKED_JTI_PREFIX).map_err(|e| {
            GatewayError::InternalError(format!("failed to load gateway revocation list: {e}"))
        })?;

        let cache = entries
            .into_iter()
            .filter_map(|(key, _)| {
                key.strip_prefix(REVOKED_JTI_PREFIX)
                    .and_then(|raw| std::str::from_utf8(raw).ok())
                    .map(str::to_string)
            })
            .collect::<HashSet<String>>();

        Ok(Self {
            store,
            cache: RwLock::new(cache),
        })
    }

    fn key(jti: &str) -> Vec<u8> {
        let mut k = REVOKED_JTI_PREFIX.to_vec();
        k.extend_from_slice(jti.as_bytes());
        k
    }
}

impl RevocationAuthority for StoreRevocationAuthority {
    fn is_revoked(&self, jti: &str) -> Result<bool> {
        {
            let cache = self.cache.read().map_err(|_| {
                GatewayError::InternalError("gateway revocation cache lock poisoned".to_string())
            })?;
            if cache.contains(jti) {
                return Ok(true);
            }
        }

        // Cache miss is not proof of validity: the RPC surface shares this store
        // and revokes without touching this process's cache. Consult the store,
        // and propagate read failures so the caller fails closed.
        let present = self.store.get(&Self::key(jti)).map_err(|e| {
            GatewayError::InternalError(format!("failed to read revocation state: {e}"))
        })?;

        if present.is_some() {
            if let Ok(mut cache) = self.cache.write() {
                cache.insert(jti.to_string());
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn revoke(&self, jti: &str, expires_at: u64) -> Result<()> {
        // Value shape mirrors `icn-rpc`'s `RevokedToken` so the RPC server's
        // startup cache load (which deserializes values and skips entries it
        // cannot parse) honors revocations written here. The KEY is the
        // authoritative revocation fact for this side; the value carries the
        // expiry that both sides use to reclaim entries after the credential
        // would have expired anyway.
        let value = serde_json::json!({
            "jti": jti,
            "subject": "",
            "revoked_at": icn_time::current_timestamp_secs(),
            "original_expiry": expires_at,
            "reason": "revoked via gateway session authority",
        });
        let encoded = serde_json::to_vec(&value).map_err(|e| {
            GatewayError::InternalError(format!("failed to encode revocation entry: {e}"))
        })?;
        self.store.put(&Self::key(jti), &encoded).map_err(|e| {
            GatewayError::InternalError(format!("failed to persist gateway revocation: {e}"))
        })?;
        let mut cache = self.cache.write().map_err(|_| {
            GatewayError::InternalError("gateway revocation cache lock poisoned".to_string())
        })?;
        cache.insert(jti.to_string());
        Ok(())
    }

    fn durability(&self) -> RevocationDurability {
        RevocationDurability::Durable
    }

    fn backend(&self) -> &'static str {
        "store"
    }
}

/// Volatile revocation for disposable deployments.
///
/// Honest about what it is: revocations are lost on restart. Permitted only by
/// profiles that explicitly accept that (see [`AuthorityProfile`]).
#[derive(Default)]
pub struct InMemoryRevocationAuthority {
    revoked: RwLock<HashSet<String>>,
}

impl InMemoryRevocationAuthority {
    pub fn new() -> Self {
        Self::default()
    }
}

impl RevocationAuthority for InMemoryRevocationAuthority {
    fn is_revoked(&self, jti: &str) -> Result<bool> {
        let revoked = self.revoked.read().map_err(|_| {
            GatewayError::InternalError("gateway revocation lock poisoned".to_string())
        })?;
        Ok(revoked.contains(jti))
    }

    fn revoke(&self, jti: &str, _expires_at: u64) -> Result<()> {
        let mut revoked = self.revoked.write().map_err(|_| {
            GatewayError::InternalError("gateway revocation lock poisoned".to_string())
        })?;
        revoked.insert(jti.to_string());
        Ok(())
    }

    fn durability(&self) -> RevocationDurability {
        RevocationDurability::Volatile
    }

    fn backend(&self) -> &'static str {
        "memory"
    }
}

// ---------------------------------------------------------------------------
// Deployment profile
// ---------------------------------------------------------------------------

/// What a deployment *requires* of its authority subsystem.
///
/// Profile names follow the appliance's existing vocabulary
/// (`deploy/appliance/evaluator` = the portable demo profile;
/// `deploy/appliance/lan` = the operator-controlled node).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityProfile {
    /// Disposable, offline, single-reviewer evaluator. Volatile revocation is
    /// permitted because the whole deployment is discarded after use — but it is
    /// reported as a degraded guarantee, never as revocation support.
    PortableEvaluator,
    /// Operator-controlled or institutional deployment. Durable revocation is
    /// required: an institution that cannot make a withdrawal stick across a
    /// restart does not have revocation.
    Institutional,
}

impl AuthorityProfile {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PortableEvaluator => "portable-evaluator",
            Self::Institutional => "institutional",
        }
    }

    /// Does this profile require revocation to survive restart?
    pub fn requires_durable_revocation(&self) -> bool {
        matches!(self, Self::Institutional)
    }

    /// Startup invariant: refuse to run with guarantees this profile requires
    /// but the assembled runtime does not provide.
    ///
    /// The error names the missing capability, why the profile requires it, the
    /// unsafe fallback that was refused, and what to supply — an operator should
    /// not have to read this source file to fix their deployment.
    pub fn validate(&self, installed: RevocationDurability) -> Result<()> {
        if self.requires_durable_revocation() && installed == RevocationDurability::Volatile {
            return Err(GatewayError::InternalError(format!(
                "SECURITY: deployment profile '{}' requires durable session revocation, but only \
                 volatile (in-memory) revocation is installed.\n\
                 Why: volatile revocation is lost on restart, so every credential revoked before a \
                 restart silently becomes valid again — an institution cannot withdraw authority.\n\
                 Refused fallback: starting with in-memory revocation and reporting it as \
                 revocation support.\n\
                 To fix: supply a persistent revocation store to the gateway \
                 (`GatewayServer::with_revocation_store`), or run the \
                 '{}' profile if this deployment really is disposable.",
                self.as_str(),
                Self::PortableEvaluator.as_str(),
            )));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Capability truth
// ---------------------------------------------------------------------------

/// How real a capability is in *this* runtime.
///
/// The distinction the campaign found missing everywhere: code existing in a
/// crate is not a runtime guarantee. Only [`CapabilityState::Enforced`] means a
/// request path cannot bypass it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState {
    /// Installed and on the enforcing request path.
    Enforced,
    /// Installed but with explicitly reduced guarantees (see the report's notes).
    Degraded,
    /// Not installed in this runtime.
    Absent,
}

/// Machine-checkable description of the authority subsystem this runtime
/// actually assembled.
///
/// Derived from the constructed [`SessionAuthority`] — not hand-maintained — so
/// it cannot drift from the composition the way a checked-in inventory would.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuthorityCapabilities {
    /// Deployment profile in force.
    pub profile: &'static str,
    /// Session credential issuance.
    pub session_issuance: CapabilityState,
    /// Delegation scope attenuation (issuer ceiling).
    pub scope_attenuation: CapabilityState,
    /// Individual credential revocation.
    pub revocation: CapabilityState,
    /// Whether revocation survives restart.
    pub revocation_durability: &'static str,
    /// Revocation backend implementation name.
    pub revocation_backend: &'static str,
    /// The lifetime actually applied to issued credentials.
    pub token_ttl_secs: u64,
    /// Human-readable qualifications on the states above.
    pub notes: Vec<String>,
}

// ---------------------------------------------------------------------------
// SessionAuthority
// ---------------------------------------------------------------------------

/// The gateway's authority composition boundary.
///
/// Production (`GatewayServer::run`) and integration tests construct this the
/// same way, so a test that exercises `SessionAuthority` is exercising the
/// object the daemon serves requests with.
pub struct SessionAuthority {
    auth: Arc<AuthManager>,
    revocation: Arc<dyn RevocationAuthority>,
    lifetime: TokenLifetimePolicy,
    profile: AuthorityProfile,
}

impl SessionAuthority {
    /// Assemble the session authority, enforcing the profile's startup
    /// invariants.
    ///
    /// Fails closed: a profile whose required guarantees are not installed does
    /// not start. This is the mechanism that stops an authority capability from
    /// silently disappearing from a deployment.
    pub fn new(
        auth: Arc<AuthManager>,
        revocation: Arc<dyn RevocationAuthority>,
        lifetime: TokenLifetimePolicy,
        profile: AuthorityProfile,
    ) -> Result<Self> {
        profile.validate(revocation.durability())?;
        if auth.token_ttl() != lifetime.ttl() {
            return Err(GatewayError::InternalError(format!(
                "session authority lifetime mismatch: issuer uses {} seconds but policy reports {} seconds",
                auth.token_ttl().as_secs(),
                lifetime.ttl().as_secs()
            )));
        }
        Ok(Self {
            auth,
            revocation,
            lifetime,
            profile,
        })
    }

    /// Assemble the disposable-evaluator posture: volatile revocation, default
    /// lifetime, [`AuthorityProfile::PortableEvaluator`].
    ///
    /// This is the honest minimum an authenticated surface needs in order to
    /// serve at all — it enforces attenuation and revocation-within-a-run while
    /// reporting that revocation does not survive restart. It exists so that
    /// callers which legitimately have no persistent store (the portable
    /// evaluator, and tests that build their own routers) get a real authority
    /// rather than a bypass. Deployments that must withdraw authority durably
    /// use [`SessionAuthority::new`] with a store-backed authority and the
    /// institutional profile.
    pub fn evaluator(auth: Arc<AuthManager>) -> Self {
        let lifetime = TokenLifetimePolicy {
            ttl: auth.token_ttl(),
        };
        Self {
            auth,
            revocation: Arc::new(InMemoryRevocationAuthority::new()),
            lifetime,
            profile: AuthorityProfile::PortableEvaluator,
        }
    }

    /// The underlying credential issuer/verifier.
    pub fn auth_manager(&self) -> &Arc<AuthManager> {
        &self.auth
    }

    /// Applied session lifetime.
    pub fn lifetime(&self) -> TokenLifetimePolicy {
        self.lifetime
    }

    /// Deployment profile in force.
    pub fn profile(&self) -> AuthorityProfile {
        self.profile
    }

    /// Issue a credential **delegated from an existing one**, attenuated to the
    /// issuer's own authority.
    ///
    /// This is the path a session-approval flow must use. `flow_allowed` is the
    /// ceiling the flow itself may ever grant; `requested` lets a caller ask for
    /// less. Neither can raise the issuer's authority.
    pub fn issue_delegated(
        &self,
        issuer: &TokenClaims,
        subject: &icn_identity::Did,
        coop_id: &str,
        flow_allowed: &[&str],
        requested: Option<&[String]>,
    ) -> Result<(String, Vec<String>)> {
        let granted = attenuate_scopes(&issuer.scopes, flow_allowed, requested)?;
        let token = self.auth.issue_token(subject, coop_id, granted.clone())?;
        Ok((token, granted))
    }

    /// Verify a presented credential: signature and expiry, then the configured
    /// lifetime bound, then revocation.
    ///
    /// Ordering is deliberate — revocation state is only consulted for a
    /// credential that is otherwise valid, so an attacker cannot use this path to
    /// probe which identifiers exist.
    ///
    /// Fails closed on revocation-store errors: an authority that cannot read its
    /// revocation state denies rather than admits.
    pub fn verify(&self, token: &str) -> Result<TokenClaims> {
        let claims = self.auth.verify_token(token)?;

        // The configured lifetime bounds ACCEPTANCE, not just issuance. Every
        // gateway mint already carries exactly the configured lifetime, but a
        // co-issuer holding the signing secret — `icnctl auth token
        // --local-mint` is the supported one — signs whatever expiry it
        // chooses, and a signature check alone would honor it. Two checks,
        // guarding different failures:
        //
        // 1. `exp - iat` beyond the bound: an issuer that never learned this
        //    deployment's configured lifetime. Refused loudly and immediately
        //    (not clamped, not accepted-later), so the operator re-mints
        //    within policy instead of discovering a silently shortened
        //    session.
        // 2. `exp` further than one lifetime from now: independent of what
        //    `iat` claims, no accepted credential can have more than one
        //    configured lifetime of validity remaining. This holds even for a
        //    fabricated or forward-dated `iat`, which check 1 alone would
        //    trust.
        //
        // No leeway on either check, matching the zero-leeway expiry stance:
        // the trusted-local co-issuer runs on this same host and clock.
        let bound = self.lifetime.ttl().as_secs();
        if claims.exp.saturating_sub(claims.iat) > bound {
            return Err(GatewayError::AuthenticationFailed(format!(
                "credential lifetime exceeds this deployment's configured session \
                 lifetime ({bound} seconds); re-mint within the configured bound"
            )));
        }
        let now = icn_time::current_timestamp_secs();
        if claims.exp > now.saturating_add(bound) {
            return Err(GatewayError::AuthenticationFailed(
                "credential expiry lies further away than the configured session \
                 lifetime permits"
                    .to_string(),
            ));
        }

        let Some(jti) = claims.jti.as_deref() else {
            // A credential minted before revocable ids existed cannot be
            // individually withdrawn. Institutions must not accept an
            // unrevocable credential; disposable evaluators may (they have no
            // durable revocation to contradict anyway).
            //
            // MIGRATION (breaking): because the daemon declares the
            // institutional profile whenever it has a revocation store, every
            // credential outstanding at upgrade time is rejected *immediately*,
            // not gradually — holders must re-authenticate. The appliance
            // re-mints during seeding; browser sessions held elsewhere die at
            // upgrade.
            if self.profile.requires_durable_revocation() {
                return Err(GatewayError::AuthenticationFailed(
                    "credential predates revocable session ids and cannot be withdrawn; \
                     re-authenticate to obtain a current credential"
                        .to_string(),
                ));
            }
            return Ok(claims);
        };

        if self.revocation.is_revoked(jti).map_err(|e| {
            // Do not leak store internals to the client; the operator gets the
            // detail in logs.
            tracing::error!(error = %e, "revocation lookup failed; denying request (fail-closed)");
            GatewayError::AuthenticationFailed("credential could not be validated".to_string())
        })? {
            return Err(GatewayError::AuthenticationFailed(
                "credential has been revoked".to_string(),
            ));
        }

        Ok(claims)
    }

    /// Withdraw a credential by its claims.
    pub fn revoke(&self, claims: &TokenClaims) -> Result<()> {
        let jti = claims.jti.as_deref().ok_or_else(|| {
            GatewayError::BadRequest(
                "credential carries no revocable id; it cannot be individually withdrawn"
                    .to_string(),
            )
        })?;
        self.revocation.revoke(jti, claims.exp)
    }

    /// Report what this runtime actually assembled.
    pub fn capabilities(&self) -> AuthorityCapabilities {
        let durability = self.revocation.durability();
        let mut notes = Vec::new();

        let revocation = match durability {
            RevocationDurability::Durable => CapabilityState::Enforced,
            RevocationDurability::Volatile => {
                notes.push(
                    "revocation is in-memory: credentials revoked before a restart become valid \
                     again after it"
                        .to_string(),
                );
                CapabilityState::Degraded
            }
        };

        if !self.profile.requires_durable_revocation() {
            notes.push(format!(
                "profile '{}' does not require durable revocation; this deployment is not an \
                 institutional authority",
                self.profile.as_str()
            ));
        }

        // Attenuation is DEGRADED, not enforced, and saying so is the whole
        // point of this report. `issue_delegated` is the only attenuating mint
        // path; the invite, SDIS-enrollment and trusted-local (`icnctl
        // --local-mint`) paths still issue fixed scope sets chosen by the flow
        // rather than bounded by the issuer's own authority. Reporting
        // `Enforced` here would be precisely the failure this subsystem exists
        // to make impossible: a capability claim the assembled runtime does not
        // back on every path.
        notes.push(
            "scope attenuation is enforced only for delegated session approval; the invite, \
             SDIS-enrollment and trusted-local mint paths still issue flow-chosen scope sets"
                .to_string(),
        );

        AuthorityCapabilities {
            profile: self.profile.as_str(),
            session_issuance: CapabilityState::Enforced,
            scope_attenuation: CapabilityState::Degraded,
            revocation,
            revocation_durability: durability.as_str(),
            revocation_backend: self.revocation.backend(),
            token_ttl_secs: self.lifetime.ttl().as_secs(),
            notes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scopes(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    // -- attenuation -------------------------------------------------------

    #[test]
    fn attenuation_never_exceeds_issuer_authority() {
        // The #2436 defect, expressed as an invariant: a read-only issuer cannot
        // produce write authority no matter what the flow permits.
        let issuer = scopes(&["governance:read"]);
        let flow = ["coop:write", "ledger:transact", "governance:write"];
        let err = attenuate_scopes(&issuer, &flow, None).unwrap_err();
        assert!(matches!(err, GatewayError::AuthorizationFailed(_)));
    }

    #[test]
    fn attenuation_yields_the_intersection() {
        let issuer = scopes(&["governance:read", "governance:write", "ledger:read"]);
        let flow = ["governance:read", "governance:write", "coop:write"];
        let granted = attenuate_scopes(&issuer, &flow, None).unwrap();
        assert_eq!(granted, scopes(&["governance:read", "governance:write"]));
    }

    #[test]
    fn requested_scopes_can_only_narrow() {
        let issuer = scopes(&["governance:read", "governance:write"]);
        let flow = ["governance:read", "governance:write"];
        // Asking for more than the ceiling does not raise it.
        let granted = attenuate_scopes(
            &issuer,
            &flow,
            Some(&scopes(&[
                "governance:read",
                "ledger:transact",
                "coop:write",
            ])),
        )
        .unwrap();
        assert_eq!(granted, scopes(&["governance:read"]));
    }

    #[test]
    fn duplicate_and_unordered_scopes_normalize() {
        let issuer = scopes(&["b:write", "a:read", "b:write", "a:read"]);
        let flow = ["a:read", "b:write", "a:read"];
        let granted = attenuate_scopes(&issuer, &flow, None).unwrap();
        assert_eq!(granted, scopes(&["a:read", "b:write"]));
    }

    #[test]
    fn empty_issuer_or_flow_grants_nothing() {
        assert!(attenuate_scopes(&[], &["a:read"], None).is_err());
        assert!(attenuate_scopes(&scopes(&["a:read"]), &[], None).is_err());
        assert!(attenuate_scopes(&scopes(&["a:read"]), &["a:read"], Some(&[])).is_err());
    }

    #[test]
    fn unknown_scopes_cannot_be_conjured() {
        // An unknown/garbage scope only survives if the issuer already held it,
        // in which case it is the issuer's own authority, not an escalation.
        let issuer = scopes(&["weird::scope\u{0}"]);
        let flow = ["governance:write"];
        assert!(attenuate_scopes(&issuer, &flow, None).is_err());
    }

    /// Exhaustive attenuation invariant over the real gateway scope vocabulary:
    /// for every issuer subset and every requested subset, the granted set is
    /// always a subset of the issuer's set. This is the property that closes
    /// #2436 for all inputs, not just the reported one.
    #[test]
    fn granted_is_always_a_subset_of_issuer_authority() {
        let vocab = [
            "coop:read",
            "coop:write",
            "ledger:read",
            "ledger:transact",
            "governance:read",
            "governance:write",
        ];
        let flow: Vec<&str> = vocab.to_vec();

        for issuer_mask in 0u32..(1 << vocab.len()) {
            let issuer: Vec<String> = vocab
                .iter()
                .enumerate()
                .filter(|(i, _)| issuer_mask & (1 << i) != 0)
                .map(|(_, s)| s.to_string())
                .collect();

            for req_mask in 0u32..(1 << vocab.len()) {
                let requested: Vec<String> = vocab
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| req_mask & (1 << i) != 0)
                    .map(|(_, s)| s.to_string())
                    .collect();

                if let Ok(granted) = attenuate_scopes(&issuer, &flow, Some(&requested)) {
                    for scope in &granted {
                        assert!(
                            issuer.contains(scope),
                            "escalation: granted {scope:?} not held by issuer {issuer:?}"
                        );
                        assert!(
                            requested.contains(scope),
                            "granted {scope:?} beyond request {requested:?}"
                        );
                    }
                }
            }
        }
    }

    // -- lifetime ----------------------------------------------------------

    #[test]
    fn configured_lifetime_is_applied() {
        assert_eq!(
            TokenLifetimePolicy::from_hours(24).unwrap().ttl(),
            Duration::from_secs(86_400)
        );
    }

    #[test]
    fn unconfigured_issuer_and_policy_share_the_canonical_default() {
        let auth = AuthManager::new(vec![7u8; 32]);
        assert_eq!(auth.token_ttl(), DEFAULT_TOKEN_TTL);
        assert_eq!(TokenLifetimePolicy::default().ttl(), DEFAULT_TOKEN_TTL);
    }

    #[test]
    fn zero_and_excessive_lifetimes_are_rejected() {
        assert!(TokenLifetimePolicy::from_hours(0).is_err());
        assert!(TokenLifetimePolicy::from_hours(24 * 31).is_err());
        assert!(TokenLifetimePolicy::from_hours(24 * 30).is_ok());
    }

    // -- profile / startup -------------------------------------------------

    #[test]
    fn institutional_profile_refuses_volatile_revocation() {
        let err = AuthorityProfile::Institutional
            .validate(RevocationDurability::Volatile)
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("durable session revocation"), "{msg}");
        assert!(msg.contains("To fix"), "{msg}");
    }

    #[test]
    fn portable_evaluator_may_run_volatile() {
        assert!(AuthorityProfile::PortableEvaluator
            .validate(RevocationDurability::Volatile)
            .is_ok());
        assert!(AuthorityProfile::Institutional
            .validate(RevocationDurability::Durable)
            .is_ok());
    }

    #[test]
    fn assembly_fails_closed_when_profile_requirements_are_unmet() {
        let auth = Arc::new(AuthManager::new(vec![7u8; 32]));
        let result = SessionAuthority::new(
            auth,
            Arc::new(InMemoryRevocationAuthority::new()),
            TokenLifetimePolicy::default(),
            AuthorityProfile::Institutional,
        );
        assert!(
            result.is_err(),
            "institutional profile must not assemble with volatile revocation"
        );
    }

    #[test]
    fn assembly_rejects_an_issuer_policy_lifetime_mismatch() {
        let auth = Arc::new(AuthManager::new(vec![7u8; 32]));
        let result = SessionAuthority::new(
            auth,
            Arc::new(InMemoryRevocationAuthority::new()),
            TokenLifetimePolicy::from_hours(2).unwrap(),
            AuthorityProfile::PortableEvaluator,
        );
        let error = match result {
            Ok(_) => panic!("mismatched lifetime truth must fail assembly"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("lifetime mismatch"));
    }

    // -- revocation --------------------------------------------------------

    #[test]
    fn in_memory_revocation_round_trips() {
        let rev = InMemoryRevocationAuthority::new();
        assert!(!rev.is_revoked("abc").unwrap());
        rev.revoke("abc", 0).unwrap();
        assert!(rev.is_revoked("abc").unwrap());
        assert!(!rev.is_revoked("other").unwrap());
        assert_eq!(rev.durability(), RevocationDurability::Volatile);
    }

    #[test]
    fn capability_report_reflects_actual_assembly() {
        let lifetime = TokenLifetimePolicy::from_hours(2).unwrap();
        let auth = Arc::new(AuthManager::new(vec![7u8; 32]).with_token_ttl(lifetime.ttl()));
        let authority = SessionAuthority::new(
            auth,
            Arc::new(InMemoryRevocationAuthority::new()),
            lifetime,
            AuthorityProfile::PortableEvaluator,
        )
        .unwrap();

        let caps = authority.capabilities();
        assert_eq!(caps.profile, "portable-evaluator");
        assert_eq!(caps.revocation, CapabilityState::Degraded);
        assert_eq!(caps.revocation_durability, "volatile");
        assert_eq!(caps.revocation_backend, "memory");
        assert_eq!(caps.token_ttl_secs, 7200);
        // Attenuation is Degraded, not Enforced: only delegated session approval
        // attenuates today. Pinning the honest value here so a future change that
        // upgrades the claim has to also upgrade the reality.
        assert_eq!(caps.scope_attenuation, CapabilityState::Degraded);
        assert!(
            caps.notes
                .iter()
                .any(|n| n.contains("enforced only for delegated session approval")),
            "partial attenuation must be reported honestly: {:?}",
            caps.notes
        );
        assert!(
            caps.notes.iter().any(|n| n.contains("valid again after")),
            "degraded revocation must be reported honestly: {:?}",
            caps.notes
        );
    }

    // -- lifetime acceptance bound (#2442 review: trusted-local co-issuer) --

    fn hour_authority() -> SessionAuthority {
        let lifetime = TokenLifetimePolicy::from_hours(1).unwrap();
        let auth = Arc::new(AuthManager::new(vec![7u8; 32]).with_token_ttl(lifetime.ttl()));
        SessionAuthority::new(
            auth,
            Arc::new(InMemoryRevocationAuthority::new()),
            lifetime,
            AuthorityProfile::PortableEvaluator,
        )
        .unwrap()
    }

    fn test_did() -> icn_identity::Did {
        icn_identity::IdentityBundle::generate()
            .expect("generate test identity")
            .did()
            .clone()
    }

    /// A co-issuer holding the signing secret but not the deployment's
    /// configuration (the `icnctl auth token --local-mint` shape) mints with
    /// the canonical 24-hour default. A one-hour deployment must refuse that
    /// credential at acceptance: the configured lifetime is an authorization
    /// bound on what is ACCEPTED, not advice applied only at issuance.
    #[test]
    fn acceptance_rejects_a_lifetime_beyond_the_configured_bound() {
        let authority = hour_authority();
        let co_issuer = AuthManager::new(vec![7u8; 32]); // canonical 24h default
        let token = co_issuer
            .issue_token(&test_did(), "test-coop", scopes(&["coop:read"]))
            .unwrap();
        let err = authority.verify(&token).unwrap_err();
        assert!(matches!(err, GatewayError::AuthenticationFailed(_)));
    }

    /// The bound is exact, not fuzzy: a credential carrying precisely the
    /// configured lifetime — what every gateway mint produces — is accepted.
    #[test]
    fn acceptance_admits_exactly_the_configured_lifetime() {
        let authority = hour_authority();
        let token = authority
            .auth_manager()
            .issue_token(&test_did(), "test-coop", scopes(&["coop:read"]))
            .unwrap();
        assert!(authority.verify(&token).is_ok());
    }

    /// A forward-dated `iat` cannot smuggle extra validity: even when
    /// `exp - iat` respects the configured lifetime, a credential whose expiry
    /// lies further than one configured lifetime from now is refused.
    #[test]
    fn forward_dated_credentials_cannot_outlive_the_configured_bound() {
        let authority = hour_authority();
        let now = icn_time::current_timestamp_secs();
        let claims = crate::auth::TokenClaims {
            sub: test_did().to_string(),
            iat: now + 7200,
            exp: now + 7200 + 3600, // exp - iat == configured ttl, but 3h out
            coop_id: "test-coop".to_string(),
            scopes: scopes(&["coop:read"]),
            entity_id: None,
            entity_type: None,
            jti: Some("forward-dated-test".to_string()),
        };
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(&[7u8; 32]),
        )
        .unwrap();
        let err = authority.verify(&token).unwrap_err();
        assert!(matches!(err, GatewayError::AuthenticationFailed(_)));
    }
}
