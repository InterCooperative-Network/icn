---
Status: descriptive
Canonical: no
Last Reviewed: 2026-07-21
---

# The Authority Spine

How ICN makes the powers of an assembled runtime provable: where authority came
from, what bounds it, how it is withdrawn, and what the runtime will honestly
say about itself.

> **Truth status.** This note describes a pattern **implemented for gateway
> session authority only** (issues #2436, #2437). Every other domain named in
> §4 is *analysis*: the extension is proposed, not built. Nothing here asserts
> production operation, pilot readiness, or institutional adoption.

## 1. The problem this answers

A recurring failure shape across the codebase: a capability is implemented in a
crate, unit-tested against a miniature composition, and then **not installed**
in the assembled runtime — while documentation describes the library capability
as though it were a runtime guarantee. Verified instances at the time of writing
included RPC token revocation (machinery complete, constructed as `None` in the
daemon), the gateway session lifetime (`token_expiry_hours` parsed, tested, and
never applied), credit-policy enforcement on gateway-owned ledgers, ledger
inbound sync, invariant gates, and hybrid blob storage.

The common cause is not carelessness. It is that **composition is not a
first-class value**: optional capabilities are exposed as setter seams, the
composition root takes the minimal default at each seam, and nothing compares
"built and tested" against "installed here".

## 2. The invariants

Four properties, enforced together, are what turn a credential into accountable
authority rather than a bearer secret.

| Invariant | Statement | Where enforced |
|---|---|---|
| **Attenuation** | `issued ⊆ issuer ∩ flow_allowed ∩ requested` — every term a ceiling, never a grant | `session_authority::attenuate_scopes` |
| **Expiration** | The *configured* lifetime bounds the **gateway session** credentials actually issued and accepted; client responses report that same lifetime, without hidden verification leeway. The RPC surface is **not** bounded — see below | `TokenLifetimePolicy`, `AuthManager::with_token_ttl`, `SessionAuthority::verify` (acceptance bound), auth/invite/session responses |
| **Revocation** | An issued credential can be individually withdrawn and is revalidated before each protected operation on every surface that accepted it | `RevocationAuthority`, HTTP middleware, WebSocket operations, RPC verification |
| **Truth** | The runtime reports which of the above it actually installed, and a profile that *requires* a guarantee refuses to **serve** without it | `AuthorityCapabilities`, `AuthorityProfile::validate` |

Two design rules make these hold under failure:

- **Fail closed at the boundary, not per-caller.** Issuance handlers and
  `jwt_auth` resolve the same `SessionAuthority` from `app_data`; the runtime no
  longer registers a separate bare issuer. A route cannot opt out by forgetting
  to check, and a misassembly cannot silently restore signature-only
  verification.
- **Unreadable state is not authorization.** A revocation lookup that errors
  denies the request. "We could not determine whether this was revoked" must
  never resolve to "not revoked".

## 3. What a deployment profile means

A profile is a **requirement**, not a description. `PortableEvaluator` may run
volatile revocation because the deployment is disposable — and says so in its
capability report. `Institutional` requires durable revocation and **refuses to
assemble** without it, naming the capability, the reason, the refused fallback,
and the fix.

**Which deployments actually get which profile today**, since a profile
document that does not say this is exactly the kind of unbacked capability claim
this note argues against: the profile is *inferred from whether a revocation
store was supplied*, not declared by operator configuration. The supply is
conditional, not guaranteed — `revocation_store` is an `Option` on the gateway
handles (`supervisor/lifecycle.rs`) that `init_gateway` consumes with `if let
Some(..)`. A daemon startup that completes the component set opens
`<store_path>/auth-revocation` and supplies it, so **a fully started daemon —
including the portable evaluator appliance — runs `Institutional` with durable
revocation**.

The consequence to state plainly, because the inference is silent: `Institutional`
is not a property of *being* a daemon. Any path that does not reach that
assignment leaves the handle `None` and yields `PortableEvaluator` — embedded and
test callers that construct a `GatewayServer` without a store, and equally a
daemon run that never gets that far, such as one whose keystore does not unlock.
In that case a daemon *downgrades* its own authority guarantee without an
operator asking for it, and the only thing that says so is the capability report.
Making the profile an operator-declared configuration value — so that a
deployment which asked for `Institutional` fails instead of quietly becoming
disposable — is a follow-up.

Precisely what "refuses" means today: the gateway does not come up, and the
daemon logs the error and continues running without a gateway. The supervisor
waits for the gateway's initialization-and-bind acknowledgement and marks the
gateway actor active only after that acknowledgement; failed startup is reported
inactive. This is fail-closed — no request is ever served under an unmet
guarantee — but it is *not* a process abort. Making an unmet authority profile
fail the whole daemon is a deliberate follow-up decision. An institution that
cannot make a withdrawal survive a restart does not have revocation, and the
software should not claim otherwise on its behalf.

### Current lifetime and continuing-authorization semantics

- The canonical unconfigured session lifetime is 24 hours across
  `AuthManager`, `TokenLifetimePolicy`, embedded gateways, and daemon
  `GatewayConfig`. Explicit configuration replaces that value; authority
  assembly rejects a mismatch between the issuer and the reported policy.
- `/auth/verify`, invite join, and QR-session responses derive their reported
  lifetime from the installed authority. Verification uses the credential's
  exact `exp` boundary; the JWT library's default expiry leeway is disabled.
- The configured lifetime bounds **acceptance**, not just issuance. Every
  gateway mint already carries exactly the configured lifetime, but a co-issuer
  holding the signing secret — `icnctl auth token --local-mint` is the
  supported one — signs whatever expiry it chooses. `SessionAuthority::verify`
  therefore refuses a credential whose `exp - iat` exceeds the configured
  lifetime (loudly, naming the bound — not clamped, which would silently
  shorten a session the holder was told was longer), and independently refuses
  any credential whose expiry lies more than one configured lifetime from now,
  so a fabricated or forward-dated `iat` cannot buy extra validity. The local
  mint takes `--expiry-hours`, validated through the same
  `TokenLifetimePolicy` the gateway applies to its own configuration, for
  deployments configured shorter than the canonical default.
- **The acceptance bound covers the gateway surface only.** The daemon derives
  the RPC signing key from the same `gateway.jwt_secret`
  (`supervisor/init_rpc.rs`), and `RpcTokenClaims` does not reject unknown
  fields, so a gateway-issued credential is structurally verifiable on the RPC
  surface. `icn-rpc`'s `verify_token` checks signature, `exp`, and revocation —
  it applies no configured-lifetime bound, and its own issuance lifetime is a
  hardcoded 24 hours that never reads `token_expiry_hours`. Verified by direct
  test on this branch: on a deployment configured for one-hour sessions, a
  24-hour co-issued credential is refused by `SessionAuthority::verify` and
  **accepted** by `RpcAuthManager::verify_token`. Consequence an operator must
  know: lowering `token_expiry_hours` shortens gateway sessions immediately but
  does not shorten already-issued credentials on the RPC surface; revocation,
  which *is* shared, remains the instrument that reaches both. Closing this
  requires bounding RPC acceptance **and** issuance together — bounding
  acceptance alone would refuse the RPC manager's own freshly-minted tokens —
  which is a separate change with its own credential-invalidation migration,
  tracked in #2445.
- HTTP bearer routes revalidate on every request. WebSockets retain the
  credential, revalidate after asynchronous subscription setup, and revalidate
  before every protected event and every backfill operation/event. A revoked or
  expired socket is stopped before protected delivery. An idle socket may remain
  connected until its next protected operation; it retains no protected access
  during that idle period.
- Gateway and RPC revocation caches are positive-only. A miss consults the
  shared durable store, so a revocation written by either surface is visible to
  the other without restart. Store and cache errors deny verification.
- This means verification of a non-revoked credential performs one point read
  from the revocation store. No negative cache is installed: no demonstrated
  bottleneck currently justifies a stale-revocation window.

## 4. Extending the pattern (analysis — not implemented)

Authority is not only about people. A compute worker, a storage provider, a
model endpoint, and a federation peer each hold *powers* over cooperative
infrastructure, and each currently follows the same failure shape the session
work just corrected. The same lifecycle applies:

```text
advertised → discovered → attested → institutionally authorized
   → allocated → invoked → metered → receipt-producing
   → revocable → degraded → unavailable/withdrawn
```

The load-bearing distinction is between three things that are routinely
conflated:

1. **Technical capacity** — the machine can do it.
2. **Institutional legitimacy** — someone with standing authorized it.
3. **Current availability** — it works right now.

A capability report must never let (1) or (3) stand in for (2). Concretely:

- A machine may **advertise** compute without being **authorized** to execute
  protected workloads.
- A storage node may be **reachable** without being **authorized** to retain
  member evidence.
- A model may be **callable** without being **approved** for a given data class.
- A federation peer may **authenticate** a message without holding **authority**
  to alter institutional state. (This is a live gap: inbound institutional-state
  application is currently ambient — see #2441.)
- A ledger implementation may **exist** without the runtime **enforcing** the
  institution's credit policy — see #2438, the same "installed?" question in the
  economic domain.

For each such domain, the extension is the same three moves this work made:
give the subsystem one explicit composition value; make the profile's required
guarantees a startup invariant; and derive the capability report from the
constructed object rather than a hand-maintained list.

## 5. What this pattern does NOT decide

It enforces technical ceilings on authority. It does not decide who is
legitimate. Which DIDs may approve a session, which scopes a role carries, how
long an institution wants credentials to live, who may revoke whose authority,
and what recourse a member has — these are institutional questions that belong
to charter and governance policy above this layer, and several of them are
unanswered today:

| Question | Status |
|---|---|
| Who may approve a session on another member's behalf? | Open — currently any credential holder for the cooperative, attenuated to their own scopes |
| Who may revoke, and on whose authority? | Open — the revocation *mechanism* exists; the institutional act does not |
| What evidence should a revocation leave? | Open — no revocation receipt yet; belongs with the ADR-0026 ladder |
| How is authority recovered when an administrator disappears? | Open — operator succession is unaddressed |
| How does a member contest a revocation? | Open — challenge paths are weaker than institutional action paths |

Recording these as open is deliberate. A system whose authority cannot be
revoked, contested, or understood by its members does not meet ICN's purpose,
and naming the gap is more useful than a mechanism that implies it is closed.

## 6. References

- `icn/crates/icn-gateway/src/session_authority.rs` — the composition boundary
- `icn/crates/icn-gateway/tests/session_authority_enforcement.rs` — enforcement
  proof through the real middleware
- Issues #2436 (attenuation), #2437 (revocation + lifetime), #2421 (production
  route assembly not test-constructible — the remaining gap between "this
  boundary is tested" and "every mounted route is tested")
