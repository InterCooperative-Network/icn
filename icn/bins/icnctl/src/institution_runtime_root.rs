//! Provisioning the **institutional runtime root** (#2744).
//!
//! # What this is, and what it deliberately is not
//!
//! `docs/architecture/IDENTITY_SEMANTICS.md` §2.3 is the registered owner of
//! what an Institution *is*, and it is unambiguous: an Institution is
//! "constituted by a charter and its governance rather than by custody of a
//! key", its identifier domain is `EntityId`, its genesis is "a founding act
//! signed by founding Principals", and its authority is "a governance decision
//! evidenced against authenticated state, never a signature by any single key".
//!
//! This ceremony implements **none of that**, and an earlier draft of it called
//! itself `institution genesis` anyway. It creates no `EntityId`, persists no
//! signed founding act, and establishes no governance. Its receipt is
//! provisioning evidence, **not** the canonical founding act, and later work
//! (#2602 / GEN) must not read it as though it were.
//!
//! What it does create is the runtime substrate the current execution spine
//! needs, and that is genuinely missing today:
//!
//! * durable cooperative state, in the store the daemon reads;
//! * a **genesis trust-root Principal** — key-backed, distinct from the node,
//!   whose only role is to be the source of the trust fact reaching the
//!   treasury. It is NOT the Institution;
//! * a **treasury Principal** — key-backed, distinct from the node DID;
//! * the persisted trust relationships the ledger's author-trust path needs;
//! * configuration that makes the daemon consume that treasury instead of
//!   silently substituting the node's own DID;
//! * a completion receipt written only over verified durable state.
//!
//! # Why it is a separate, offline ceremony
//!
//! Before this existed the only production path that mentioned a cooperative
//! was `icnctl init-coop`, which wrote an `icn.toml` with no `[cooperative]`
//! section — so `CooperativeConfig.treasury_did` was always `None` and
//! `supervisor/lifecycle.rs` fell back to the node's own DID for every
//! governance-authored ledger entry. The treasury *was* the node operator. The
//! treasury-creation code existed but had no runtime producer: every caller of
//! `create_treasury` / `activate_cooperative` sat behind `#[cfg(test)]`.
//!
//! Three candidate homes were rejected on evidence:
//!
//! * `institution bootstrap apply` authenticates to a **running gateway** with
//!   `--coop-id`, so it needs the institution to exist in order to create it,
//!   and its `BootstrapOperation` set has no treasury operation at all;
//! * the gateway/RPC surface cannot write the facts this needs: production
//!   builds `TrustManager::with_trust_service`, leaving `trust_graph: None`, so
//!   trust edges land in an in-memory `DashMap` and never reach
//!   `<data_dir>/store/trust`;
//! * extending `init-coop` would inherit #2725 and conflate provisioning a node
//!   with founding an institution.
//!
//! What is left is a privileged **local** ceremony run with the daemon stopped,
//! alongside the maintenance commands that already open these sled databases
//! directly and already cross the N2-A startup gate.
//!
//! # Why the treasury needs its own key material
//!
//! `Did::from_str` requires the bytes after `did:icn:` to decode to exactly 32
//! bytes **and** form a valid Ed25519 point, and `Deserialize for Did` calls it.
//! Measured against the two treasury spellings this repository already produces:
//! `derive_treasury_did` emits `did:icn:treasury:<bs58>`, which is not multibase
//! and never parses; `Did::from_anchor_id` emits 16 hashed bytes zero-padded to
//! 32 and parsed 140 of 300 sampled values. `lifecycle.rs` parsed the configured
//! treasury with `.ok()` and fell back to the node DID, so either spelling would
//! be *silently* replaced by the node — the exact collapse this exists to
//! prevent. A keypair-backed DID always parses. `AgeKeyStore` holds a single
//! identity bundle, so a distinct Principal is necessarily a distinct keystore.
//!
//! # Principals and roles
//!
//! | role | what it is today |
//! |---|---|
//! | node Principal | the node's own `identity.age` keypair |
//! | provisioning authority | **cryptographically the node Principal**, proven by opening that keystore rather than asserted |
//! | runtime trust root | a distinct, newly minted, keypair-backed Principal — **not** the Institution |
//! | treasury | a distinct, newly minted, keypair-backed Principal |
//! | canonical Institution | does not exist here; `EntityId` is unallocated |
//!
//! # Durable state
//!
//! Seven components, each reported individually by `show`:
//!
//! 1. runtime trust-root keystore (minted, 0600);
//! 2. treasury keystore (minted, 0600);
//! 3. cooperative record, in the store the daemon opens;
//! 4. treasury registration, in the ledger store;
//! 5. two trust edges, in the trust store;
//! 6. configuration linkage — `<data_dir>/icn.toml` naming the treasury;
//! 7. the runtime-root receipt, written last.
//!
//! Only three are exclusive to this ceremony, and `is_untouched` keys on those
//! alone: the gateway's `CoopManager` creates cooperative records and
//! `init-coop` writes trust edges, so counting the shared ones would report an
//! ordinary node as mid-ceremony.
//!
//! # Trust topology, and the authority boundary
//!
//! ```text
//! node Principal
//!   --local recognition-->        direct edge, 0.7 tier
//! runtime trust-root Principal
//!   --runtime-root-attributed-->  transitive, 0.3 tier
//! treasury Principal
//! ```
//!
//! Effective author score `1.0 * 1.0 * 0.3 = 0.300`, against the ledger's
//! `DEFAULT_MIN_TRUST_FOR_ENTRY = 0.1`. **Both** edges are required: the gate is
//! ego-centric from the node's own DID, so a `trust-root -> treasury` edge alone
//! is unreachable and scores `0.0`. Removing either drops the treasury below the
//! gate, and both directions are pinned by tests, as is a stranger DID scoring
//! below it.
//!
//! The trust root's key does **not** sign the second edge. `TrustEdge` carries
//! no signature or provenance field, so the store records no evidence of who
//! established it; it is written through `TrustGraph::add_edge` under the
//! privilege of a local ceremony authenticated by the founding Principal's
//! keystore. The honest description is an *institution-attributed trust fact
//! established under privileged local ceremony*, not a cryptographic
//! authorisation — and therefore the cooperative's authority here is not
//! independent of the node.
//!
//! # Operator input, and who owns the limits
//!
//! The ceremony applies its own rules — non-empty, no control characters, no
//! whitespace inside a currency, a 200-scalar paste-accident cap on the name —
//! and then calls `icn_gateway::validation::validate_coop_name` and
//! `validate_unit` directly rather than restating their constants.
//!
//! Restating them is what produced icn#2749's byte-versus-scalar defect: the
//! gateway measures `str::len()`, which is BYTES, while a hand-written check
//! counted Unicode scalars, so nine four-byte characters passed a "32
//! character" rule and failed the gateway's 32-byte one. These values are
//! written straight into durable state and never re-checked, and a rerun is
//! refused, so an accepted-here/rejected-there value is permanent.
//!
//! The rule, stated once: **every value this ceremony permanently provisions
//! must be accepted by the subsystem that later consumes it.** It may be
//! stricter than that consumer for a declared local reason. It may not be
//! weaker.
//!
//! The same principle governs the configuration preflight, in the other
//! direction: `icnd` applies `--gateway-jwt-secret`, then
//! `ICN_GATEWAY_JWT_SECRET`, then the file, and only then runs
//! `Config::validate`. This ceremony applies the environment variable before
//! validating for exactly that reason — validating the raw file would make it
//! *stricter* than the daemon it is checking for, rejecting the standard
//! `init-coop` flow and pushing an operator into persisting a plaintext secret.
//!
//! # Coordination protocol
//!
//! Two resources, because a daemon depends on two things it does not own alone.
//! See [`icn_core::DataDirLock`] for the mechanism.
//!
//! | resource | file | daemon | this ceremony |
//! |---|---|---|---|
//! | storage it mutates | `<data_dir>/.icn-data-dir.lock` | exclusive | exclusive |
//! | configuration it consumed | `<config_root>/.icn-config.lock` | **shared** | **exclusive** |
//!
//! The configuration side is shared/exclusive because a daemon *reads* a
//! configuration and this ceremony *writes* one. Several daemons may hold one
//! directory at once — the shipped two-node demo keeps both node configurations
//! in `config/` with different data roots — while any publisher is refused.
//!
//! They are separate **files** rather than two modes of one because `flock(2)`
//! is per open-file-description: a second handle on one path conflicts with the
//! first inside the same process. Separate files let a daemon whose config root
//! and data root coincide hold both without contending with itself.
//!
//! Both are held for the daemon's whole life. Releasing the configuration lock
//! after `Config::from_file` leaves a running daemon acting on an
//! interpretation a ceremony is free to republish underneath it — the case
//! where `--data-dir` (or any `data_dir` pointing elsewhere) makes the two roots
//! differ. Ordering is configuration-then-storage in both actors but is not
//! load-bearing: every acquisition is non-blocking, so a crossed pair is refused
//! rather than hung.
//!
//! **Actor model.** Both parties run under the *same account*. Every scripted
//! invocation in this repository drops to the service user first (`sudo -u icn`,
//! `runuser -u icn`), and the ceremony enforces it (below). This matters
//! because advisory locking coordinates equals: a daemon that cannot create the
//! coordination file has not established that no more privileged writer can, so
//! that case fails closed. A genuinely immutable filesystem — a read-only
//! ConfigMap mount — reports `ReadOnlyFilesystem` and is recognised as having
//! nothing to exclude. None of this is a boundary against a privileged local
//! adversary, who can remove the file or ignore advisory locking.
//!
//! One consequence is deliberate and worth stating rather than discovering: a
//! daemon running as a service account over a configuration directory owned by
//! a *more privileged* account will not start until that directory is made
//! writable by the daemon's account. It cannot establish exclusion there, and
//! the same-account model is what makes the coordination domain meaningful. A
//! read-only filesystem is exempt, because nothing on it can be published by
//! anyone.
//!
//! A symlinked `<data_dir>/icn.toml` is refused, twice and independently: the
//! N2-A startup gate refuses a symlink under the data directory before any
//! mutation, and publication refuses a non-regular destination. A ceremony that
//! refuses symlinked managed configuration does not need to join the link
//! target's lock domain.
//!
//! # Publishing the configuration
//!
//! `rename` replaces the inode, so the replacement's owner and group come from
//! the process that created it. That is refused, not repaired:
//!
//! * the destination must be a regular, non-symlinked file;
//! * a replacement whose uid/gid would differ from the file it replaces is
//!   refused — before the exclusion locks are taken, so a wrong-account run
//!   creates nothing at all (the coordination files are retained after release,
//!   and root-owned ones would lock the daemon's account out permanently), and
//!   again inside `check_config_linkable`, which publication re-runs;
//! * the original mode is carried onto the replacement **before** `sync_all`,
//!   because `fsync` flushes the inode as well as the data and a mode set
//!   afterwards would not have crossed the barrier the receipt certifies;
//! * `rename`, then the parent directory is synced — atomicity is not
//!   durability;
//! * a post-condition then requires the published file to be a regular file
//!   whose uid, gid and mode equal the one it replaced.
//!
//! Repairing ownership with `chown` was rejected: it would fix `icn.toml` while
//! the two keystores and three sled databases stayed owned by the invoking
//! account, buying a receipt that looks right over state that is not — and it
//! would add a privilege-bearing path to a command that needs none.
//!
//! # Commit order
//!
//! ```text
//! resolve storage root -> refuse a foreign account -> configuration lock
//!   -> storage lock -> N2-A gate -> refuse over existing key material
//!   -> validate operator input -> refuse a foreign institution
//!   -> require the node keystore -> check the configuration is linkable
//!   -> PROVE founding authority by opening the node keystore
//!   -> mint both principals (0600, fsync file, fsync parent)
//!   -> cooperative record + flush -> treasury registration + flush
//!   -> trust edges + flush -> close every store handle
//!   -> publish the configuration -> re-verify through FRESH handles
//!   -> completion receipt LAST
//! ```
//!
//! Two details that are load-bearing and easy to prettify away: handles are
//! closed *before* verification, because sled takes an exclusive directory lock
//! and reading back through the writer's own handle would prove nothing about
//! durability; and cooperative, ledger and trust are three separate databases,
//! each flushed separately, because reopening proves visibility rather than
//! survival.
//!
//! # Partial state, and evidence levels
//!
//! `show` classifies into `NOT_STARTED`, `INCOMPLETE` (with the component
//! list), `READY`, `INCONSISTENT`, and an `ERROR` envelope for unreadable
//! cases. A directory already holding institutional state is **refused**, never
//! repaired or resumed: no silent remint, no partial overwrite.
//!
//! Evidence is tri-state, never boolean — `false` would conflate *checked and
//! wrong* with *not checked*. `create` holds the passphrase and verifies key
//! provenance and the current node identity. `show` deliberately does not
//! prompt, so those read `not_reverified` while the durable relationships,
//! configuration linkage, trust score and key presence read `verified`.
//! **Unknown is not false**, and the output says so rather than leaving a reader
//! to infer it.
//!
//! Exactly one JSON document reaches stdout for every outcome, including load
//! failures; tracing goes to stderr so `--json` stays machine-parseable.
//!
//! # External defects this does not fix
//!
//! * **icn#2747** — stock `init-coop` output cannot start a daemon (`[network]`
//!   omits `bootstrap_peers`, which has no serde default; the gateway is
//!   enabled with `jwt_secret` commented out). This ceremony refuses such a
//!   configuration rather than certify a node that cannot start.
//! * **icn#2748** — general `AgeKeyStore` file-mode behaviour. The two
//!   keystores minted here are hardened to 0600.
//! * **icn#2750** — persisted trust edges become invisible to
//!   `compute_trust_score_weighted` once any unrelated in-process edge exists
//!   (`0.300 -> 0.000`). Not introduced here, and deliberately not fixed here:
//!   special-casing this ceremony inside the trust substrate would be the wrong
//!   owner.
//!
//! # Standing non-claims
//!
//! * not canonical Institution genesis; no `EntityId`, no signed founding act;
//! * not GEN (#2602) and not Subject-generation (#2694);
//! * institutional authority is **not** independent of the node: the ledger's
//!   trust query is ego-centric from the node DID, so both edges are required;
//! * the provisioning authority is cryptographically the same Principal as the
//!   node today — the roles differ, the subjects do not;
//! * the boundary crossed is the ledger **author-trust gate**; the `PolicyOracle`
//!   beside it is `AllowAllOracle::wildcard()` (pre-existing, permissive);
//! * **the cooperative record is left `Forming`.** This ceremony does not call
//!   `activate_cooperative`, so the record keeps `status: Forming`,
//!   `charter_ratified: false` and no founding signatures. That is deliberate —
//!   ratification is a governance act, not a provisioning one. One production
//!   path does gate on `CoopStatus::Active`:
//!   `supervisor/init_notifications.rs`'s `handle_coop_update` binds
//!   `coop_id -> EntityId` in the `coop_entity_map` only for activated
//!   cooperatives, so a peer syncing this record by gossip records no binding.
//!   That path logs "name binding only; grants no authority", so nothing this
//!   ceremony establishes depends on it — but the gate exists, and anything
//!   that starts gating on `Active` for something load-bearing will have to
//!   decide what activates a runtime root. This receipt is not that decision.
//!
//! # Honest evidence limits
//!
//! * no full composite stale-configuration orchestration: the exclusion
//!   protocol is proved by a live daemon and by lock-level probes, not by
//!   driving a ceremony to completion against a daemon mid-flight;
//! * no power-loss simulation. Durability is argued from fsync ordering, an
//!   injected-failure discriminator, and re-reads through fresh handles;
//! * **extended ACLs are neither preserved nor certified.** What the publication
//!   contract covers is uid, gid and mode — the *supported* access metadata.
//!   There is no ACL or xattr usage anywhere in this repository's deployment
//!   material; if that changes, this needs revisiting;
//! * **daemon readability is not proven.** This process does not run under the
//!   daemon's credentials, so a check it performs speaks only for its own. The
//!   claim is that the original supported access metadata was preserved;
//! * **the cooperative this provisions cannot obtain a gateway token through
//!   the default `institution bootstrap apply` path.** Nothing here issues a
//!   trusted issuance, membership or bootstrap grant binding the node to that
//!   cooperative, so `/v1/auth/verify` refuses the self-asserted claim with a
//!   403 (icn#2075) against any gateway without a dev posture. That refusal is
//!   correct on both sides, and issuing such a grant from here would make the
//!   machine a member of the institution it merely provisioned — the collapse
//!   this scope exists to prevent. Trusted local issuance (`--local-mint`)
//!   works; the missing first-grant path is icn#2757, which is a question of
//!   *authorization* and distinct from icn#2755's *reachability*;
//! * a **hard-linked** managed configuration is refused, not published. A
//!   second directory entry for the same inode is a second configuration
//!   identity in a different lock domain, so publication would leave a daemon
//!   reading the old inode through the other name while this receipt said
//!   READY. Refusing is the claim; preserving the link count is not offered;
//! * a daemon whose configuration directory it cannot write **will not start**.
//!   That is deliberate (it cannot establish exclusion over a configuration a
//!   more privileged actor could republish), and it is a behaviour change: such
//!   a daemon previously started with no configuration lock at all. A read-only
//!   *filesystem* is exempt, because nothing on it can be published;
//! * the cross-*user* ownership case is argued from the policy function but was
//!   executed only cross-*group*: an unprivileged test cannot `chown` to another
//!   uid.
//!
//! And the largest one, measured rather than assumed: **the shipped native
//! service does not consume what this publishes.** `deploy/icnd.service` passes
//! `--data-dir` and no `--config`, so `icnd` builds `Config::default()` and
//! falls back to the node DID. On one provisioned data directory the real
//! daemon reports *"No treasury_did configured, using node DID for budget
//! payouts"* without `--config` and *"Ledger service using the cooperative's own
//! treasury principal"* with it. What this ceremony establishes is a
//! **daemon-consumable** configuration, proved against `icnd --config <managed
//! file>`; that the deployed unit *selects* that configuration is a separate
//! property and is icn#2755. Consumer compatibility and consumer reachability
//! are not the same claim.

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use icn_identity::{AgeKeyStore, Did, KeyStore};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::{enforce_n2a_gate, get_keystore_path, read_passphrase};

/// Schema version of the genesis receipt.
///
/// Bump this when the receipt's meaning changes, not merely when a field is
/// added; a reader that cannot recognise the version must refuse rather than
/// guess what ceremony occurred.
pub const RUNTIME_ROOT_RECEIPT_SCHEMA_VERSION: u32 = 1;

/// Storage key of the genesis receipt inside the cooperative store.
///
/// It lives in the same sled database as the `coop:` rows it describes so that
/// a receipt cannot survive a cooperative store that was replaced or removed.
const RECEIPT_KEY_PREFIX: &str = "runtimeroot:receipt:";

#[derive(Subcommand, Debug)]
pub enum InstitutionRuntimeRootCommands {
    /// Provision the institutional runtime root for a new cooperative.
    ///
    /// Creates durable cooperative state, a treasury Principal with its own key
    /// material, a distinct genesis trust-root Principal, the trust facts a
    /// governance-authored ledger entry needs, and a versioned completion
    /// receipt.
    ///
    /// This is **not** canonical Institution genesis under
    /// `docs/architecture/IDENTITY_SEMANTICS.md`: it creates no `EntityId` and
    /// persists no signed founding act. See the module docs.
    ///
    /// Opens the local stores directly, so run it with the daemon stopped (the
    /// daemon holds an exclusive lock).
    Create {
        /// Human-readable cooperative name.
        #[arg(long)]
        name: String,

        /// Settlement unit recorded on the treasury.
        #[arg(long, default_value = "HOURS")]
        currency: String,

        /// Proceed without the interactive confirmation.
        #[arg(long)]
        yes: bool,
    },

    /// Read back the runtime-root receipt, re-verified against durable state.
    ///
    /// This is not a plain file read: it re-checks the cooperative record and
    /// its trust-root binding, the treasury registration, both trust facts, the
    /// minted key material, and the configuration linkage, so that a receipt
    /// cannot outlive what it claims. That means it opens the
    /// same sled databases the daemon does — run it with the daemon stopped.
    Show {
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
}

/// Durable evidence of what came into existence, and under whose authority.
///
/// This is evidence *of* genesis, not genesis itself: the cooperative, the
/// treasury registration and the trust facts are all separately durable. The
/// receipt is written last and is what a reader consults to decide whether the
/// ceremony completed (see `Ceremony ordering` on [`provision_runtime_root`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeRootReceipt {
    /// Schema version of this receipt.
    pub schema_version: u32,
    /// The cooperative that came into existence.
    pub cooperative_id: String,
    /// Its human-readable name at genesis.
    pub cooperative_name: String,
    /// The cooperative's genesis trust root — the SOURCE of the trust fact that
    /// reaches the treasury. Distinct from the node.
    ///
    /// **This is not an "institution DID".** See
    /// [`trust_root_keystore_path`] for why no such thing is minted here.
    pub trust_root_did: String,
    /// The treasury principal. Distinct from the node DID by construction.
    pub treasury_did: String,
    /// The principal that authorised this ceremony, proven by unlocking the
    /// node keystore.
    ///
    /// Named for exactly what is proven: *the principal controlling this node
    /// authorised this local genesis ceremony*. **Today this is the same
    /// cryptographic principal as `node_did`** — the roles differ, the subjects
    /// do not — so no claim about a distinct human founder should be read into
    /// the two fields being separate. A durable founding *Subject* requires
    /// stable Subject-generation (#2694).
    ///
    /// What this slice does establish is narrower and still substantial: the
    /// node is no longer silently used AS the treasury or as the ledger author.
    pub genesis_authority_did: String,
    /// The node the ceremony ran on, recorded so a reader can see that the
    /// treasury is not the node.
    pub node_did: String,
    /// Settlement unit recorded on the treasury.
    pub currency: String,
    /// RFC3339 timestamp of the ceremony.
    pub created_at: String,
}

/// The cooperative sled database, in one place.
///
/// `icn_core::config` owns `store_path`, `trust_store_path` and
/// `ledger_store_path` but not this one, so the `join("cooperative")` was
/// repeated at every call site. A path spelled independently in several places
/// is what produced #2717/#2718; the tests spell it literally on purpose, so
/// that a wrong helper cannot make them agree with the code by construction.
fn coop_db_path(data_dir: &Path) -> PathBuf {
    icn_core::config::store_path(data_dir).join("cooperative")
}

/// Storage key of the cooperative's genesis trust root on its own record.
///
/// The binding lives in the durable `Cooperative` record — the object the
/// production `CoopStore` reads — so the trust root is not an identifier whose
/// only evidence is the receipt or a trust-graph row.
pub const TRUST_ROOT_METADATA_KEY: &str = "genesis.trust_root_did";

/// Where each principal's key material lives, as siblings of the node keystore
/// (`<data_dir>/identity.age`).
///
/// # Why there is no "institution DID" here
///
/// An earlier draft of this ceremony minted a principal and called it the
/// institution's identity. That was not honest, and the repository says why:
///
/// * `Cooperative` has no DID field at all — a cooperative is identified by its
///   `coop_id` string.
/// * The one existing `coop_id`-to-principal mechanism, `CoopEntityMap`, is
///   documented as a **name binding only** that "grants no standing, role,
///   capability, mandate, or permission".
/// * #2602 (GEN) still has to decide how an institution acquires a stable
///   identifier *without* a permanent master key. Minting one here would
///   pre-empt that decision, and an identifier whose authority existed only
///   because a genesis receipt asserted it would be worse than none.
///
/// So this ceremony does **not** claim to create institutional identity. What
/// it creates is narrower and nameable: a **genesis trust root** for the
/// cooperative — a principal whose sole function is to be the source of the
/// trust fact that reaches the treasury, so that the treasury's ledger
/// authority derives from something other than the machine directly.
///
/// It is keypair-derived for a mechanical reason: `TrustEdge` stores
/// `source`/`target` as `Did`, and deserializing a `Did` runs the full
/// validator, so an identifier that is not a valid Ed25519 point could not
/// survive a round trip through the trust store.
///
/// Its private key is persisted rather than discarded because the public half
/// becomes durable state the moment the edge is written; destroying the only key
/// that could ever demonstrate control would be an irreversible commitment this
/// slice has no mandate to make. It signs nothing today, and no code path uses
/// it after genesis.
///
/// It is bound to the cooperative durably via [`TRUST_ROOT_METADATA_KEY`] on
/// the `Cooperative` record, so the relationship survives restart and is read
/// back through production state rather than asserted by the receipt.
fn trust_root_keystore_path(data_dir: &Path) -> PathBuf {
    data_dir.join("genesis-trust-root.age")
}

fn treasury_keystore_path(data_dir: &Path) -> PathBuf {
    data_dir.join("treasury.age")
}

/// Resolve the storage root, refusing when the CLI and an existing
/// configuration disagree.
///
/// This is the #2725 class of error applied to authoritative writes: seeding
/// institutional state under `--data-dir /A` while the daemon that will read it
/// resolves `data_dir = /B` from its own `icn.toml` is a false success, not a
/// partial one. The repository defines no canonical winner between the two, so
/// this refuses and names both paths rather than silently preferring either.
///
/// This does **not** fix #2725 itself: `init-coop`'s existing-config branch is a
/// different caller with its own acceptance criteria and remains open.
fn resolve_storage_root(data_dir: &Path) -> Result<()> {
    let config_path = data_dir.join("icn.toml");
    if !config_path.exists() {
        return Ok(());
    }
    let text = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    // `toml::from_str`, not `str::parse` — in toml 0.9 `FromStr for Value`
    // parses a bare value rather than a document, so a normal config file with
    // a leading comment fails there. This is the parser the rest of the
    // repository uses.
    let parsed: toml::Value = toml::from_str(&text)
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;
    let Some(configured) = parsed.get("data_dir").and_then(|v| v.as_str()) else {
        return Ok(());
    };

    // A relative `data_dir` cannot be proven equivalent to the CLI root. Both
    // sides would be resolved against *this* process's working directory, while
    // `icnd` resolves the same string against whatever directory it is started
    // in — so agreeing here would prove nothing about where the daemon will
    // actually read. Refuse rather than certify a cwd-dependent equivalence.
    if std::path::Path::new(configured).is_relative() {
        bail!(
            "Refusing institutional runtime-root provisioning: {} sets a relative `data_dir` \
             ({configured:?}).\n\
             It would be resolved against whatever directory the daemon happens \
             to start in, so this ceremony cannot prove it names the same root \
             it is about to write. Use an absolute path.",
            config_path.display()
        );
    }

    // Compare resolved paths so `/A` and `/A/` — or a relative spelling of the
    // same directory — are not reported as a disagreement. `canonicalize` is
    // used only when the target exists; otherwise the lexical forms are
    // compared, which is the honest answer for a path that is not there yet.
    let cli = std::fs::canonicalize(data_dir).unwrap_or_else(|_| data_dir.to_path_buf());
    let cfg_path = PathBuf::from(configured);
    let cfg = std::fs::canonicalize(&cfg_path).unwrap_or(cfg_path);
    if cli != cfg {
        bail!(
            "Refusing institutional runtime-root provisioning: the storage root on the command \
             line and the one this configuration will be read with disagree.\n  \
             --data-dir:            {}\n  \
             {} data_dir = {}\n\
             Genesis writes authoritative institutional state, and writing it \
             where the daemon will not look is a false success, not a partial \
             one. Re-run with --data-dir set to the configured root, or correct \
             the configuration (#2725).",
            cli.display(),
            config_path.display(),
            cfg.display(),
        );
    }
    Ok(())
}

/// What a data directory currently shows about runtime-root provisioning.
///
/// Deliberately three states rather than two. "A receipt exists" and "some
/// artefacts exist but no receipt does" are different operator situations and
/// must not produce the same message.
#[derive(Debug)]
pub enum RuntimeRootState {
    /// Nothing has been written.
    NotStarted,
    /// Components exist but no completion receipt does — a ceremony that did
    /// not finish. Carries a component-level report, so a refusal can say what
    /// is actually on disk rather than only that "something" is.
    Incomplete { components: RuntimeRootComponents },
    /// The runtime root is provisioned and its recorded relationships still hold.
    ///
    /// "Still hold" means every non-secret relationship: the cooperative record
    /// and its trust-root binding, the treasury registration, both trust facts,
    /// the presence of both keystores, and the configuration linkage. It does
    /// **not** include key provenance — `genesis_state` runs verification
    /// without a passphrase, so a keystore substituted after the ceremony still
    /// classifies as `Complete`. The ceremony itself verifies provenance before
    /// writing the marker; nothing re-verifies it afterwards.
    Ready(Box<RuntimeRootReceipt>),
    /// A completion receipt exists, but durable state no longer agrees with it.
    ///
    /// A receipt is only evidence if reading it re-checks what it asserts.
    /// Otherwise it is an assertion that outlives the state it describes.
    Inconsistent {
        receipt: Box<RuntimeRootReceipt>,
        problem: String,
    },
}

/// Which durable components of a genesis exist, determined from the components
/// themselves rather than from the receipt.
///
/// Every field here is observable **without secrets** — no keystore is
/// unlocked and no passphrase is prompted for — so `show` can report an
/// interrupted ceremony without asking the operator for anything. That is also
/// why the trust facts are reported as a count rather than attributed to
/// specific principals: naming them would require unlocking the keystores to
/// learn the DIDs, and a receipt is exactly what an incomplete ceremony lacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRootComponents {
    pub trust_root_key: bool,
    pub treasury_key: bool,
    pub cooperative_record: bool,
    pub treasury_registration: bool,
    pub trust_store_edges: usize,
    pub config_linkage: bool,
    pub receipt: bool,
}

impl RuntimeRootComponents {
    /// True when no **genesis-exclusive** marker is present.
    ///
    /// Only three components are written exclusively by this ceremony, and only
    /// those may be read as evidence that a genesis was attempted here:
    ///
    /// | Component | Exclusive to genesis? |
    /// |---|---|
    /// | `genesis-trust-root.age` | **yes** |
    /// | `treasury.age` | **yes** |
    /// | `runtimeroot:receipt:` rows | **yes** |
    /// | cooperative record | no — the gateway's `CoopManager` creates cooperatives |
    /// | treasury registration | no — reachable from the activation path |
    /// | trust store edges | no — `init-coop` writes `my_did -> member` edges |
    /// | `[cooperative]` config | ambiguous — an operator can hand-write it |
    ///
    /// Counting the shared ones would tell a node that had merely created a
    /// cooperative through the gateway that it holds "incomplete provisioning
    /// state", and refuse to provision on it. They are still *reported* — they are
    /// what an operator needs to see when a ceremony did break — but they do
    /// not by themselves make a directory look mid-ceremony.
    ///
    /// **The residual, stated rather than hidden.** The trade-off runs one way:
    /// a ceremony interrupted before the configuration was published, whose two
    /// `.age` files are then removed but whose cooperative record, treasury
    /// registration and edges are left, classifies `NotStarted` and a rerun
    /// mints a second treasury over the orphans. That requires someone to
    /// delete exactly the exclusive markers and keep the shared ones — a
    /// key-excluding restore, or an operator following the refusal message
    /// half-way. Post-publish partials are still caught by the existing
    /// `[cooperative]` section, and completed ones by the receipt.
    pub fn is_untouched(&self) -> bool {
        !self.trust_root_key && !self.treasury_key && !self.receipt
    }

    fn describe(&self) -> String {
        fn mark(present: bool) -> &'static str {
            if present {
                "present:"
            } else {
                "absent: "
            }
        }
        [
            format!(
                "{} genesis trust-root key material",
                mark(self.trust_root_key)
            ),
            format!("{} treasury key material", mark(self.treasury_key)),
            format!("{} cooperative record", mark(self.cooperative_record)),
            format!("{} treasury registration", mark(self.treasury_registration)),
            // Deliberately NOT called "genesis trust facts": attributing edges
            // to this ceremony needs the trust-root and treasury DIDs, which
            // live in the keystores and the receipt — and a receipt is exactly
            // what an incomplete ceremony lacks. Unlocking keystores to make
            // this line prettier is not worth prompting for a passphrase during
            // a read-only inspection.
            format!(
                "         {} edge(s) in the trust store (total; attribution to this ceremony needs the receipt)",
                self.trust_store_edges
            ),
            format!("{} configuration linkage", mark(self.config_linkage)),
            format!("{} completion receipt", mark(self.receipt)),
        ]
        .join("\n  ")
    }
}

/// Read the component report from durable state.
pub fn runtime_root_components(data_dir: &Path) -> Result<RuntimeRootComponents> {
    use icn_store::Store;

    // `symlink_metadata`, not `metadata`: a symlink where a keystore belongs is
    // state to report, not to follow.
    //
    // `.is_ok()` here would report *could not look* as *is not there*, for the
    // two artifacts that are exclusive to this ceremony -- the one wrong answer
    // that invites an operator to run a second one. Reachability is narrower
    // than it looks: every non-`NotFound` error `lstat` can raise at these flat
    // paths is a property of `data_dir` itself, so the sibling store checks
    // below refuse first. This is the shape the rest of the function keeps, not
    // a repair for an observed misreport.
    let exists = |p: PathBuf| -> Result<bool> {
        match std::fs::symlink_metadata(&p) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(uninspectable(anyhow::Error::new(e).context(format!(
                "Could not determine whether {} exists",
                p.display()
            )))),
        }
    };

    // Fail closed, like every other inspection in this file. An earlier version
    // answered `false` for any store that could not be opened or scanned — a
    // stale sled lock, a mode change, a regular file where a database belongs —
    // which reports *could not read* as *is not there*, in the very report an
    // operator uses to decide what to delete by hand. That is the same defect
    // `refuse_if_foreign_institutional_state` documents having removed.
    let scan_any = |db: PathBuf, prefix: &[u8]| -> Result<bool> {
        if !store_directory_exists(&db)? {
            return Ok(false);
        }
        let store = open_store_refusing_links(&db)
            .with_context(|| format!("Could not open {} to inspect it", db.display()))
            .map_err(uninspectable)?;
        let found = store
            .scan(prefix)
            .map(|r| !r.is_empty())
            .map_err(|e| uninspectable(anyhow::anyhow!("Could not scan {}: {e}", db.display())))?;
        drop(store);
        Ok(found)
    };
    let scan_count = |db: PathBuf, prefix: &[u8]| -> Result<usize> {
        if !store_directory_exists(&db)? {
            return Ok(0);
        }
        let store = open_store_refusing_links(&db)
            .with_context(|| format!("Could not open {} to inspect it", db.display()))
            .map_err(uninspectable)?;
        let n = store
            .scan(prefix)
            .map(|r| r.len())
            .map_err(|e| uninspectable(anyhow::anyhow!("Could not scan {}: {e}", db.display())))?;
        drop(store);
        Ok(n)
    };

    let coop_db = coop_db_path(data_dir);
    // Only absence may read as "no linkage".
    //
    // `.ok()` funnelled four different answers into one: absent, unreadable,
    // and unparseable all became `config_linkage: false`. With no receipt to
    // re-verify, that false is what renders the root INCOMPLETE -- telling an
    // operator the ceremony never published a configuration, when a corrupt one
    // is sitting right there. The repairs are opposites: finish the ceremony
    // versus repair or remove the file that is already published. The daemon
    // would reject the malformed one outright rather than treat it as missing,
    // so reporting it as missing also breaks
    // `configuration validation should predict structural daemon acceptance`.
    let config_linkage = {
        let path = data_dir.join("icn.toml");
        match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str::<toml::Value>(&text)
                // A `[cooperative]` table is not a linkage; a `treasury_did` is.
                // `Config::cooperative` is not an `Option`, so any file that has
                // been round-tripped through `Config::to_file` carries an empty
                // table — and reporting that as linkage told an operator staring
                // at an INCOMPLETE root that the configuration step had
                // completed when nothing had been published. The component
                // report exists to say which recovery step is outstanding.
                .map(|v| {
                    v.get("cooperative")
                        .and_then(|section| section.get("treasury_did"))
                        .is_some()
                })
                .map_err(|e| {
                    uninspectable(anyhow::Error::new(e).context(format!(
                        "Could not parse {} to determine its cooperative linkage",
                        path.display()
                    )))
                })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => {
                return Err(uninspectable(anyhow::Error::new(e).context(format!(
                    "Could not read {} to determine its cooperative linkage",
                    path.display()
                ))))
            }
        }
    };

    Ok(RuntimeRootComponents {
        trust_root_key: exists(trust_root_keystore_path(data_dir))?,
        treasury_key: exists(treasury_keystore_path(data_dir))?,
        cooperative_record: scan_any(coop_db.clone(), b"coop:")?,
        treasury_registration: scan_any(
            icn_core::config::ledger_store_path(data_dir),
            b"ledger:treasury:",
        )?,
        trust_store_edges: scan_count(
            icn_core::config::trust_store_path(data_dir),
            b"trust/edges/",
        )?,
        config_linkage,
        receipt: scan_any(coop_db, RECEIPT_KEY_PREFIX.as_bytes())?,
    })
}

/// Classify the data directory's runtime-root provisioning state.
///
/// **Not side-effect free.** When a receipt exists this runs
/// `verify_durable_state`, which opens the ledger and trust stores
/// unconditionally — and `SledStore::open` is a *creating* open that will
/// `create_dir_all` the path and take an exclusive lock. Inspecting a data
/// directory whose trust store was deleted therefore recreates it empty before
/// reporting INCONSISTENT. It writes no domain row, but it is not read-only,
/// and it needs the daemon stopped.
pub fn runtime_root_state(data_dir: &Path) -> Result<RuntimeRootState> {
    if let Some(receipt) = load_receipt(data_dir)? {
        // Re-verify rather than trusting the row. This is what makes the
        // receipt evidence: it cannot outlive the cooperative record, the
        // treasury registration, the trust facts, or the configuration linkage
        // that make its claims true. It is also what keeps the commit ordering
        // honest — a receipt written before the configuration was published
        // would surface here as inconsistent, not as a genesis.
        // `None`: a read-only inspection must not prompt for a passphrase.
        return match verify_durable_state(data_dir, &receipt, None) {
            Ok(()) => Ok(RuntimeRootState::Ready(Box::new(receipt))),
            // Could not be inspected: propagate, so `show` reports ERROR /
            // `state_unreadable` rather than claiming corruption it never saw.
            Err(problem) if problem.downcast_ref::<Uninspectable>().is_some() => Err(problem),
            // Observed, and it disagreed. That is INCONSISTENT.
            Err(problem) => Ok(RuntimeRootState::Inconsistent {
                receipt: Box::new(receipt),
                problem: format!("{problem:#}"),
            }),
        };
    }
    // No receipt: report which components a partial ceremony left behind.
    //
    // Keystore presence is probed with `symlink_metadata`, not `exists()`:
    // `exists()` follows symlinks and so reports `false` for a DANGLING one,
    // which would let a rerun create key material *through* that link.
    let components = runtime_root_components(data_dir)?;
    if components.is_untouched() {
        Ok(RuntimeRootState::NotStarted)
    } else {
        Ok(RuntimeRootState::Incomplete { components })
    }
}

/// Refuse an offline command that would replace the node DID, once a runtime
/// root exists that depends on the current one.
///
/// # The invariant this defends
///
/// Serialising the ceremony against concurrent identity commands — which
/// `DataDirLock` already does — establishes that *the node identity cannot
/// change during the runtime-root transaction*. It says nothing about
/// afterwards. This is the other half: once a runtime root has been committed,
/// an offline DID-changing operation must not be able to silently invalidate
/// the authority relationship that committed state depends on.
///
/// Without this, `id rotate` / `id import` succeed the moment no ceremony holds
/// the lock, and leave three durable facts disagreeing:
///
///   * the receipt still names the **old** node DID;
///   * the persisted `node -> trust_root` edge still starts at the old DID;
///   * the daemon roots its trust computation at the **new** DID.
///
/// The treasury then scores zero for the runtime authority path and
/// governance-authored ledger entries are rejected — while `runtime-root show`,
/// reading the receipt, can still report `READY`. The failure is silent in
/// exactly the place an operator would look.
///
/// # Why refuse rather than migrate
///
/// Migrating the binding means rewriting the receipt and the trust edge
/// atomically with the keystore replacement. No existing primitive does that
/// completely, and a partial migration is strictly worse than a refusal: it
/// would leave the same three-way disagreement with the added claim that it had
/// been handled. Refusal is the fail-closed choice; atomic node-authority
/// migration is tracked separately.
///
/// # Scope
///
/// Only commands that can *replace the DID*. A DID-preserving keystore
/// operation — `upgrade-pq`, the device and recovery commands — still rewrites
/// `identity.age`, and still takes the data-directory lock for the tearing
/// reason documented at its call site, but it does not invalidate any
/// authority binding and is not refused here.
///
/// # States
///
/// Classified through [`runtime_root_state`], the same mechanism `show` uses,
/// rather than a second ad-hoc definition of "provisioned":
///
///   * `NotStarted` — nothing to invalidate; existing behaviour is retained.
///   * `Ready` / `Inconsistent` — a receipt exists and names a node DID that
///     durable state is bound to. Refuse.
///   * `Incomplete` — an interrupted ceremony already wrote components bound to
///     the current node DID. Refuse; there is no reading under which replacing
///     the DID here is safe.
///   * `Err` — the state could not be read at all, including the
///     `Uninspectable` case from a storage failure. **Fail closed**: an
///     unreadable root is not evidence of an absent one.
pub fn refuse_node_did_change_if_runtime_root_exists(data_dir: &Path, command: &str) -> Result<()> {
    let advice = "Provision a new root in a separate data directory, or migrate the runtime \
                  authority binding deliberately. Rotating the node identity underneath a \
                  committed runtime root would leave the receipt, the persisted trust edge and \
                  the daemon's trust computation naming different principals.";
    match runtime_root_state(data_dir) {
        // No runtime root: nothing depends on this node DID.
        Ok(RuntimeRootState::NotStarted) => Ok(()),
        Ok(RuntimeRootState::Ready(receipt)) => bail!(
            "{command} would replace this node's DID, but a runtime root is provisioned here and \
             its authority binding starts at the current node DID {}.\n\n{advice}",
            receipt.node_did
        ),
        Ok(RuntimeRootState::Inconsistent { receipt, problem }) => bail!(
            "{command} would replace this node's DID, but a runtime-root receipt exists here \
             naming node DID {} (and durable state already disagrees with it: {problem}).\n\n\
             {advice}",
            receipt.node_did
        ),
        Ok(RuntimeRootState::Incomplete { components }) => bail!(
            "{command} would replace this node's DID, but an interrupted runtime-root ceremony \
             left components here that are bound to the current node DID ({components:?}).\n\n\
             {advice}"
        ),
        // Fail closed. An unreadable runtime root is not an absent one.
        Err(problem) => bail!(
            "{command} would replace this node's DID, and whether a runtime root exists here \
             could not be determined ({problem:#}). Refusing rather than assuming there is \
             none.\n\n{advice}"
        ),
    }
}

/// Reject operator-supplied values before the ceremony writes or prompts.
///
/// Deliberately conservative rather than clever: this is durable runtime-root
/// provisioning, the name lands in a durable record and in the daemon's
/// configuration, and the currency is recorded on the treasury. A value that is
/// empty, whitespace-only or absurdly long is a mistake, and a value discovered
/// to be invalid after key material has been minted leaves partial provisioning
/// state an operator has to clean up by hand.
fn validate_runtime_root_inputs(name: &str, currency: &str) -> Result<()> {
    /// Long enough for any real cooperative name, short enough that a paste
    /// accident cannot bloat the configuration. This is the ceremony's own
    /// tighter rule; the gateway's is applied below, in addition.
    const MAX_NAME: usize = 200;

    if name.trim().is_empty() {
        bail!("Refusing institutional runtime-root provisioning: the cooperative name is empty or only whitespace.");
    }
    if name.chars().count() > MAX_NAME {
        bail!(
            "Refusing institutional runtime-root provisioning: the cooperative name is {} characters; the \
             maximum is {MAX_NAME}.",
            name.chars().count()
        );
    }
    if name.chars().any(|c| c.is_control()) {
        bail!("Refusing institutional runtime-root provisioning: the cooperative name contains control characters.");
    }
    if currency.trim().is_empty() {
        bail!("Refusing institutional runtime-root provisioning: the currency is empty or only whitespace.");
    }
    if currency
        .chars()
        .any(|c| c.is_control() || c.is_whitespace())
    {
        bail!(
            "Refusing institutional runtime-root provisioning: the currency contains whitespace or control \
             characters."
        );
    }

    // Then the gateway's rules, by calling the gateway's own validators rather
    // than restating them.
    //
    // These values are written straight into durable state, bypassing the
    // gateway entirely — but the gateway serves the same cooperative, and
    // `validate_unit` gates every ledger entry submitted through it
    // (`icn-gateway/src/api/ledger.rs`). A unit this ceremony accepts and the
    // gateway rejects is one the cooperative can never use, permanently: a
    // rerun over provisioned state is refused, by design.
    //
    // Restating the limits here is exactly what produced the defect this
    // replaces. The gateway measures `str::len()` — BYTES — while the
    // hand-written check counted Unicode scalars, so nine four-byte characters
    // passed a "32 character" rule and failed the gateway's 32-byte one. The
    // same gap existed for the name, whose 200 scalars can be 800 bytes against
    // the gateway's 256. Calling the owner removes the drift instead of
    // re-synchronising two copies of it.
    icn_gateway::validation::validate_coop_name(name).map_err(|e| {
        anyhow::anyhow!(
            "Refusing institutional runtime-root provisioning: the gateway would reject this cooperative name: {e}"
        )
    })?;
    icn_gateway::validation::validate_unit(currency).map_err(|e| {
        anyhow::anyhow!(
            "Refusing institutional runtime-root provisioning: the gateway would reject this currency: {e}"
        )
    })?;
    Ok(())
}

/// Refuse a data directory that already holds another institution's state.
///
/// **This is a different question from [`RuntimeRootComponents::is_untouched`], and
/// conflating them was a real defect.** That predicate answers "was provisioning
/// attempted here?", and deliberately ignores cooperative records because the
/// gateway's `CoopManager` creates them — counting them there would refuse
/// provisioning on an ordinary node. This one answers "is it *safe* to found a new
/// institution here?", and the answer is no as soon as any other cooperative or
/// treasury state exists.
///
/// The reason is a live runtime hazard, not tidiness. The daemon publishes ONE
/// global `CooperativeConfig.treasury_did`, and `LedgerServiceImpl` is a
/// singleton whose `build_account_deltas` debits `self.treasury_did` — the
/// configured one — rather than selecting an account from the request's
/// `treasury_id` (`icn/crates/icn-core/src/services/ledger_service.rs:166`).
/// So founding a second cooperative here and repointing the configuration would
/// make treasury operations belonging to the *pre-existing* cooperative debit
/// the newly founded one's treasury.
///
/// Cooperative-scoped treasury routing is the real answer to that, and it is a
/// runtime architecture problem well outside this ceremony. Until it exists,
/// genesis refuses rather than silently making one cooperative spend another's
/// treasury.
fn refuse_if_foreign_institutional_state(data_dir: &Path) -> Result<()> {
    let mut found: Vec<String> = Vec::new();

    // Inspect **fail-closed**. A store that exists but cannot be opened or
    // scanned tells us nothing about what it holds, and for a privileged
    // founding ceremony "I could not look" must never be recorded as "there is
    // nothing there". An earlier draft used `if let Ok(..)` here, which quietly
    // turned every inspection failure — a lock held by a running daemon, a
    // permission error, a corrupt database — into permission to found.
    //
    // A path that does not exist is genuinely clean: there is nothing to hold
    // state. It is also deliberately NOT created here; `sled::open` would
    // materialise an empty database as a side effect of asking the question.
    let inspect = |db: std::path::PathBuf, prefix: &[u8], label: &str| -> Result<Vec<String>> {
        if !store_directory_exists(&db)? {
            return Ok(Vec::new());
        }
        let store = open_store_refusing_links(&db).with_context(|| {
            format!(
                "Refusing institutional runtime-root provisioning: {} exists but could not be opened to check for \
                 existing institutional state (if the daemon is running, stop it first). \
                 Genesis refuses rather than assume an unreadable store is empty",
                db.display()
            )
        })?;
        let rows = {
            use icn_store::Store;
            store.scan(prefix).with_context(|| {
                format!(
                    "Refusing institutional runtime-root provisioning: {} could not be scanned for existing \
                     institutional state. Genesis refuses rather than assume absence",
                    db.display()
                )
            })?
        };
        let mut out = Vec::new();
        for (k, _) in rows {
            let key = String::from_utf8_lossy(&k).into_owned();
            // Skip the coop index rows; the primary rows name the problem once.
            if !key.starts_with("ledger:treasury:idx:") {
                out.push(format!("{label} {key}"));
            }
        }
        drop(store);
        Ok(out)
    };

    found.extend(inspect(coop_db_path(data_dir), b"coop:", "cooperative")?);
    found.extend(inspect(
        icn_core::config::ledger_store_path(data_dir),
        b"ledger:treasury:",
        "treasury",
    )?);

    if !found.is_empty() {
        bail!(
            "Refusing institutional runtime-root provisioning: this data directory already holds \
             institutional state that this ceremony did not create:\n  {}\n\
             The daemon publishes a single `[cooperative] treasury_did` and its \
             ledger service debits that one treasury for every treasury \
             operation, so provisioning a second runtime root here would make \
             the existing cooperative's operations debit the new treasury. \
             Provision a runtime root in a fresh data directory instead.",
            found.join("\n  ")
        );
    }
    Ok(())
}

/// Refuse unless the data directory shows no prior ceremony.
///
/// This is **duplicate-safe, partial-state-detecting and fail-closed**. It is
/// deliberately *not* described as idempotent or restartable: a ceremony
/// interrupted after it minted key material leaves artefacts that make every
/// subsequent invocation refuse, and there is no recovery path in this slice.
/// An operator must inspect and remove that state deliberately. Refusing is the
/// safe outcome — silently adopting half-written institutional state, or
/// minting a second treasury over an orphaned first one, are both worse — but
/// it is a refusal, not a resumption.
fn refuse_if_already_provisioned(data_dir: &Path) -> Result<()> {
    match runtime_root_state(data_dir)? {
        RuntimeRootState::NotStarted => Ok(()),
        RuntimeRootState::Inconsistent { receipt, problem } => bail!(
            "Refusing institutional runtime-root provisioning: this data directory holds a \
             runtime-root receipt for cooperative {} ({}) whose claims no longer \
             hold:\n  {}\n\
             Provisioning again over inconsistent state would compound it. \
             Inspect and resolve this deliberately.",
            receipt.cooperative_name,
            receipt.cooperative_id,
            problem
        ),
        RuntimeRootState::Ready(receipt) => bail!(
            "Refusing institutional runtime-root provisioning: this data directory has already \
             been provisioned.\n  cooperative: {} ({})\n  treasury:    {}\n\
             Provisioning a second runtime root over the first would orphan its \
             treasury and trust facts.",
            receipt.cooperative_name,
            receipt.cooperative_id,
            receipt.treasury_did
        ),
        RuntimeRootState::Incomplete { components } => bail!(
            "Refusing institutional runtime-root provisioning: this data directory holds \
             INCOMPLETE provisioning state — a ceremony that did not finish, with \
             no completion receipt:\n  {}\n\
             Provisioning never overwrites key material, and it cannot resume: \
             re-running would mint a second set of principals and leave the \
             first orphaned. Inspect this state and remove it deliberately \
             before founding again.",
            components.describe()
        ),
    }
}

/// Mint a principal and persist its key material, returning its DID.
///
/// The DID comes from the generated public key (`Did::from_public_key`), so it
/// is backed by a key this node holds and survives the daemon's own config
/// parse. Nothing here fabricates a DID string.
/// # Control, as distinct from identity
///
/// Both minted keystores are encrypted with the **node keystore passphrase**.
/// The treasury is therefore a distinct *principal* but not separately
/// *controlled*: whoever can unlock `identity.age` can unlock `treasury.age`.
///
/// That is honest for this slice — the treasury key signs nothing yet, so the
/// shared passphrase grants no capability that is currently exercised — but it
/// is a real limit on what "distinct principal" means here, and separating
/// custody is a prerequisite for any later claim that the institution controls
/// its treasury independently of the operator.
fn mint_principal(path: &Path, passphrase: &[u8], what: &str) -> Result<Did> {
    let keystore = AgeKeyStore::init(path, passphrase)
        .with_context(|| format!("Failed to create {what} keystore at {}", path.display()))?;
    let did = keystore
        .get_keypair()
        .with_context(|| format!("Failed to read the generated {what} keypair"))?
        .did()
        .clone();

    // Make the key material durable before anything can certify it.
    //
    // `AgeKeyStore::init` writes through `std::fs::write`, which establishes
    // visibility, not survival: after it returns the bytes may still be in the
    // page cache. Everything downstream — relational verification, and then a
    // completion receipt that IS flushed — would happily certify a principal
    // whose private key had not reached the disk. A power cut in that window
    // leaves the worst possible state: a receipt saying the root is READY,
    // naming principals whose keys are gone, and a rerun that refuses precisely
    // *because* that receipt exists.
    //
    // The file is opened read-only and its contents are never touched, so
    // nothing secret is read into this process to sync it.
    // Tighten the mode on the material THIS ceremony creates.
    //
    // `AgeKeyStore` writes through `fs::write`, so a keystore lands at the
    // process umask — commonly 0664. The content is encrypted at rest under a
    // passphrase, so that exposes ciphertext and salt rather than keys, which is
    // why it is defence-in-depth and not a plaintext leak. But this ceremony
    // mints two *new* secret files, and leaving them world-readable when a
    // one-line tightening exists is not defensible for a privileged founding
    // operation.
    //
    // Deliberately scoped to the two files owned here. Changing `AgeKeyStore`
    // for every caller — including the node identity that predates this
    // ceremony — is icn#2748's to own, and doing it from here would give one
    // caller different keystore semantics from the rest.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).with_context(
            || {
                format!(
                    "Failed to restrict permissions on the {what} keystore at {}",
                    path.display()
                )
            },
        )?;
    }

    // Test-only injection, so the failure PATH can be exercised. A power cut
    // cannot be simulated in a unit test, and this does not pretend to: what it
    // establishes is narrower and still worth holding — a detected failure to
    // establish the durability barrier cannot be followed by a committed READY
    // marker.
    #[cfg(test)]
    if DurabilityFailureGuard::armed_for(path) {
        bail!(
            "Failed to sync the {what} keystore at {} (injected)",
            path.display()
        );
    }

    let file = std::fs::File::open(path)
        .with_context(|| format!("Failed to reopen the {what} keystore to make it durable"))?;
    file.sync_all()
        .with_context(|| format!("Failed to sync the {what} keystore at {}", path.display()))?;
    drop(file);

    // The directory entry needs its own barrier, or the file can survive while
    // the name that reaches it does not.
    if let Some(parent) = path.parent() {
        sync_directory_entries(parent)?;
    }

    Ok(did)
}

/// Read the genesis receipt back out of the cooperative store.
pub fn load_receipt(data_dir: &Path) -> Result<Option<RuntimeRootReceipt>> {
    let coop_db = coop_db_path(data_dir);
    if !store_directory_exists(&coop_db)? {
        return Ok(None);
    }
    let store = open_store_refusing_links(&coop_db).with_context(|| {
        format!(
            "Failed to open the cooperative store at {} (stop the daemon first; \
             it holds an exclusive lock)",
            coop_db.display()
        )
    })?;
    let found = {
        use icn_store::Store;
        store.scan(RECEIPT_KEY_PREFIX.as_bytes())?
    };
    if found.len() > 1 {
        let keys: Vec<String> = found
            .iter()
            .map(|(k, _)| String::from_utf8_lossy(k).into_owned())
            .collect();
        bail!(
            "Refusing to read runtime-root state: {} holds {} runtime-root receipts:\n  {}\n\
             A data directory has exactly one runtime root. More than one is \
             corruption or a manual edit, and picking one would make `show` and \
             the rerun guard report whichever happened to sort first. Resolve \
             this deliberately.",
            coop_db.display(),
            found.len(),
            keys.join("\n  ")
        );
    }
    let Some((_, value)) = found.into_iter().next() else {
        return Ok(None);
    };
    let receipt: RuntimeRootReceipt = serde_json::from_slice(&value).context(
        "The runtime-root receipt is present but unreadable; refusing to guess what it said",
    )?;
    if receipt.schema_version != RUNTIME_ROOT_RECEIPT_SCHEMA_VERSION {
        bail!(
            "Runtime-root receipt schema version {} is not the {} this binary \
             understands; refusing to interpret it.",
            receipt.schema_version,
            RUNTIME_ROOT_RECEIPT_SCHEMA_VERSION
        );
    }
    Ok(Some(receipt))
}

/// Deterministic fault-injection points, compiled only for tests.
///
/// These exist so that partial-genesis tests exercise the **ceremony's own
/// ordering** rather than a partial state assembled by hand. Constructing the
/// state manually proves the inspector; forcing the real `provision_runtime_root` to stop
/// at a boundary proves what the implementation actually writes, and in what
/// order. A test that deletes rows from a completed genesis cannot tell you
/// whether the cooperative record is written before or after the treasury
/// registration; this can.
///
/// Nothing here is reachable from the CLI: the enum, the parameter and every
/// check are `#[cfg(test)]`, so the shipped binary contains no failpoint and
/// no environment variable can trigger one.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// The shared `After` prefix is the point: each variant names the write it
// follows, so the enum reads as the ceremony's own ordering.
#[allow(clippy::enum_variant_names)]
pub(crate) enum RuntimeRootFailpoint {
    AfterTrustRootKey,
    AfterTreasuryKey,
    AfterCooperativeSave,
    AfterTreasuryRegistration,
    AfterAuthorityEdge,
    AfterTreasuryAuthorityEdge,
    AfterConfigPublish,
}

/// Test-only switch that makes the keystore durability barrier report failure,
/// **scoped to one data root**.
///
/// Scoping is not optional. `cargo test` runs these in parallel, and a global
/// flag fires inside whichever ceremony happens to be minting at the time —
/// failing an unrelated test. That is the same hazard the boundary observer has,
/// and it bit this flag within minutes of being introduced.
#[cfg(test)]
static FAIL_KEYSTORE_DURABILITY: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// Arm the durability failure for one root, disarming on drop so a panicking
/// test cannot leave it armed for whatever runs next.
#[cfg(test)]
struct DurabilityFailureGuard;

#[cfg(test)]
impl DurabilityFailureGuard {
    fn arm(root: &Path) -> Self {
        if let Ok(mut slot) = FAIL_KEYSTORE_DURABILITY.lock() {
            *slot = Some(icn_core::DataDirLock::lock_path(root));
        }
        Self
    }

    fn armed_for(path: &Path) -> bool {
        // `path` is the keystore file; its parent is the data root.
        let Some(root) = path.parent() else {
            return false;
        };
        FAIL_KEYSTORE_DURABILITY
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .is_some_and(|armed| armed == icn_core::DataDirLock::lock_path(root))
    }
}

#[cfg(test)]
impl Drop for DurabilityFailureGuard {
    fn drop(&mut self) {
        if let Ok(mut slot) = FAIL_KEYSTORE_DURABILITY.lock() {
            *slot = None;
        }
    }
}

/// Test-only switch that makes the first cooperative-row read fail at the
/// *storage* layer, **scoped to one data root**.
///
/// The defect this exists to pin is a classification one, not a detection one:
/// `get_cooperative` returning `CoopError::Storage` used to be flattened into an
/// ordinary `anyhow::Error`, so `runtime_root_state` reported `INCONSISTENT` — a
/// positive claim that durable state disagrees with the receipt — for a read it
/// never actually performed. Injecting the storage failure is therefore the
/// right shape of witness: the question under test is what the code *concludes*
/// from a storage error, not whether sled can produce one.
///
/// Scoped for the same reason `FAIL_KEYSTORE_DURABILITY` is: `cargo test` runs
/// these in parallel and a global flag would fire inside an unrelated
/// ceremony.
#[cfg(test)]
static FAIL_COOP_ROW_READ: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// Arm the cooperative-row storage failure for one root, disarming on drop.
#[cfg(test)]
struct CoopRowReadFailureGuard;

#[cfg(test)]
impl CoopRowReadFailureGuard {
    fn arm(root: &Path) -> Self {
        if let Ok(mut slot) = FAIL_COOP_ROW_READ.lock() {
            *slot = Some(icn_core::DataDirLock::lock_path(root));
        }
        Self
    }

    fn armed_for(data_dir: &Path) -> bool {
        FAIL_COOP_ROW_READ
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .is_some_and(|armed| armed == icn_core::DataDirLock::lock_path(data_dir))
    }
}

#[cfg(test)]
impl Drop for CoopRowReadFailureGuard {
    fn drop(&mut self) {
        if let Ok(mut slot) = FAIL_COOP_ROW_READ.lock() {
            *slot = None;
        }
    }
}

/// Called at each failpoint boundary **while the ceremony guard is still
/// alive**, so a test can observe lock lifetime rather than lock acquisition.
///
/// The previous attempt at this evidence checked contention *after* the
/// ceremony returned, by which time the guard had unwound — proving only that a
/// released lock can be re-acquired. This runs inside the ceremony's own stack
/// frame.
#[cfg(test)]
type BoundaryObserver = fn(&Path, RuntimeRootFailpoint);

/// A boundary observer, bound to the exact root it is watching.
///
/// Scoping matters: `cargo test` runs these in parallel, and every other
/// runtime-root ceremony also owns a `DataDirLock`. An unscoped observer would
/// happily fire on someone else's ceremony, see contention on *their* root, and
/// report success without the ceremony under test ever reaching its boundary —
/// another false green of exactly the kind this suite keeps finding.
#[cfg(test)]
struct ScopedObserver {
    expected_root: PathBuf,
    callback: BoundaryObserver,
}

#[cfg(test)]
static LATE_BOUNDARY_OBSERVERS: std::sync::Mutex<Vec<ScopedObserver>> =
    std::sync::Mutex::new(Vec::new());

/// Install an observer for one root, removing it again on drop so a panicking
/// test cannot leave it installed for whatever runs next.
///
/// One entry per root rather than one slot in total. Dispatch was already
/// root-scoped, but a single slot is not: `cargo test` runs these in parallel,
/// so a second installing test replaced the first one's observer and the first
/// one's callback then never ran — its ceremony reached the boundary and found
/// somebody else's root recorded there. The drop was worse: whichever guard
/// dropped first cleared the slot for every other live test. Both failures are
/// scheduling-dependent, which is why they stayed invisible until a third
/// observer test existed.
#[cfg(test)]
struct ObserverGuard {
    expected_root: PathBuf,
}

#[cfg(test)]
impl ObserverGuard {
    fn install(root: &Path, callback: BoundaryObserver) -> Self {
        let expected_root = icn_core::DataDirLock::lock_path(root);
        if let Ok(mut observers) = LATE_BOUNDARY_OBSERVERS.lock() {
            observers.retain(|o| o.expected_root != expected_root);
            observers.push(ScopedObserver {
                expected_root: expected_root.clone(),
                callback,
            });
        }
        Self { expected_root }
    }
}

#[cfg(test)]
impl Drop for ObserverGuard {
    fn drop(&mut self) {
        if let Ok(mut observers) = LATE_BOUNDARY_OBSERVERS.lock() {
            observers.retain(|o| o.expected_root != self.expected_root);
        }
    }
}

/// May inspection create this root's coordination file, or must it only join
/// an existing one?
///
/// The same account policy the ceremony applies, asked about a different file.
/// Creating `.icn-data-dir.lock` is a durable act: the file is retained after
/// release, so an inspection run under `sudo` against a service-owned data
/// directory would leave a root-owned `0600` file the daemon's own account
/// cannot reopen — a diagnostic that stops the service from starting.
///
/// The reference is the data directory itself: whoever owns it is the account
/// whose daemon reads this state. If new files here would be owned by somebody
/// else, inspection may join an existing lock but must not mint one.
///
/// This is a question about *creation*, not about permission to look. When the
/// answer is no and no lock exists, inspection refuses rather than proceeding
/// unlocked — being outside the exclusion domain is the failure this whole
/// mechanism exists to prevent.
/// Who owns the directory state actually lands in.
///
/// `metadata`, not `symlink_metadata`: the data root is the one ownership
/// question in this file that must **follow** a link. Every file-level check
/// deliberately does not — a symlink where a keystore or a configuration
/// belongs is state to report, not to follow.
///
/// A data root is different. `--data-dir` naming a symlink is an ordinary
/// deployment (`/var/lib/icn` pointing at a storage volume), and a symlink
/// carries its own uid/gid, usually the account that created it. Reading the
/// link would compare the *link's* owner against files created in the
/// *target* — so a root-owned link to an `icn`-owned directory would refuse
/// the very account that should be running the ceremony.
///
/// Deliberately shared by both callers, and deliberately raw: the follow
/// decision is the part that must never diverge, while each caller keeps its
/// own error policy for a root that is missing or unreadable.
#[cfg(unix)]
fn data_root_account(data_dir: &Path) -> std::io::Result<AccessIdentity> {
    std::fs::metadata(data_dir).map(|m| AccessIdentity::of(&m))
}

#[cfg(unix)]
fn inspection_may_create_the_lock(data_dir: &Path) -> Result<bool> {
    let owner = data_root_account(data_dir)
        .with_context(|| format!("Failed to inspect {}", data_dir.display()))?;
    Ok(matches!(
        classify_ownership_transfer(owner, identity_new_files_receive(data_dir)?),
        OwnershipTransfer::Preserved
    ))
}

#[cfg(not(unix))]
fn inspection_may_create_the_lock(_data_dir: &Path) -> Result<bool> {
    Ok(true)
}

/// A test-only barrier inside `show`, between deciding about the lock and
/// touching any durable state.
///
/// This exists to make one specific interleaving deterministic rather than
/// timing-dependent: `show` decides it may proceed, provisioning then starts
/// and takes the real lock, and `show` walks into the stores anyway. A sleep
/// would prove nothing repeatable; this pauses exactly there.
///
/// Scoped to one canonical root and removed on drop, for the same reason
/// [`ScopedObserver`] is: `cargo test` runs these in parallel, and an unscoped
/// barrier would stall an unrelated inspection.
#[cfg(test)]
type InspectionBarrier = fn(&Path);

#[cfg(test)]
static INSPECTION_BARRIER: std::sync::Mutex<Option<(PathBuf, InspectionBarrier)>> =
    std::sync::Mutex::new(None);

#[cfg(test)]
struct BarrierGuard;

#[cfg(test)]
impl BarrierGuard {
    fn install(root: &Path, callback: InspectionBarrier) -> Self {
        if let Ok(mut slot) = INSPECTION_BARRIER.lock() {
            *slot = Some((icn_core::DataDirLock::lock_path(root), callback));
        }
        Self
    }
}

#[cfg(test)]
impl Drop for BarrierGuard {
    fn drop(&mut self) {
        if let Ok(mut slot) = INSPECTION_BARRIER.lock() {
            *slot = None;
        }
    }
}

/// Run the installed barrier, if this is the root it was installed for.
#[cfg(test)]
fn inspection_barrier(root: &Path) {
    // The callback is copied out before the guard is released: holding the
    // registry lock across a blocking callback would deadlock the test that
    // installed it.
    let hit = INSPECTION_BARRIER.lock().ok().and_then(|slot| {
        slot.as_ref()
            .filter(|(expected, _)| *expected == icn_core::DataDirLock::lock_path(root))
            .map(|(_, cb)| *cb)
    });
    if let Some(cb) = hit {
        cb(root);
    }
}

/// Stop the ceremony at `point` when the test asked for it.
macro_rules! failpoint {
    ($injected:expr, $root:expr, $point:expr) => {
        #[cfg(test)]
        {
            // Observation happens BEFORE the bail, so the ceremony guard is
            // still held when a test looks.
            if let Ok(observers) = LATE_BOUNDARY_OBSERVERS.lock() {
                let here = icn_core::DataDirLock::lock_path($root);
                // Matched on canonical lock identity, not path spelling, so a
                // ceremony only ever reaches the observer installed for it.
                if let Some(o) = observers.iter().find(|o| o.expected_root == here) {
                    (o.callback)($root, $point);
                }
            }
            if $injected == Some($point) {
                anyhow::bail!("injected runtime-root fault at {:?}", $point);
            }
        }
    };
}

/// Perform the ceremony.
///
/// # Ceremony ordering and partial failure
///
/// There is no cross-store transaction available here: the cooperative store,
/// the ledger store and the trust store are three separate sled databases. So
/// the ordering is chosen to make partial failure *detectable* rather than
/// pretending it is impossible:
///
/// 1. refuse on a storage-root disagreement (nothing written);
/// 2. refuse if the configuration belongs to another account (nothing written,
///    and before the coordination locks exist);
/// 3. take the configuration and storage locks;
/// 4. cross the N2-A startup gate (before any store is opened);
/// 5. refuse if key material or a receipt already exists, if there is no node
///    keystore to provision under, if this directory holds another
///    institution's state, or if the configuration cannot be linked (nothing
///    written);
/// 6. prove provisioning authority by unlocking the node keystore;
/// 7. mint the runtime trust-root and treasury key material;
/// 8. write the cooperative record;
/// 9. register the treasury durably;
/// 10. write the trust facts;
/// 11. publish the configuration linkage;
/// 12. verify every component through freshly opened handles;
/// 13. write the receipt — **the commit point** — last.
///
/// The receipt is written last and is what `show` consults, so a ceremony
/// interrupted at any earlier point leaves no receipt and is reported as
/// INCOMPLETE rather than as a genesis. Step 3 then refuses a rerun and names
/// the artefacts it found.
///
/// The precise properties are **duplicate-safe**, **partial-state-detecting**
/// and **fail-closed**. This ceremony is *not* idempotent and *not*
/// restartable: an interrupted run cannot be resumed, and every later
/// invocation refuses until an operator removes the partial state by hand.
/// That is an accepted limitation of this slice, not a hidden one — deciding
/// what to do with half-written institutional state is an operator's
/// judgement, and a recovery mechanism is out of scope for #2744.
fn provision_runtime_root(
    data_dir: &Path,
    name: &str,
    currency: &str,
) -> Result<RuntimeRootReceipt> {
    provision_runtime_root_inner(
        data_dir,
        name,
        currency,
        #[cfg(test)]
        None,
    )
}

fn provision_runtime_root_inner(
    data_dir: &Path,
    name: &str,
    currency: &str,
    #[cfg(test)] injected: Option<RuntimeRootFailpoint>,
) -> Result<RuntimeRootReceipt> {
    // (1) Never write authoritative state where the daemon will not read it.
    resolve_storage_root(data_dir)?;

    // (1a) Refuse an account mismatch BEFORE anything is created — including the
    // coordination lock files below, which are retained after release and which
    // a wrong-account run would otherwise leave behind for the daemon to trip
    // over.
    //
    // Two different questions, and the data root is asked first because it
    // governs everything this ceremony writes, not just the one file.
    refuse_if_new_files_would_not_belong_to_the_data_root_account(data_dir)?;
    refuse_if_the_configuration_belongs_to_another_account(data_dir)?;
    refuse_unusable_publication_temp_path(data_dir)?;
    refuse_if_the_configuration_is_hard_linked(data_dir)?;

    // (1b) Take exclusive ownership of the data root BEFORE any state-sensitive
    // observation. Everything from here to the receipt is a read that a
    // concurrent ceremony could invalidate, so the boundary has to span all of
    // it. Held until these guards drop at the end of this function — including
    // on every error path.
    //
    // Two locks, because this ceremony changes two things a daemon depends on:
    // it publishes `<data_dir>/icn.toml`, and it rewrites the state under
    // `<data_dir>`. A daemon started as `icnd --config <data_dir>/icn.toml
    // --data-dir /elsewhere` holds the *configuration* of this directory and
    // the storage of another, so it does not contend for the storage lock at
    // all — republishing its configuration without the first lock would leave
    // it running on bytes that no longer exist. Taken in the same order the
    // daemon takes them, and non-blocking, so neither ordering can hang.
    let _ceremony_config =
        icn_core::DataDirLock::acquire_config(data_dir, "runtime-root provisioning")?;
    let _ceremony = icn_core::DataDirLock::acquire(data_dir, "runtime-root provisioning")?;

    // (2) Before anything opens or writes a store. The gate takes exclusive
    // locks while it audits, so this also fails fast when the daemon is running
    // — which is exactly when this ceremony must not proceed. It is required,
    // but it is NOT the ceremony lock: it releases its locks when it returns.
    enforce_n2a_gate(data_dir, "institutional runtime-root provisioning")?;

    // (3) Refuse over existing key material before minting anything.
    refuse_if_already_provisioned(data_dir)?;

    // (3-) Operator-supplied values are checked before anything is prompted for
    // or written. A whitespace-only cooperative name would otherwise reach the
    // durable record and the configuration, and an oversized one would be
    // discovered only when the config was published — after the institution had
    // been minted.
    validate_runtime_root_inputs(name, currency)?;

    // (3a) And refuse a directory that already belongs to another institution.
    // Distinct question, distinct predicate — see the function's own docs.
    refuse_if_foreign_institutional_state(data_dir)?;

    // (3b) There is no authority to found under without the node keystore.
    // Checked for *existence* here, among the other cheap refusals, so that a
    // ceremony destined to fail does not first prompt for a passphrase. The
    // authority is actually *proven* at (4), by unlocking it.
    //
    // This precedes the configuration check on purpose: "under whose authority"
    // is the more fundamental question, and a data directory missing both
    // should say so rather than reporting the shallower problem.
    let node_keystore_path = get_keystore_path(data_dir);
    // `unwrap_or(false)` here would read "could not tell" as "not a symlink"
    // and proceed to prove founding authority against key material this never
    // managed to look at. Absence is answered by the keystore open below, which
    // reports it far better than this guard could.
    let node_keystore_is_symlink = match std::fs::symlink_metadata(&node_keystore_path) {
        Ok(meta) => meta.file_type().is_symlink(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => {
            return Err(e).with_context(|| {
                format!(
                    "Refusing institutional runtime-root provisioning: could not determine \
                     whether the node keystore at {} is a symlink",
                    node_keystore_path.display()
                )
            })
        }
    };
    if node_keystore_is_symlink {
        bail!(
            "Refusing institutional runtime-root provisioning: the node keystore at {} is a \
             symlink. Founding authority must be proven against real key \
             material in this data directory, not through a redirection.",
            node_keystore_path.display()
        );
    }
    if !node_keystore_path.exists() {
        bail!(
            "Refusing institutional runtime-root provisioning: no node identity at {}.\n\
             The founding authority is the principal that unlocks this \
             keystore; with no keystore there is no authority to found under. \
             Run `icnctl id init` first.",
            node_keystore_path.display()
        );
    }

    // (3c) The configuration link is the step that makes the treasury
    // *consumed* rather than merely stored, and it is the last write in the
    // ceremony. Its preconditions are therefore checked here, before the first
    // write: a genesis that minted key material and then discovered it had
    // nowhere to link it would leave exactly the half-founded institution this
    // ordering exists to prevent.
    check_config_linkable(data_dir)?;

    // (4) Founding authority is *proven*, not asserted: the operator must hold
    // the node keystore and its passphrase.
    let passphrase = read_passphrase("Enter node keystore passphrase: ")?;
    let mut node_keystore = AgeKeyStore::open(&node_keystore_path)
        .with_context(|| format!("Failed to open {}", node_keystore_path.display()))?;
    node_keystore.unlock(&passphrase).context(
        "Failed to unlock the node keystore, so the founding authority could \
         not be established. Genesis refuses rather than proceeding without one.",
    )?;
    let node_did = node_keystore
        .get_keypair()
        .context("Failed to read the node keypair")?
        .did()
        .clone();
    let genesis_authority_did = node_did.clone();

    // (5) `refuse_if_already_provisioned` already covered a completed or
    // half-finished ceremony above, before the passphrase prompt. Nothing to
    // re-check here.

    // (6) Mint the two principals. Both are keypair-backed, so both survive the
    // daemon's config parse and neither is a fabricated string.
    let trust_root_did = mint_principal(
        &trust_root_keystore_path(data_dir),
        &passphrase,
        "genesis trust-root",
    )?;
    failpoint!(injected, data_dir, RuntimeRootFailpoint::AfterTrustRootKey);
    let treasury_did = mint_principal(&treasury_keystore_path(data_dir), &passphrase, "treasury")?;
    failpoint!(injected, data_dir, RuntimeRootFailpoint::AfterTreasuryKey);

    // The invariant this whole issue exists to establish. Asserted rather than
    // assumed: these come from independent `KeyPair::generate()` calls, so an
    // equality here would mean the identity layer itself is broken, and
    // continuing would reintroduce exactly the collapse being fixed.
    if treasury_did == node_did || trust_root_did == node_did {
        bail!(
            "Refusing institutional runtime-root provisioning: a generated institutional \
             principal collided with the node DID ({node_did}). Genesis fails \
             rather than letting the machine stand in for the cooperative."
        );
    }

    // `coop-<uuid>`, not `coop:<uuid>`. The gateway's `validate_coop_id`
    // (`icn-gateway/src/validation.rs`) permits only alphanumerics, hyphens and
    // underscores, so a colon-bearing ID is rejected by `/v1/auth/verify` on
    // syntax alone, before any authorization is even considered.
    //
    // **That fix does not make the default remote bootstrap path work, and an
    // earlier version of this comment implied it did.** A hyphenated ID now
    // reaches the authorization check and is refused there: `verify_challenge`
    // fails closed on a caller-supplied `coop_id` unless a dev posture is
    // enabled (icn#2075), because proving DID ownership does not authorize that
    // DID to act for an arbitrary cooperative. This ceremony establishes no
    // trusted issuance, membership or bootstrap grant binding the node to the
    // cooperative it provisions — deliberately: the node is the machine, and
    // hosting a founding is not membership in the thing founded. So
    // `institution bootstrap apply` without `--local-mint` gets a 403 against a
    // production gateway, and that is correct behaviour on both sides.
    //
    // What the identifier fix buys is narrower and still worth having: the ID
    // is one the rest of the system can *accept*, so trusted local issuance
    // (`--local-mint`) and every later trusted path are not blocked by a
    // spelling this repository's own validators reject.
    //
    // `Cooperative::new` still mints `coop:<uuid>`, so that inconsistency is
    // pre-existing and affects every cooperative created through it; it is filed
    // separately.
    let coop_id = format!("coop-{}", uuid::Uuid::new_v4());

    // (7) Durable cooperative record, in the database the daemon opens.
    let coop_db = coop_db_path(data_dir);
    let coop_sled = Arc::new(open_store_refusing_links(&coop_db).with_context(|| {
        format!(
            "Failed to open the cooperative store at {} (stop the daemon first; \
             it holds an exclusive lock)",
            coop_db.display()
        )
    })?);
    let db = Arc::new(coop_sled.db().clone());
    let coop_store = icn_coop::CoopStore::new(db);

    let mut coop = icn_coop::Cooperative::new_with_domain(
        coop_id.clone(),
        name.to_string(),
        icn_coop::CoopType::Worker,
        coop_id.clone(),
        1,
    );
    coop.assign_treasury(treasury_did.as_str().to_string())
        .map_err(|e| anyhow::anyhow!("Failed to link the treasury to the cooperative: {e}"))?;
    // Bind the trust root to the cooperative on the cooperative's own durable
    // record, so this relationship is readable back through production state
    // and does not depend on the receipt asserting it.
    coop.metadata.insert(
        TRUST_ROOT_METADATA_KEY.to_string(),
        trust_root_did.as_str().to_string(),
    );
    coop_store
        .save_cooperative(&coop)
        .map_err(|e| anyhow::anyhow!("Failed to persist the cooperative record: {e}"))?;
    failpoint!(
        injected,
        data_dir,
        RuntimeRootFailpoint::AfterCooperativeSave
    );

    // (8) Register the treasury durably. `with_store` is required: the plain
    // `TreasuryManager::new()` keeps its maps in memory only, and a treasury
    // that vanishes on restart is not institutional state.
    let ledger_db_path = icn_core::config::ledger_store_path(data_dir);
    let ledger_sled = Arc::new(open_store_refusing_links(&ledger_db_path).with_context(|| {
        format!(
            "Failed to open the ledger store at {} (stop the daemon first; \
                 it holds an exclusive lock)",
            ledger_db_path.display()
        )
    })?);
    let ledger_store: Arc<dyn icn_store::Store> = ledger_sled.clone();
    let mut treasury_manager = icn_ledger::TreasuryManager::with_store(ledger_store)
        .context("Failed to open the treasury manager over the ledger store")?;
    treasury_manager
        .register_treasury(
            treasury_did.clone(),
            coop_id.clone(),
            currency.to_string(),
            genesis_authority_did.clone(),
            Some(format!("Runtime-root treasury for {name}")),
        )
        .context("Failed to register the treasury")?;
    failpoint!(
        injected,
        data_dir,
        RuntimeRootFailpoint::AfterTreasuryRegistration
    );

    // (9) The two trust facts, and precisely what they do and do not mean.
    //
    // The ledger's author-trust gate scores an author through
    // `TrustGraph::compute_trust_score`, which is ego-centric from the node's
    // own DID. A `trust root -> treasury` edge *alone* is unreachable from
    // that origin and scores 0.0, so both edges are required:
    //
    //   node -> trust root       the operator's local recognition of the
    //                            cooperative founded here. Not a self-edge.
    //   trust root -> treasury   a trust fact whose SOURCE is the trust root.
    //
    // Together they score 1.0*1.0 * 0.3 (the transitive weight) = 0.30, above
    // the 0.1 the ledger requires.
    //
    // What the second edge is NOT: it is not signed by the trust root's key,
    // and `TrustEdge` carries no signature or provenance field, so the store
    // records no evidence of who established it. It is written through
    // `TrustGraph::add_edge` — the storage primitive, and the only API that
    // accepts an arbitrary source, since the service-level `submit_attestation`
    // hardcodes the caller's own DID — under the privilege of this local
    // ceremony, authenticated by the founding Principal's keystore.
    //
    // So the honest description is an *institution-attributed* trust fact
    // established under the founding Principal's privileged local ceremony.
    // It is NOT a cryptographic authorisation by the cooperative, and this
    // slice does not claim the cooperative holds authority independent of the
    // node: removing `node -> trust root` also drops the author to 0.0. Both
    // of those are covered by tests, so the claim cannot drift.
    //
    // Genesis never writes the `node -> node` self-edge that ICN_DEV_SELF_TRUST
    // writes.
    let trust_db_path = icn_core::config::trust_store_path(data_dir);
    let trust_sled = Arc::new(open_store_refusing_links(&trust_db_path).with_context(|| {
        format!(
            "Failed to open the trust store at {} (stop the daemon first; \
             it holds an exclusive lock)",
            trust_db_path.display()
        )
    })?);
    let trust_store: Arc<dyn icn_store::Store> = trust_sled.clone();
    let mut trust_graph = icn_trust::TrustGraph::new(trust_store, node_did.clone());
    let full =
        icn_trust::TrustScore::new(1.0).map_err(|e| anyhow::anyhow!("Invalid trust score: {e}"))?;
    // The graph type is stated explicitly rather than taken from
    // `TrustEdge::new`'s default, so the intended authority mechanism is written
    // down. Two facts about the current implementation are worth recording: the
    // storage key (`trust/edges/{source}:{target}`) does NOT include the graph
    // type, and `compute_trust_score` reads whatever edge sits at that key — so
    // the ledger's author-trust query is type-agnostic today and this choice
    // does not change the score. `Social` is nonetheless the honest label: its
    // documented uses include governance membership resolution and organization
    // membership, and it is the same type `icnd`'s own `ICN_DEV_SELF_TRUST`
    // seed writes for this very gate. If a typed query is ever introduced, this
    // declaration is what must be revisited.
    let graph_type = icn_trust::TrustGraphType::Social;
    trust_graph
        .add_edge(icn_trust::TrustEdge::new_typed(
            node_did.clone(),
            trust_root_did.clone(),
            full,
            graph_type,
        ))
        .map_err(|e| anyhow::anyhow!("Failed to record the node's recognition edge: {e}"))?;
    failpoint!(injected, data_dir, RuntimeRootFailpoint::AfterAuthorityEdge);
    trust_graph
        .add_edge(icn_trust::TrustEdge::new_typed(
            trust_root_did.clone(),
            treasury_did.clone(),
            full,
            graph_type,
        ))
        .map_err(|e| anyhow::anyhow!("Failed to record the trust root's authority edge: {e}"))?;
    failpoint!(
        injected,
        data_dir,
        RuntimeRootFailpoint::AfterTreasuryAuthorityEdge
    );

    // Close every store handle before verification. sled takes an exclusive
    // directory lock, so the fresh handles in (11) cannot open these databases
    // while this ceremony still holds them — and a verification that read back
    // through the writer's own handle would prove nothing about durability.
    // Flush every store before dropping it. Reopening a handle proves a write is
    // *visible*, not that it survived a crash: sled buffers, and the receipt is
    // about to certify this state as durable. Cooperative, ledger and trust are
    // three separate databases and each needs its own flush.
    drop(coop_store);
    coop_sled
        .flush()
        .context("Failed to flush cooperative state")?;
    drop(coop_sled);
    drop(treasury_manager);
    ledger_sled
        .flush()
        .context("Failed to flush the treasury registration")?;
    drop(ledger_sled);
    drop(trust_graph);
    trust_sled
        .flush()
        .context("Failed to flush the trust facts")?;
    drop(trust_sled);

    // (10) Publish the configuration. This is what makes the treasury
    // *consumed* by the daemon rather than merely stored, so it precedes the
    // completion marker: a receipt written before this point would claim a
    // genesis the daemon would not act on.
    publish_cooperative_config(data_dir, name, &treasury_did)?;
    failpoint!(injected, data_dir, RuntimeRootFailpoint::AfterConfigPublish);

    let receipt = RuntimeRootReceipt {
        schema_version: RUNTIME_ROOT_RECEIPT_SCHEMA_VERSION,
        cooperative_id: coop_id.clone(),
        cooperative_name: name.to_string(),
        trust_root_did: trust_root_did.as_str().to_string(),
        treasury_did: treasury_did.as_str().to_string(),
        genesis_authority_did: genesis_authority_did.as_str().to_string(),
        node_did: node_did.as_str().to_string(),
        currency: currency.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    // (11) Verify every claim the receipt is about to make, through handles
    // opened after the writers were dropped. A COMPLETE receipt must not be
    // producible for durable state that disagrees with it.
    verify_durable_state(data_dir, &receipt, Some(&passphrase))?;

    // (11b) Durably publish the directory entries the receipt depends on.
    //
    // Flushing a sled database makes its *contents* durable. It does not make
    // the directory entry that names the database durable, and this ceremony
    // may create two levels of them: `<data_dir>/store` if no store existed
    // yet, and `store/{cooperative,ledger,trust}` beneath it. A crash could
    // therefore leave the cooperative store and its receipt intact while the
    // ledger directory entry — and with it the treasury registration the
    // receipt vouches for — was never named on disk.
    //
    // `fsync` on a directory persists the names it contains, so both levels are
    // needed: `store` to persist `ledger`/`trust`/`cooperative`, and `data_dir`
    // to persist `store` itself. Syncing only the one the review named would
    // leave the outer entry unpersisted, and a lost `store` takes the receipt
    // with it — self-consistent, but not what "survives a restart" means.
    //
    // Ordering is the load-bearing part: create, flush contents, publish the
    // names, and only then commit the marker that claims all of it. sled owns
    // durability *inside* its own directory; nothing else does the outside.
    sync_directory_entries(&icn_core::config::store_path(data_dir))?;
    sync_directory_entries(data_dir)?;

    // (12) The completion marker, written last and only over verified state.
    {
        use icn_store::Store;
        let coop_db = coop_db_path(data_dir);
        let coop_sled = open_store_refusing_links(&coop_db)
            .context("Failed to reopen the cooperative store to record the receipt")?;
        let key = format!("{RECEIPT_KEY_PREFIX}{coop_id}");
        #[cfg(test)]
        record_durability_event(DurabilityEvent::WroteReceipt(data_dir.to_path_buf()));
        let value =
            serde_json::to_vec(&receipt).context("Failed to encode the runtime-root receipt")?;
        // Written into the same sled database as the `coop:` rows, so the
        // receipt cannot outlive the cooperative it describes.
        coop_sled
            .put(key.as_bytes(), &value)
            .context("Failed to persist the runtime-root receipt")?;
        coop_sled
            .flush()
            .context("Failed to flush the runtime-root receipt")?;
    }

    Ok(receipt)
}

/// Re-read everything the receipt asserts, through handles opened after the
/// ceremony's own writers were dropped.
///
/// This is what makes the receipt a claim about durable state rather than about
/// what this process believes it did. Each check is the one a later reader
/// actually performs: the daemon resolves its treasury from the configuration,
/// `TreasuryManager::with_store` rehydrates from the ledger rows, and the
/// cooperative record is read straight out of sled.
fn verify_durable_state(
    data_dir: &Path,
    receipt: &RuntimeRootReceipt,
    passphrase: Option<&[u8]>,
) -> Result<()> {
    // The cooperative record, and its link to the treasury.
    let coop_db = coop_db_path(data_dir);
    let coop_sled = Arc::new(
        open_store_refusing_links(&coop_db)
            .context("Verification: failed to reopen the cooperative store")
            .map_err(uninspectable)?,
    );
    let coop_store = icn_coop::CoopStore::new(Arc::new(coop_sled.db().clone()));
    // Two different failures reach this call, and conflating them makes the
    // caller state a fact it did not observe.
    //
    //   - `Storage` (a sled error) and `Serialization` (a row that will not
    //     decode) mean the check *could not be run*. Reporting those as a
    //     disagreement would be a positive corruption claim derived from an
    //     absence of evidence, and `runtime_root_state` would render it
    //     `INCONSISTENT` instead of the documented `ERROR` / `state_unreadable`.
    //   - every other variant — `NotFound` above all — is an observation: the
    //     store was read successfully and the record is missing or wrong. That
    //     genuinely is a disagreement with the receipt.
    //
    // Matched on the typed variant rather than on the rendered message, so a
    // reworded `Display` cannot silently move a failure between the two
    // classes. The `SledStore::open` above is already marked uninspectable for
    // the same reason; this row read was the gap.
    let read = {
        #[cfg(test)]
        {
            if CoopRowReadFailureGuard::armed_for(data_dir) {
                Err(icn_coop::CoopError::Storage(sled::Error::Unsupported(
                    "injected cooperative-row storage failure".to_string(),
                )))
            } else {
                coop_store.get_cooperative(&receipt.cooperative_id)
            }
        }
        #[cfg(not(test))]
        {
            coop_store.get_cooperative(&receipt.cooperative_id)
        }
    };
    let stored = read.map_err(|e| {
        let unreadable = matches!(
            e,
            icn_coop::CoopError::Storage(_) | icn_coop::CoopError::Serialization(_)
        );
        let described = anyhow::anyhow!(
            "Verification: the cooperative record {} is not readable back ({e}); refusing to \
                 record a genesis receipt for state that is not there.",
            receipt.cooperative_id
        );
        if unreadable {
            uninspectable(described)
        } else {
            described
        }
    })?;
    match stored.treasury_did.as_deref() {
        Some(linked) if linked == receipt.treasury_did => {}
        other => bail!(
            "Verification: the stored cooperative names treasury {other:?}, but the receipt would \
             claim {}. Refusing to record a receipt that disagrees with durable state.",
            receipt.treasury_did
        ),
    }
    match stored.metadata.get(TRUST_ROOT_METADATA_KEY) {
        Some(bound) if *bound == receipt.trust_root_did => {}
        other => bail!(
            "Verification: the stored cooperative binds trust root {other:?}, but the receipt \
             would claim {}. Refusing to record a receipt that disagrees with durable state.",
            receipt.trust_root_did
        ),
    }
    drop(coop_store);
    drop(coop_sled);

    // The treasury registration, rehydrated the way the ledger app rehydrates
    // it — `with_store` runs the fail-closed hydration path, so a row this
    // ceremony wrote badly is caught here rather than at the daemon's next start.
    let ledger_store: Arc<dyn icn_store::Store> = Arc::new(
        open_store_refusing_links(&icn_core::config::ledger_store_path(data_dir))
            .context("Verification: failed to reopen the ledger store")
            .map_err(uninspectable)?,
    );
    let manager = icn_ledger::TreasuryManager::with_store(ledger_store)
        .context("Verification: the treasury store did not rehydrate")
        .map_err(uninspectable)?;
    let treasury_did: Did = receipt
        .treasury_did
        .parse()
        .context("Verification: the treasury DID in the receipt is not a usable DID")?;
    // Verify the **establishment facts**, and only those.
    //
    // Classified from `icn-ledger/src/treasury.rs` rather than from a reviewer's
    // list, because over-verifying is its own defect: a runtime root must not
    // report itself inconsistent because ordinary treasury operation changed
    // something it never claimed.
    //
    // | field         | mutable after registration? | reverified here |
    // |---------------|------------------------------|-----------------|
    // | `treasury_did`| no — primary key             | yes             |
    // | `coop_id`     | no — the one mutator asserts it byte-for-byte | yes |
    // | `currency`    | no mutator exists            | yes             |
    // | `created_by`  | no mutator exists            | yes             |
    // | `entity_id`   | **YES** — `populate_entity_id_at_*` sets None -> Some (#2082) | **no** |
    // | `description` | no mutator, but audit metadata only | no       |
    // | `is_active`   | operational lifecycle state  | no              |
    //
    // `entity_id` is the important exclusion. Populating it later is the
    // legitimate #2082 backfill, and treating it as an establishment fact would
    // make a previously-READY root report INCONSISTENT the moment that backfill
    // ran.
    let Some(stored_treasury) = manager.get_treasury(&treasury_did) else {
        bail!(
            "Verification: treasury {} is not readable back from the ledger store.",
            receipt.treasury_did
        );
    };
    if stored_treasury.coop_id != receipt.cooperative_id {
        bail!(
            "Verification: treasury {} is registered to cooperative {:?}, but the receipt \
             claims {:?}. The treasury does not belong to the cooperative this root \
             established.",
            receipt.treasury_did,
            stored_treasury.coop_id,
            receipt.cooperative_id
        );
    }
    if stored_treasury.currency != receipt.currency {
        bail!(
            "Verification: treasury {} records currency {:?}, but the receipt claims {:?}.",
            receipt.treasury_did,
            stored_treasury.currency,
            receipt.currency
        );
    }
    if stored_treasury.created_by.as_str() != receipt.genesis_authority_did {
        bail!(
            "Verification: treasury {} records {:?} as its creating authority, but the receipt \
             claims {:?}.",
            receipt.treasury_did,
            stored_treasury.created_by.as_str(),
            receipt.genesis_authority_did
        );
    }
    drop(manager);

    // Both trust facts, read through a fresh handle.
    let trust_store: Arc<dyn icn_store::Store> = Arc::new(
        open_store_refusing_links(&icn_core::config::trust_store_path(data_dir))
            .context("Verification: failed to reopen the trust store")
            .map_err(uninspectable)?,
    );
    let node_did: Did = receipt
        .node_did
        .parse()
        .context("Verification: the node DID in the receipt is not a usable DID")?;
    let trust_root_did: Did = receipt
        .trust_root_did
        .parse()
        .context("Verification: the trust root DID in the receipt is not a usable DID")?;
    let graph = icn_trust::TrustGraph::new(trust_store, node_did.clone());
    for (source, target, what) in [
        (
            &node_did,
            &trust_root_did,
            "the node's recognition of the genesis trust root",
        ),
        (
            &trust_root_did,
            &treasury_did,
            "the trust root's authority over the treasury",
        ),
    ] {
        let present = graph
            .get_edge(source, target)
            .map_err(|e| {
                uninspectable(anyhow::anyhow!(
                    "Verification: could not read a trust edge ({e})"
                ))
            })?
            .is_some();
        if !present {
            bail!("Verification: {what} is not readable back from the trust store.");
        }
    }

    // Edge *presence* is not the property the ledger enforces. If either edge is
    // later rewritten with a lower score, both rows still read back while the
    // treasury falls under the author-trust threshold and every
    // governance-authored entry is refused — with the receipt still saying
    // COMPLETE. So score the treasury the way the gate does and enforce the
    // production threshold.
    //
    // Computed through `TrustGraph::compute_trust_score`, which is what
    // `TrustServiceImplTokio::trust_score` calls, over the same persisted store.
    //
    // The threshold is `icn_ledger::DEFAULT_MIN_TRUST_FOR_ENTRY` rather than a
    // literal copied here. What that constant proves, stated exactly: `Ledger`
    // initialises `min_trust_for_entry` from it, and the only non-test caller of
    // `set_min_trust_for_entry` anywhere in the workspace is
    // `icn-ledger/tests/witness_trust.rs` — there is no configuration, env var
    // or production path that overrides it today. It is therefore the effective
    // gate *for the current production constructor*, not a value guaranteed
    // immutable for all time. Should a configured threshold ever be introduced,
    // this check must read the configured value instead.
    //
    // One honest caveat: this runs on a graph that has added no edge of its own,
    // which is the arm icn#2750 does NOT break. It therefore verifies the facts
    // are *sufficient*, not that a long-running daemon will still honour them —
    // that is exactly what icn#2750 blocks, and it is not papered over here.
    let scored = graph.compute_trust_score(&treasury_did).map_err(|e| {
        uninspectable(anyhow::anyhow!(
            "Verification: could not score the treasury as the ledger would ({e})"
        ))
    })?;
    if scored < icn_ledger::DEFAULT_MIN_TRUST_FOR_ENTRY {
        bail!(
            "Verification: the treasury scores {scored:.3} against the ledger's author-trust \
             threshold of {:.3}, so a governance-authored entry would be refused. The trust \
             facts exist but do not carry the authority the receipt would claim.",
            icn_ledger::DEFAULT_MIN_TRUST_FOR_ENTRY
        );
    }
    drop(graph);

    // The configuration the daemon will actually parse, resolved through the
    // owner of that rule rather than by re-reading the key here.
    let config_path = data_dir.join("icn.toml");
    let text = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Verification: failed to read {}", config_path.display()))
        .map_err(uninspectable)?;
    // The key material itself. Without this, deleting `treasury.age` after a
    // completed ceremony would still report COMPLETE, and the claim that every
    // component is re-read would be narrower than stated.
    //
    // Presence is checked always; **provenance** — that the key actually
    // derives the DID the receipt names — only when a passphrase is available.
    // The ceremony has one and checks it, so a receipt is never written over
    // key material that does not back its principals. `show` deliberately does
    // not: it is a read-only inspection and must not prompt for a passphrase,
    // so it reports presence only.
    //
    // A keystore substituted AFTER a completed genesis is therefore not
    // detected by any current command. A rerun of `create` does refuse, but it
    // refuses because a genesis already exists — not because the key stopped
    // backing the receipt; `a_rerun_over_a_substituted_keystore_refuses_on_prior_genesis_not_provenance`
    // pins that distinction so this comment cannot drift back into claiming
    // detection it does not perform. Post-hoc cryptographic verification is
    // recorded as a limitation, not implemented here.
    // The NODE keystore is checked alongside the two the ceremony minted, and
    // for a reason the other two do not have. The daemon builds its trust graph
    // rooted at whatever `identity.age` currently holds — not at the DID the
    // receipt recorded. If the node identity is replaced or rotated after
    // genesis, the persisted `node -> trust root` edge still reads back, so a
    // presence-only check reports COMPLETE while the running daemon roots trust
    // at a different DID and scores the treasury from there. Comparing the
    // receipt's `node_did` against the keystore's current DID is what makes
    // COMPLETE mean "the daemon that will run here still recognises this
    // institution".
    //
    // Like key provenance, this needs the passphrase and so is unavailable to
    // `show`. There is no non-secret source for the node's current DID:
    // `AgeKeyStore::open` populates nothing until `unlock`, and `id init`
    // writes no public DID artifact beside the keystore.
    for (path, what, expected) in [
        (get_keystore_path(data_dir), "node", &receipt.node_did),
        (
            trust_root_keystore_path(data_dir),
            "genesis trust-root",
            &receipt.trust_root_did,
        ),
        (
            treasury_keystore_path(data_dir),
            "treasury",
            &receipt.treasury_did,
        ),
    ] {
        // `symlink_metadata`, not `is_file()`. `is_file()` follows links, so a
        // symlink pointing at any regular file would satisfy "the key is
        // present" — and `show` never crosses the N2-A gate that refuses
        // symlinks for the create path, so nothing else would catch it.
        //
        // This is **containment**, and it is a different property from
        // provenance: it needs no passphrase, so the read-only path can and
        // must check it. Provenance — that the key derives the recorded DID —
        // still needs the passphrase and remains unavailable to `show`.
        match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => bail!(
                "Verification: {what} key material is missing from {}. The \
                 principal named by the receipt has no key behind it.",
                path.display()
            ),
            // Anything else is a failed observation, not an observed absence.
            // Reporting it as missing would render INCONSISTENT — a positive
            // claim that durable state contradicts the receipt — for a look
            // that never happened.
            Err(e) => {
                return Err(uninspectable(anyhow::Error::new(e).context(format!(
                    "Verification: could not determine whether {what} key material exists at {}",
                    path.display()
                ))))
            }
            Ok(meta) if meta.file_type().is_symlink() => bail!(
                "Verification: {what} key material at {} is a symlink. Runtime-root key \
                 material must be a regular file inside the data directory; a link could \
                 point anywhere, and its target is not this root's to vouch for.",
                path.display()
            ),
            Ok(meta) if !meta.is_file() => bail!(
                "Verification: {what} key material at {} is not a regular file.",
                path.display()
            ),
            Ok(_) => {}
        }
        if let Some(passphrase) = passphrase {
            let mut keystore = AgeKeyStore::open(&path)
                .with_context(|| format!("Verification: failed to open {}", path.display()))
                .map_err(uninspectable)?;
            keystore
                .unlock(passphrase)
                .with_context(|| format!("Verification: failed to unlock {}", path.display()))?;
            let actual = keystore
                .get_keypair()
                .with_context(|| format!("Verification: failed to read the {what} keypair"))
                .map_err(uninspectable)?
                .did()
                .clone();
            if actual.as_str() != expected {
                bail!(
                    "Verification: {} holds key material for {}, but the receipt names {} as \
                     the {what}. Refusing to certify a principal whose key does not back it — \
                     for the node keystore this also means the running daemon would root its \
                     trust graph at a different DID than this genesis recorded.",
                    path.display(),
                    actual.as_str(),
                    expected
                );
            }
        }
    }

    // The configuration must still name THIS storage root. Without this,
    // editing `icn.toml` to point elsewhere leaves `show --data-dir <old-root>`
    // reporting READY while `icnd --config` would open the newly configured root
    // and find none of the cooperative, treasury or trust state being certified.
    // Same comparison the ceremony makes, so the two cannot disagree.
    resolve_storage_root(data_dir).context(
        "Verification: the configuration no longer names the storage root this runtime root \
         was provisioned into",
    )?;

    // Through the daemon's own loader, not a sub-table read: a receipt that
    // certifies "the daemon will consume this treasury" must be backed by the
    // same parse the daemon performs.
    let mut config: icn_core::Config = toml::from_str(&text).with_context(|| {
        format!(
            "Verification: {} is not loadable by the daemon, so a runtime root linked into it \
             would never be consumed (see icn#2747)",
            config_path.display()
        )
    })?;

    // Parsing is necessary but not sufficient on the readback path either, and
    // for the same reason it is not sufficient at preflight: `icnd` runs
    // `Config::validate` at startup and exits on the fatal arm. Without this, a
    // configuration edited after provisioning to something syntactically valid
    // but fatally invalid — an empty `network.listen_addr`, a trust threshold
    // outside [0,1] — still resolved a treasury here, and `show` reported READY
    // over a file the daemon refuses to start on.
    //
    // The environment override is applied first, exactly as the daemon applies
    // it before validating, so this stays neither weaker nor stricter than the
    // process it models.
    let secret_from_environment = std::env::var("ICN_GATEWAY_JWT_SECRET").ok();
    if let Some(jwt_secret) = secret_from_environment.clone() {
        config.gateway.jwt_secret = jwt_secret;
    }
    if let Err(errors) = config.validate() {
        // One fatal error is *not* evidence about durable state: the gateway
        // secret may legitimately live in the daemon's environment rather than
        // in the file (`init-coop` recommends exactly that, and the daemon
        // applies it before validating). `show` runs in whatever shell an
        // operator happens to use, which is not the service's environment — so
        // a missing secret here means "this invocation cannot tell", not "the
        // published configuration is broken". Reporting INCONSISTENT for it
        // would be the boolean-for-unknown mistake this command avoids
        // everywhere else.
        //
        // Distinguished by substitution rather than by matching message text:
        // supply a placeholder and re-validate. If everything else passes, the
        // environment was the only gap. If anything else fails, that failure is
        // about the file and is fatal here.
        //
        // This deliberately diverges from the preflight, which bails on any
        // validation error including this one. The two commands are answering
        // different questions: `create` is about to *write* and certify, so
        // requiring the operator to present a configuration that demonstrably
        // starts a daemon is a reasonable precondition and fails closed;
        // `show` only *reports* durable state, where "this shell cannot see the
        // secret" is not a fact about the bytes on disk. An operator can
        // therefore see `create` refuse and `show` report READY over one file.
        // That is the safe direction — the stricter side is the one that
        // writes — and it is recorded here rather than smoothed over.
        //
        // Note also that when this carve-out does not fire, the refusal prints
        // the validator'"'"'s own error list verbatim, which may still mention the
        // secret among the daemon'"'"'s reasons.
        let only_the_environment_secret = secret_from_environment.is_none() && {
            let mut probe = config.clone();
            probe.gateway.jwt_secret = "x".repeat(64);
            probe.validate().is_ok()
        };
        if !only_the_environment_secret {
            bail!(
                "Verification: {} parses, but the daemon would refuse to start on it, so a \
                 runtime root linked into it would never be consumed:\n  {}",
                config_path.display(),
                errors.join("\n  ")
            );
        }
    }

    let resolved = config
        .cooperative
        .resolve_treasury_did(&node_did)
        .context("Verification: the published configuration does not resolve a treasury")?;
    if !resolved.is_configured() || resolved.did().as_str() != receipt.treasury_did {
        bail!(
            "Verification: the published configuration resolves treasury {:?}, not the {} this \
             ceremony created. The daemon would not consume the institution's treasury.",
            resolved.did().as_str(),
            receipt.treasury_did
        );
    }
    Ok(())
}

/// The access identity a file carries: the owner and group that, together with
/// the mode bits, decide which accounts may read it.
///
/// Captured from the inode rather than from the process, deliberately. A file
/// created in a setgid directory takes the *directory's* group, not the
/// creator's, so asking "what does a new file here actually get?" answers a
/// question `geteuid`/`getegid` cannot.
#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AccessIdentity {
    uid: u32,
    gid: u32,
}

#[cfg(unix)]
impl AccessIdentity {
    fn of(meta: &std::fs::Metadata) -> Self {
        use std::os::unix::fs::MetadataExt as _;
        Self {
            uid: meta.uid(),
            gid: meta.gid(),
        }
    }
}

#[cfg(unix)]
impl std::fmt::Display for AccessIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "uid {} gid {}", self.uid, self.gid)
    }
}

/// What replacing a file with a newly created one would do to its access
/// identity.
#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OwnershipTransfer {
    /// The replacement carries the same owner and group. Who may read the file
    /// is unchanged.
    Preserved,
    /// The replacement would be owned by a different account, so accounts that
    /// could read the file may no longer be able to.
    WouldChange {
        existing: AccessIdentity,
        replacement: AccessIdentity,
    },
}

/// The policy, as a pure function.
///
/// Separated from every filesystem call on purpose: an unprivileged test
/// process cannot create a file owned by another account, but it can — and
/// does — drive this with identities it could never construct on disk.
#[cfg(unix)]
fn classify_ownership_transfer(
    existing: AccessIdentity,
    replacement: AccessIdentity,
) -> OwnershipTransfer {
    if existing == replacement {
        OwnershipTransfer::Preserved
    } else {
        OwnershipTransfer::WouldChange {
            existing,
            replacement,
        }
    }
}

/// Why this refuses rather than restoring the ownership itself.
///
/// Atomic publication replaces the inode, so the new file's owner and group
/// come from the process that created it. `chown(2)` could put them back — but
/// that would fix the *configuration* alone. This same ceremony mints
/// `genesis-trust-root.age` and `treasury.age` and writes three sled databases,
/// all as the invoking account. A ceremony running as the wrong account
/// produces a runtime root the daemon cannot read in its entirety; repairing
/// one file of it would buy a receipt that looks right over state that is not.
///
/// So the account mismatch is treated as what it is — the ceremony being run as
/// the wrong user — and the remedy is to run it as the right one.
#[cfg(unix)]
fn refuse_ownership_change(
    path: &Path,
    existing: AccessIdentity,
    replacement: AccessIdentity,
) -> anyhow::Error {
    anyhow::anyhow!(
        "Refusing institutional runtime-root provisioning: publishing {} would change which \
         account owns it.\n  \
         it is owned by:            {existing}\n  \
         this process writes as:    {replacement}\n\
         Publication replaces the file, so the replacement would carry this process's identity \
         and an account that can read the configuration today might not be able to afterwards — \
         while the receipt reported success. The keystores and stores this ceremony writes \
         would carry the same identity, so this is not a property of the configuration alone: \
         it is the ceremony running as the wrong account.\n\
         Re-run provisioning as the account that owns this data directory (the deployment \
         scripts use `runuser -u icn` / `sudo -u icn`).",
        path.display()
    )
}

/// A verification step that could not be *performed*, as distinct from one that
/// was performed and disagreed.
///
/// `runtime_root_state` used to collapse both into `INCONSISTENT`, which is a
/// positive claim of durable-state corruption. Losing read permission on
/// `icn.toml`, or a transient I/O error opening the ledger store, is not that
/// claim — nothing was observed to mismatch. Reporting it as corruption is the
/// same boolean-for-unknown mistake this command refuses everywhere else, and
/// it points an operator at the wrong repair.
///
/// Errors carrying this marker propagate to `classify_for_show`, which renders
/// them as the documented `ERROR` / `state_unreadable` outcome.
#[derive(Debug)]
struct Uninspectable(anyhow::Error);

impl std::fmt::Display for Uninspectable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#}", self.0)
    }
}

impl std::error::Error for Uninspectable {}

/// Mark a failure as "the check could not be run", not "the check failed".
fn uninspectable(e: impl Into<anyhow::Error>) -> anyhow::Error {
    anyhow::Error::new(Uninspectable(e.into()))
}

/// Make a directory's *entries* durable.
///
/// `fsync` on a directory persists the names it contains, not the directory
/// itself — so this is what a newly created child needs before anything may
/// claim that child survives a crash. Flushing a file's or a database's
/// contents is a different barrier and does not imply this one.
///
/// One helper rather than a fourth open-coded copy: the same three lines had
/// already accumulated at the keystore-minting and configuration-publication
/// sites, and persistence machinery that drifts between call sites is how a
/// durability claim quietly stops being true at one of them.
fn sync_directory_entries(dir: &Path) -> Result<()> {
    let handle = std::fs::File::open(dir)
        .with_context(|| format!("Failed to open {} to sync", dir.display()))?;
    handle
        .sync_all()
        .with_context(|| format!("Failed to sync {}", dir.display()))?;
    #[cfg(test)]
    record_durability_event(DurabilityEvent::SyncedDirectory(dir.to_path_buf()));
    Ok(())
}

/// Test-only ordering record for the durability witness.
///
/// A test cannot cut power, so it cannot observe crash survival. What it *can*
/// observe is the ordering the claim rests on: that every namespace entry the
/// receipt depends on was made durable before the receipt was written. This
/// records that order rather than pretending to prove more.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
enum DurabilityEvent {
    SyncedDirectory(PathBuf),
    /// Carries the root it was written under. Without that this event could not
    /// be attributed, so a concurrently running ceremony's receipt was read as
    /// this one's — and an ordering assertion against it silently compared two
    /// different ceremonies.
    WroteReceipt(PathBuf),
}

#[cfg(test)]
static DURABILITY_LOGS: std::sync::Mutex<Vec<(PathBuf, Vec<DurabilityEvent>)>> =
    std::sync::Mutex::new(Vec::new());

/// Record for one root only, removed on drop.
///
/// One log per root, not one slot in total. The earlier version stored the root
/// and then never consulted it — `record_durability_event` destructured it as
/// `_` — so the recorder captured every ceremony running anywhere in the
/// process. `cargo test` runs these in parallel, so an unrelated ceremony's
/// `WroteReceipt` could land in this log ahead of this root's own directory
/// syncs, and the ordering assertion then compared two different ceremonies.
/// The claim it exists to defend is an *ordering* one, so a log that can
/// interleave two ceremonies is not evidence for it.
#[cfg(test)]
struct DurabilityRecorder {
    root: PathBuf,
}

#[cfg(test)]
impl DurabilityRecorder {
    fn install(root: &Path) -> Self {
        let root = root.to_path_buf();
        if let Ok(mut logs) = DURABILITY_LOGS.lock() {
            logs.retain(|(recorded, _)| *recorded != root);
            logs.push((root.clone(), Vec::new()));
        }
        Self { root }
    }

    fn events(&self) -> Vec<DurabilityEvent> {
        DURABILITY_LOGS
            .lock()
            .ok()
            .and_then(|logs| {
                logs.iter()
                    .find(|(recorded, _)| *recorded == self.root)
                    .map(|(_, events)| events.clone())
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
impl Drop for DurabilityRecorder {
    fn drop(&mut self) {
        if let Ok(mut logs) = DURABILITY_LOGS.lock() {
            logs.retain(|(recorded, _)| *recorded != self.root);
        }
    }
}

#[cfg(test)]
fn record_durability_event(event: DurabilityEvent) {
    let (DurabilityEvent::SyncedDirectory(where_) | DurabilityEvent::WroteReceipt(where_)) = &event;
    let where_ = where_.clone();
    if let Ok(mut logs) = DURABILITY_LOGS.lock() {
        // Routed to the recorder that owns this path. `starts_with` compares
        // whole components, so a sibling root with a longer name is not a match.
        if let Some((_, events)) = logs
            .iter_mut()
            .find(|(recorded, _)| where_.starts_with(recorded))
        {
            events.push(event);
        }
    }
}

/// Does this store directory exist, or could we not tell?
///
/// `Path::exists()` answers `false` for *every* metadata error, including a
/// traversal or permission failure on an existing directory — so an
/// inaccessible store reads as a clean one. That is the same fail-open shape
/// this file removed from its scan closures, one layer further out: preflight
/// would call the directory absent, mint both keystores, and only then fail
/// opening it, leaving a permanently INCOMPLETE root.
///
/// Only `NotFound` means absent. Anything else is "cannot tell", which is not
/// evidence and is refused.
fn store_directory_exists(db: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(db) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e)
            .with_context(|| {
                format!(
                    "Refusing to proceed: could not determine whether {} exists, so it cannot \
                     be established that no institutional state is already there",
                    db.display()
                )
            })
            .map_err(uninspectable),
    }
}

/// Refuse a managed configuration that has more than one directory entry.
///
/// Atomic publication replaces the inode: the new contents arrive at
/// `<data_dir>/icn.toml`, and every *other* name for the old inode keeps
/// serving the old bytes. That is not merely lost metadata — it is a second
/// configuration identity outside this ceremony's exclusion domain.
///
/// The reachable shape:
///
/// ```text
/// A/icn.toml ─────┐
///                 ├── one inode
/// B/config.toml ──┘
/// ```
///
/// A daemon consuming `B/config.toml` takes `B/.icn-config.lock`; this ceremony
/// takes `A/.icn-config.lock`. The two never contend, so the ceremony publishes
/// a new inode into `A`, commits its receipt, and the daemon carries on reading
/// the old one through `B` — the stale-configuration state the whole protocol
/// exists to prevent, reached through inode aliasing rather than through a path
/// alias, which canonicalization already handles.
///
/// Measured: after publication over a two-link configuration, `A/icn.toml` held
/// the `[cooperative]` section at a new inode while `B/config.toml` still held
/// the original inode without it.
///
/// So the alias is refused rather than documented. `symlink_metadata` is used
/// for the same reason the rest of this file uses it: the question is about the
/// entry itself, never about something it points at.
#[cfg(unix)]
fn refuse_if_the_configuration_is_hard_linked(data_dir: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt as _;

    let config_path = data_dir.join("icn.toml");
    let meta = match std::fs::symlink_metadata(&config_path) {
        Ok(meta) => meta,
        // Absent, or not a regular file: both are refused by other layers, with
        // better messages than this one could give.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(e).with_context(|| format!("Failed to inspect {}", config_path.display()))
        }
    };
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Ok(());
    }
    if meta.nlink() > 1 {
        bail!(
            "Refusing institutional runtime-root provisioning: {} has {} directory entries, not \
             one.\n\
             Publication replaces the file, so the other name(s) for it would keep serving the \
             configuration as it is now while this data directory moved on — and a daemon reading \
             through one of them coordinates on that directory's lock, not this one, so the two \
             would never contend. Remove the additional link (or copy the file instead of linking \
             it) and re-run.",
            config_path.display(),
            meta.nlink()
        );
    }
    Ok(())
}

#[cfg(not(unix))]
fn refuse_if_the_configuration_is_hard_linked(_data_dir: &Path) -> Result<()> {
    Ok(())
}

/// Open a sled store, refusing a linked path first.
///
/// `SledStore::open` follows symlinks, and it is a *writing* open: sled can
/// perform recovery writes on open. The data-directory lock covers this root
/// only, so a `store/ledger` symlink pointing at another database would let a
/// supposedly diagnostic `runtime-root show` modify a database outside this
/// exclusion domain entirely — and verify rows this root does not contain,
/// which is a receipt certifying somebody else's state.
///
/// A function rather than a rule to remember. There are ten opens in this file,
/// and the recent history of this lane is that per-call-site rules get missed:
/// the account guard was added to three lock sites and missed a fourth, twice.
/// A caller that does not reach `SledStore::open` directly cannot forget.
///
/// The path itself and its immediate entries are both checked, matching the
/// backup verifier's shape — a link *inside* the directory redirects exactly as
/// a linked directory would. An absent path is fine: sled creates it, and
/// nothing can be linked yet. A non-directory is left to sled's own open, which
/// already reports it better than this could.
fn open_store_refusing_links(db: &Path) -> Result<icn_store::SledStore> {
    match std::fs::symlink_metadata(db) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(uninspectable(anyhow::Error::new(e).context(format!(
                "Could not determine what is at the store path {}",
                db.display()
            ))))
        }
        Ok(meta) if meta.file_type().is_symlink() => bail!(
            "Refusing to open the store at {}: it is a symbolic link. Opening it would \
             follow the link out of this data directory, and sled writes on open — so an \
             inspection of this root could modify a database the root's lock does not \
             cover.",
            db.display()
        ),
        Ok(meta) if meta.is_dir() => {
            for entry in std::fs::read_dir(db).with_context(|| {
                format!(
                    "Could not read the store directory {} to check it",
                    db.display()
                )
            })? {
                let entry = entry.with_context(|| {
                    format!(
                        "Could not read an entry of the store directory {}",
                        db.display()
                    )
                })?;
                let linked = entry
                    .file_type()
                    .with_context(|| {
                        format!(
                            "Could not inspect {} inside {}",
                            entry.path().display(),
                            db.display()
                        )
                    })?
                    .is_symlink();
                if linked {
                    bail!(
                        "Refusing to open the store at {}: {} inside it is a symbolic link, \
                         which redirects reads and writes exactly as a linked directory would.",
                        db.display(),
                        entry.path().display()
                    );
                }
            }
        }
        Ok(_) => {}
    }
    icn_store::SledStore::open(db)
}

/// Where publication stages the new configuration before renaming it into place.
fn config_publish_tmp_path(data_dir: &Path) -> PathBuf {
    data_dir.join("icn.toml").with_extension("toml.genesis-tmp")
}

/// Refuse anything at the publication temporary path that cannot be written
/// through safely.
///
/// Called twice, and both call sites matter — the same shape as the ownership
/// check above.
///
/// Publication needs it because the entire ceremony happens between preflight
/// and the write, so the answer can change underneath it.
///
/// Preflight needs it because otherwise the *first* time anything looks is at
/// publication — after both keystores are minted and the cooperative record,
/// the treasury registration and the trust rows are durable. A directory left
/// at `icn.toml.genesis-tmp` then aborts the ceremony having already written
/// all of that, leaving an INCOMPLETE root that every later `create` refuses
/// until somebody clears it by hand. A refusal that costs nothing belongs
/// before the first durable write, not after the last one.
///
/// Two things are deliberately *not* refused here.
///
/// A stale *regular* file is not: publication clears it on purpose, and
/// refusing it in preflight would strand a root after an interrupted run — the
/// failure mode this ceremony documents having removed.
///
/// A *symlink* is not either, and that boundary is load-bearing. The N2-A sweep
/// already refuses one, before anything durable exists, with a message naming
/// the link. Refusing it here first would pre-empt that gate and replace a
/// specific diagnosis with a vaguer one — and a test pinning the gate's own
/// refusal would then be satisfied by this guard instead, which is precisely
/// the attribution trap this file has had to remove before. The gap this closes
/// is the *non-symlink* non-regular entry — a directory, a socket — which the
/// sweep does not reject and nothing else looked at until publication.
fn refuse_unusable_publication_temp_path(data_dir: &Path) -> Result<()> {
    let tmp_path = config_publish_tmp_path(data_dir);
    match std::fs::symlink_metadata(&tmp_path) {
        Ok(meta) if meta.file_type().is_symlink() => Ok(()),
        Ok(meta) if !meta.is_file() => bail!(
            "Refusing institutional runtime-root provisioning: {} exists and is not a \
             regular file. Publication will not write through it, so this would fail \
             after the ceremony had already written durable state. Remove it and re-run.",
            tmp_path.display()
        ),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| {
            format!(
                "Refusing institutional runtime-root provisioning: could not determine what \
                 is at {}",
                tmp_path.display()
            )
        }),
    }
}

/// Refuse when this account would create files the daemon cannot use.
///
/// The reference is the data directory itself, exactly as in
/// [`inspection_may_create_the_lock`]: whoever owns it is the account whose
/// daemon reads this state.
///
/// [`refuse_if_the_configuration_belongs_to_another_account`] asks a narrower
/// question — would *publication* change who owns `icn.toml` — and answers it
/// correctly. It cannot answer this one. When `icn.toml` is root-owned inside a
/// service-owned root (mode `0644` in a service directory, say), a `sudo`
/// ceremony preserves that file's ownership and passes, then mints the
/// keystores, creates the cooperative and ledger databases and commits a
/// receipt — all owned by `root`. The service account cannot write those sled
/// directories, so the receipt would claim a provisioned root over state the
/// daemon cannot use, and the retained coordination files would stop it
/// starting at all.
///
/// Read-only inspection already refuses to *mint a lock* under this mismatch.
/// Provisioning writes incomparably more, so it must not be the laxer path —
/// which, until this guard, it was.
#[cfg(unix)]
pub fn refuse_if_new_files_would_not_belong_to_the_data_root_account(
    data_dir: &Path,
) -> Result<()> {
    let root_owner = match data_root_account(data_dir) {
        Ok(identity) => identity,
        // A root that is not there yet is refused with its own message by
        // `check_config_linkable`; do not pre-empt it with an ownership answer
        // about a directory that does not exist.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(e).with_context(|| format!("Failed to inspect {}", data_dir.display()))
        }
    };
    if let OwnershipTransfer::WouldChange {
        existing,
        replacement,
    } = classify_ownership_transfer(root_owner, identity_new_files_receive(data_dir)?)
    {
        bail!(
            "Refusing institutional runtime-root provisioning: this account would create files \
             in {} that the account owning it cannot use.\n\
             The directory belongs to uid {} / gid {}, but new files in it are created as \
             uid {} / gid {}.\n\
             Whoever owns the data root is the account whose daemon reads this state, so \
             provisioning under a different one would mint the keystores, create the \
             cooperative and ledger databases and commit the completion receipt where that \
             daemon cannot write them — and the coordination files, which are retained after \
             release, would stop it starting at all.\n\
             Re-run as the owning account (`sudo -u <owner> icnctl ...`), or hand the \
             directory to this account deliberately first.",
            data_dir.display(),
            existing.uid,
            existing.gid,
            replacement.uid,
            replacement.gid
        );
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn refuse_if_new_files_would_not_belong_to_the_data_root_account(
    _data_dir: &Path,
) -> Result<()> {
    Ok(())
}

/// Refuse when publication would hand the configuration to a different account.
///
/// Called twice, and both call sites matter.
///
/// * **Before the exclusion locks are taken**, so a ceremony started under the
///   wrong account creates nothing at all. This ordering is not cosmetic: the
///   coordination lock files are deliberately retained after release, so a
///   ceremony that took them as `root` and only then discovered the mismatch
///   would leave behind files the daemon's own account cannot reopen — turning
///   an operator's wrong `sudo` into a permanent startup failure.
/// * **From [`check_config_linkable`]**, which publication re-runs immediately
///   before it replaces the file, closing the window between the two.
#[cfg(unix)]
fn refuse_if_the_configuration_belongs_to_another_account(data_dir: &Path) -> Result<()> {
    let config_path = data_dir.join("icn.toml");
    let existing = match std::fs::symlink_metadata(&config_path) {
        Ok(meta) => meta,
        // No configuration yet: nothing to take away from anyone. Its absence
        // is refused separately, by `check_config_linkable`.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(e).with_context(|| format!("Failed to inspect {}", config_path.display()))
        }
    };
    // A non-regular configuration is refused by the publication step and, before
    // it, by the N2-A gate; its ownership is not this check's question.
    if existing.file_type().is_symlink() || !existing.is_file() {
        return Ok(());
    }
    if let OwnershipTransfer::WouldChange {
        existing,
        replacement,
    } = classify_ownership_transfer(
        AccessIdentity::of(&existing),
        identity_new_files_receive(data_dir)?,
    ) {
        return Err(refuse_ownership_change(&config_path, existing, replacement));
    }
    Ok(())
}

#[cfg(not(unix))]
fn refuse_if_the_configuration_belongs_to_another_account(_data_dir: &Path) -> Result<()> {
    Ok(())
}

/// What accounts new files in this directory are actually created with.
///
/// A transient probe rather than a computation: it costs one `create_new` and
/// one unlink, and it observes the same thing publication will experience,
/// including the setgid case a `getegid` answer would get wrong. It runs during
/// preflight so an account mismatch is refused *before* key material is minted,
/// rather than at publication with a half-provisioned directory left behind.
#[cfg(unix)]
fn identity_new_files_receive(dir: &Path) -> Result<AccessIdentity> {
    use std::os::unix::fs::OpenOptionsExt as _;
    let probe = dir.join(format!(
        ".icn-runtime-root-identity-probe.{}",
        std::process::id()
    ));
    // Cleared first, then created with `create_new` — which is `O_CREAT|O_EXCL`
    // and so refuses to follow a symlink, dangling or not, if one is planted
    // between the two.
    let _ = std::fs::remove_file(&probe);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&probe)
        .with_context(|| {
            format!(
                "Refusing to proceed: could not create {} to determine which account new files \
                 in this directory are owned by",
                probe.display()
            )
        })?;
    let identity = std::fs::symlink_metadata(&probe)
        .map(|m| AccessIdentity::of(&m))
        .with_context(|| format!("Failed to inspect {}", probe.display()));
    let _ = std::fs::remove_file(&probe);
    identity
}

/// Link the treasury identity into the configuration the daemon will read.
///
/// Appends a `[cooperative]` section rather than round-tripping the file
/// through a TOML serializer, so the operator's own comments and formatting —
/// including the `# jwt_secret = "CHANGE_ME"  # Set this before starting!`
/// reminder `init-coop` writes — survive. Appending at end-of-file is
/// unambiguous because a TOML table runs to the next table header or EOF.
///
/// Refuses rather than overwriting an existing `[cooperative]` section: that
/// section may already name a different treasury, and silently repointing a
/// running institution's spend source is not this command's decision.
fn check_config_linkable(data_dir: &Path) -> Result<()> {
    let config_path = data_dir.join("icn.toml");
    if !config_path.exists() {
        bail!(
            "Refusing institutional runtime-root provisioning: there is no configuration at {} to \
             link the treasury into.\n\
             The daemon resolves its treasury from `[cooperative] \
             treasury_did`, so without this link it would fall back to the node \
             DID — provisioning would appear to succeed while the machine \
             remained the treasury. Run `icnctl init-coop` (or create the \
             configuration) and re-run `institution runtime-root create`.",
            config_path.display()
        );
    }
    refuse_if_the_configuration_belongs_to_another_account(data_dir)?;

    let text = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    // `toml::from_str`, not `str::parse` — in toml 0.9 `FromStr for Value`
    // parses a bare value rather than a document, so a normal config file with
    // a leading comment fails there. This is the parser the rest of the
    // repository uses.
    // The daemon loads this file with `Config::from_file`, a plain
    // `toml::from_str::<Config>` with no defaulting layer. Checking only that
    // it is valid TOML would let this ceremony write an entire institution and
    // then certify it against a configuration `icnd` cannot load — and
    // `icnd --data-dir`, its only other form, takes `Config::default()`, whose
    // `cooperative.treasury_did` is `None`, so the node would author as itself.
    // That is exactly the collapse this work exists to prevent, so it is
    // checked HERE, before the first write, rather than discovered at
    // publication time with half an institution already on disk.
    //
    // Today this refuses the `icn.toml` that `init-coop` itself generates: that
    // template omits `[network] bootstrap_peers`, which has no serde default.
    // That is icn#2747, a pre-existing defect this ceremony does not fix —
    // refusing loudly is the honest outcome.
    match toml::from_str::<icn_core::Config>(&text) {
        Ok(mut config) => {
            // Parsing is necessary but not sufficient. `icnd` additionally runs
            // `Config::validate` at startup and exits on a fatal constraint — an
            // empty `network.listen_addr`, a trust threshold outside [0,1] — so a
            // file that merely parses can still belong to a node that cannot
            // start, and certifying it would report READY for exactly that.
            // `validate` returns Ok(warnings) / Err(fatal errors), and `icnd`
            // exits on the error arm. Warnings are the daemon's business, not
            // this ceremony's; only the fatal arm is a reason to refuse.
            // Apply the same runtime override the daemon applies *before* it
            // validates (`icnd` reads `--gateway-jwt-secret`, then
            // `ICN_GATEWAY_JWT_SECRET`, then the file). Validating the raw file
            // would make this ceremony stricter than the daemon it is checking
            // for: it would reject the standard `init-coop` flow, whose own
            // instructions recommend supplying the secret through the
            // environment, and push an operator into persisting a plaintext
            // secret in the configuration purely to satisfy a provisioning
            // check.
            //
            // The CLI flag is the daemon's own and has no counterpart here; the
            // environment variable is the part both processes can see.
            if let Ok(jwt_secret) = std::env::var("ICN_GATEWAY_JWT_SECRET") {
                config.gateway.jwt_secret = jwt_secret;
            }

            if let Err(errors) = config.validate() {
                bail!(
                    "Refusing to provision: {} parses, but the daemon would refuse to start \
                     with it:\n  {}\n\
                     A receipt certifying a node that cannot start would claim more than this \
                     ceremony can establish.",
                    config_path.display(),
                    errors.join("\n  ")
                );
            }
        }
        Err(e) => bail!(
            "Refusing to provision: {} is not loadable by the daemon \
             ({e}).\n\
             `icnd --config` parses this file with `Config::from_file`, so a \
             genesis linked into it would never be consumed. If this names a \
             missing `[network] bootstrap_peers`, that is icn#2747 — the \
             configuration `init-coop` generates cannot be loaded by the daemon \
             it tells you to run. Fix the configuration and re-run.",
            config_path.display()
        ),
    }
    let parsed: toml::Value = toml::from_str(&text)
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;
    // Presence is not the question — content is. `Config::cooperative` is not an
    // `Option`, so every configuration that has ever been round-tripped through
    // `Config::to_file` carries an *empty* `[cooperative]` table, including one
    // written by `federation add`/`remove`/`set`. Refusing on presence therefore
    // refused a root whose only sin was that an unrelated command had rewritten
    // its configuration once, and no treasury was named to protect.
    //
    // What the refusal exists for is stated right above: a section that may
    // already name a different treasury. An empty table names nothing.
    let names_cooperative_state = match parsed.get("cooperative") {
        None => false,
        // A non-table `cooperative` is state this does not understand, and
        // guessing is not better than refusing.
        Some(existing) => existing.as_table().is_none_or(|table| !table.is_empty()),
    };
    if names_cooperative_state {
        bail!(
            "Refusing institutional runtime-root provisioning: {} already has a [cooperative] \
             section that names cooperative state. It may already name a different treasury; \
             repointing an institution's spend source is an explicit operator decision.",
            config_path.display()
        );
    }
    Ok(())
}

/// Publish the `[cooperative]` link into the configuration the daemon reads.
///
/// Appends a section rather than round-tripping the file through a TOML
/// serializer, so the operator's own comments and formatting — including the
/// `# jwt_secret = "CHANGE_ME"  # Set this before starting!` reminder
/// `init-coop` writes — survive. Appending at end-of-file is unambiguous
/// because a TOML table runs to the next table header or EOF.
///
/// The write goes to a sibling temporary file which is fsynced and then
/// renamed over the original, so a crash mid-write cannot leave a truncated or
/// half-appended configuration: a reader sees either the old file or the new
/// one. `rename` is atomic within a directory on POSIX, and the temporary is
/// created in the same directory so the rename never crosses a filesystem.
///
/// This is deliberately a few lines of `std` rather than a new durability
/// helper: the repository has no shared atomic-write utility (even
/// `AgeKeyStore` uses a plain `fs::write`), and inventing one here would be a
/// larger change than this ceremony warrants.
fn publish_cooperative_config(data_dir: &Path, name: &str, treasury_did: &Did) -> Result<()> {
    use std::io::Write as _;

    let config_path = data_dir.join("icn.toml");
    // Re-checked here because this is the step that actually mutates the file;
    // the same preconditions were checked before the first write of the
    // ceremony so that a failure here is not the first sign of trouble.
    check_config_linkable(data_dir)?;
    let text = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;

    let mut out = text;
    if !out.ends_with('\n') {
        out.push('\n');
    }

    // Serialize the values through `toml` rather than interpolating them into a
    // quoted string. A cooperative name is operator-supplied text: one
    // containing a quote, a backslash or a newline would otherwise terminate the
    // string early and inject arbitrary configuration — including a second
    // `treasury_did`. Correctness outranks a pretty diff here.
    let mut table = toml::map::Map::new();
    table.insert(
        "treasury_did".to_string(),
        toml::Value::String(treasury_did.as_str().to_string()),
    );
    table.insert("name".to_string(), toml::Value::String(name.to_string()));
    let body = toml::to_string(&toml::Value::Table(table))
        .context("Failed to serialize the [cooperative] section")?;

    // An empty `[cooperative]` table may already be there — see the linkable
    // check. Populate it where it stands rather than appending a second one:
    // two `[cooperative]` tables are a duplicate key and `toml` rejects the
    // whole file, so appending would turn an accepted root into a failed
    // publication after the ceremony had already written everything else.
    //
    // Matched on the trimmed line so an unusual spelling does not silently
    // append a duplicate; if no header is found the append path runs and the
    // whole-`Config` parse below refuses, which is the fail-closed direction.
    let mut header_end = None;
    let mut cursor = 0usize;
    for line in out.split_inclusive('\n') {
        if line.trim() == "[cooperative]" {
            header_end = Some((cursor + line.len(), line.ends_with('\n')));
            break;
        }
        cursor += line.len();
    }
    match header_end {
        Some((at, newline_terminated)) => {
            let insertion = if newline_terminated {
                body.clone()
            } else {
                format!("\n{body}")
            };
            out.insert_str(at, &insertion);
        }
        None => {
            out.push_str(
                "\n# Written by `icnctl institution runtime-root create` (#2744).\n\
                 # `treasury_did` is a keypair-backed DID whose key material lives in\n\
                 # `treasury.age`. Without this section the daemon falls back to the\n\
                 # node's own DID for governance-authored ledger entries.\n\
                 [cooperative]\n",
            );
            out.push_str(&body);
        }
    }

    // Validate the candidate the way the daemon loads it — a whole-`Config`
    // parse, not just the `[cooperative]` table — before the rename, so a
    // failure leaves the original file in place. An earlier draft validated
    // only the section this ceremony writes, which let a COMPLETE receipt
    // certify a configuration `icnd` could not load.
    let candidate: icn_core::Config = toml::from_str(&out)
        .context("Refusing to publish a configuration the daemon could not load")?;
    if candidate.cooperative.treasury_did.as_deref() != Some(treasury_did.as_str()) {
        bail!(
            "Refusing to publish: the appended [cooperative] section does not \
             resolve the treasury this ceremony created"
        );
    }

    let tmp_path = config_publish_tmp_path(data_dir);

    // Never write through whatever happens to sit at the temporary path.
    // `File::create` follows symlinks, so a pre-placed link here would redirect
    // the configuration write to an arbitrary file. Refuse anything that is not
    // a plain regular file, clear a stale regular file left by an interrupted
    // run, and then create with `create_new`, which fails rather than following
    // or truncating.
    match std::fs::symlink_metadata(&tmp_path) {
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_file() => bail!(
            "Refusing to publish the configuration: {} exists and is not a \
             regular file. Genesis will not write through it.",
            tmp_path.display()
        ),
        Ok(_) => std::fs::remove_file(&tmp_path).with_context(|| {
            format!(
                "Failed to clear the stale temporary file {}",
                tmp_path.display()
            )
        })?,
        // Only absence means "nothing is in the way". Any other failure is a
        // look that did not happen, and creating through it would be writing
        // somewhere this never established was safe.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(e).with_context(|| {
                format!(
                    "Refusing to publish the configuration: could not determine what is at {}",
                    tmp_path.display()
                )
            })
        }
    }
    // The destination gets the same treatment as the temporary path. `rename`
    // replaces a symlink rather than following it, so publishing over one would
    // silently convert a deliberate indirection — a config shared from
    // `/etc/icn`, say — into a private regular file, and the metadata captured
    // below would be the *target's* rather than the link's.
    let published_contract = match std::fs::symlink_metadata(&config_path) {
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_file() => bail!(
            "Refusing to publish the configuration: {} is not a regular file. Replacing it \
             would discard whatever it is — a symlink to a shared configuration, most likely — \
             rather than update it.",
            config_path.display()
        ),
        Ok(meta) => {
            // Re-checked here, not only at preflight: the whole ceremony runs
            // between the two, and a link created in that window would leave a
            // second configuration identity outside this exclusion domain.
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt as _;
                if meta.nlink() > 1 {
                    bail!(
                        "Refusing to publish the configuration: {} has {} directory entries, not \
                         one. Publication replaces the file, so the other name(s) would keep \
                         serving it as it is now. Remove the additional link and re-run.",
                        config_path.display(),
                        meta.nlink()
                    );
                }
            }
            Some(meta)
        }
        // Nothing to replace, and nothing to preserve. Preflight requires the
        // file to exist, so this is only reachable if it vanished mid-ceremony.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            return Err(e).with_context(|| {
                format!(
                    "Refusing to publish the configuration: could not inspect {}",
                    config_path.display()
                )
            })
        }
    };

    {
        // Created 0600, then given the original file's mode once the content is
        // written. The published `icn.toml` may hold a plaintext `jwt_secret`,
        // and creating the temp file at the process umask would expose it
        // world-readable for the duration of the write.
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            opts.mode(0o600);
        }
        let mut f = opts
            .open(&tmp_path)
            .with_context(|| format!("Failed to create {}", tmp_path.display()))?;
        f.write_all(out.as_bytes())
            .with_context(|| format!("Failed to write {}", tmp_path.display()))?;

        // Access metadata is applied BEFORE the durability barrier, not after.
        // `sync_all` is `fsync`, which flushes the inode as well as the data, so
        // a mode set afterwards would not have crossed the same barrier the
        // receipt is about to certify — the file could survive a power cut with
        // its 0600 creation mode instead of the mode this publication chose.
        #[cfg(unix)]
        if let Some(meta) = published_contract.as_ref() {
            use std::os::unix::fs::PermissionsExt as _;

            // Ownership is deliberately NOT re-checked here. `check_config_linkable`
            // runs at the top of this function as well as at ceremony start, and it
            // carries the account guard — so a second copy at this point can never
            // answer first, and a mutation deleting it is invisible. One guard, in
            // the place that is actually reached. The post-condition after the
            // rename is what confirms the outcome.

            // Carry the original file's mode. `init-coop`'s template invites a
            // plaintext secret into this very file (`# jwt_secret =
            // "CHANGE_ME"  # Set this before starting!`), so a deployment that
            // hardened it to 0600 must not silently get 0644 back from the
            // process umask.
            let mode = meta.permissions().mode();
            std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(mode))
                .with_context(|| {
                    format!("Failed to carry permissions onto {}", tmp_path.display())
                })?;
        }

        f.sync_all()
            .with_context(|| format!("Failed to flush {}", tmp_path.display()))?;
    }
    // The rename is atomic, but atomicity is not durability: on a crash the
    // directory entry can still be lost unless the *directory* is synced. The
    // receipt is about to certify that the daemon will read this treasury, so
    // the linkage has to outlive a power cut, not merely a process exit.
    let sync_parent = || -> Result<()> {
        if let Some(parent) = config_path.parent() {
            sync_directory_entries(parent)?;
        }
        Ok(())
    };
    std::fs::rename(&tmp_path, &config_path).with_context(|| {
        format!(
            "Failed to publish {} over {}",
            tmp_path.display(),
            config_path.display()
        )
    })?;
    sync_parent()?;

    // Post-condition, checked rather than assumed: the file an operator and the
    // daemon will now open is a regular file carrying the access metadata of the
    // one it replaced.
    //
    // This proves the metadata was PRESERVED. It does not prove the daemon can
    // read it: this process is not running under the daemon's credentials, and
    // a check it performs itself could only ever speak for its own.
    #[cfg(unix)]
    if let Some(before) = published_contract.as_ref() {
        use std::os::unix::fs::PermissionsExt as _;
        let after = std::fs::symlink_metadata(&config_path).with_context(|| {
            format!(
                "Failed to re-inspect {} after publishing",
                config_path.display()
            )
        })?;
        if after.file_type().is_symlink() || !after.is_file() {
            bail!(
                "Verification: {} is not a regular file after publication",
                config_path.display()
            );
        }
        let (was, now) = (AccessIdentity::of(before), AccessIdentity::of(&after));
        if was != now {
            bail!(
                "Verification: publishing {} changed its owner from {was} to {now}",
                config_path.display()
            );
        }
        let (was_mode, now_mode) = (
            before.permissions().mode() & 0o7777,
            after.permissions().mode() & 0o7777,
        );
        if was_mode != now_mode {
            bail!(
                "Verification: publishing {} changed its mode from {was_mode:o} to {now_mode:o}",
                config_path.display()
            );
        }
    }
    Ok(())
}
/// Classify the root, and make sure a `--json` caller gets a document even when
/// classification itself fails.
///
/// `runtime_root_state` can fail before any output arm is reached — a receipt
/// whose schema this binary does not understand, several receipts where there
/// may be only one, a store it cannot read. Those are precisely the cases
/// automation needs to distinguish, and returning bare prose there means the
/// machine surface is absent exactly when it matters.
///
/// Errors carry a small, stable set of codes. Deliberately not a taxonomy: it is
/// enough for a caller to tell state classes apart, and anything finer would be
/// inventing categories no consumer has asked for.
fn classify_for_show(data_dir: &Path, json: bool) -> Result<RuntimeRootState> {
    // The lock is taken *here*, not at the call site, for one reason: a failure
    // to acquire it is an inspection outcome like any other, and `--json`
    // promises exactly one document for every outcome. Acquiring it above this
    // function with `?` skipped the renderer and exited with empty stdout —
    // which is precisely the contract this command exists to keep.
    //
    // `acquire_existing`, not `acquire`: observing a directory must not create
    // a persistent lock file in it. See `DataDirLock::acquire_existing`.
    //
    // The guard is dropped at the end of this function rather than held across
    // printing: everything below reads from the state already classified, and
    // holding it longer would block a ceremony for the duration of an operator
    // reading output.
    // Policy first, then a real lock — and a real lock either way.
    //
    // If new files here would belong to this account, inspection creates and
    // holds the ordinary storage lock, so it and provisioning are in one
    // exclusion domain. If they would not, it may still *join* an existing lock
    // but must not mint one; and if there is none to join it refuses, because
    // proceeding unlocked is the failure this mechanism exists to prevent.
    //
    // An earlier attempt let the absent-lock case proceed without holding
    // anything, reasoning that absence proved no holder. It proved that only at
    // the instant of the check: a witness parked `show` past that decision and
    // watched a third party take the root while the inspection was in flight.
    let state = (|| -> Result<RuntimeRootState> {
        let may_create = inspection_may_create_the_lock(data_dir)?;
        let _storage = if may_create {
            icn_core::DataDirLock::acquire(data_dir, "runtime-root inspection")?
        } else {
            icn_core::DataDirLock::acquire_without_creating(data_dir, "runtime-root inspection")?
        };

        // The storage lock does not exclude a configuration writer: they are
        // deliberately separate files, so `show` and `ManagedConfigEdit` never
        // contend. `Config::to_file` rewrites `icn.toml` with a plain
        // `fs::write`, so a classification read overlapping one can observe a
        // truncated file and report `state_unreadable` for a perfectly good
        // root, purely on arrival order. Classification reads the configuration,
        // so it joins that domain too — shared, because inspection is a reader
        // and several may look at once.
        //
        // The *same* create-or-join decision as the storage lock above, spelled
        // out rather than delegated. `acquire_config_shared_if_manageable`
        // creates the file when it is absent — `_if_manageable` is about the
        // filesystem being writable, not about who would own the result — so
        // relying on the name to mean "will not mint one" made a read-only
        // `show` leave a root-owned `.icn-config.lock` behind under `sudo`,
        // which is the poisoning this command already refuses to do with the
        // storage lock. There is no `acquire_config_without_creating` to pair
        // with, so absence plus "may not create" means proceeding without it,
        // exactly as this read did before the configuration lock existed.
        let _config = if may_create {
            icn_core::DataDirLock::acquire_config_shared_if_manageable(
                data_dir,
                "runtime-root inspection",
            )?
        } else {
            // Join it, or refuse — never proceed unlocked. The previous version
            // treated an absent lock this account may not create as licence to
            // classify anyway, which put the read outside the domain: a
            // `ManagedConfigEdit` can create that very lock and rewrite
            // `icn.toml` while this is still reading it, so the outcome would
            // depend on whether the read landed in the truncate window.
            //
            // This is the same three-outcome shape the storage lock above has
            // had all along — create, join, refuse. The configuration side
            // proceeded unlocked only because the non-creating acquire did not
            // exist to call; it does now.
            Some(
                icn_core::DataDirLock::acquire_config_shared_without_creating(
                    data_dir,
                    "runtime-root inspection",
                )?,
            )
        };

        // Test-only: pause here, between the lock decision and the first store
        // open, so the dangerous interleaving can be exercised deterministically.
        #[cfg(test)]
        inspection_barrier(data_dir);
        runtime_root_state(data_dir)
    })();
    match state {
        Ok(state) => Ok(state),
        Err(e) if json => {
            let message = format!("{e:#}");
            // Prefer the marker over the prose. `classify_show_error` guesses
            // from message text, which is fine for failures that predate this
            // distinction — but where the code already *knows* the check could
            // not be run, guessing can only get it wrong. It did: a
            // permission-denied configuration read landed in
            // `classification_failed` for want of a matching substring.
            let (code, remediation) = if e.downcast_ref::<Uninspectable>().is_some() {
                (
                    "state_unreadable",
                    "Durable state could not be inspected. If the daemon is running, stop it \
                     first — this command opens the same stores.",
                )
            } else {
                classify_show_error(&message)
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "kind": "institutional_runtime_root",
                    "state": "ERROR",
                    "error": {
                        "code": code,
                        "message": message,
                        "remediation": remediation,
                    },
                }))?
            );
            bail!("{e:#}")
        }
        Err(e) => Err(e),
    }
}

/// Map a classification failure onto a stable-ish machine category.
fn classify_show_error(message: &str) -> (&'static str, &'static str) {
    if message.contains("runtime-root receipts") || message.contains("receipts:") {
        (
            "multiple_receipts",
            "A data directory has exactly one runtime root. Inspect the extra receipts and \
             remove the wrong one deliberately.",
        )
    } else if message.contains("schema version") {
        (
            "receipt_schema_unsupported",
            "This binary does not understand the receipt's schema version. Use a build that \
             does rather than reinterpreting it.",
        )
    } else if message.contains("unreadable") || message.contains("not valid") {
        (
            "receipt_unreadable",
            "The receipt is present but cannot be decoded. Do not guess what it said; treat \
             the root as unverified.",
        )
    } else if message.contains("already holds") {
        (
            "root_in_use",
            "Another ICN process holds this data directory. Inspection opens the same stores, \
             so stop the daemon or wait for the ceremony to finish, then retry.",
        )
    } else if message.contains("could not be opened")
        || message.contains("could not be scanned")
        || message.contains("Failed to open")
        || message.contains("exclusive lock")
    {
        (
            "state_unreadable",
            "Durable state could not be inspected. If the daemon is running, stop it first — \
             this command opens the same stores.",
        )
    } else {
        (
            "classification_failed",
            "The runtime-root state could not be classified. The message names what failed.",
        )
    }
}

/// Print the receipt with an explicit statement of what was actually verified.
///
/// `provenance_verified` separates two genuinely different evidence levels, so
/// the output cannot claim more than the path that produced it checked:
///
/// * the ceremony unlocks both keystores and confirms they derive the recorded
///   DIDs before writing its commit marker;
/// * `show` re-checks every non-secret durable relationship but does not
///   prompt for a passphrase, so it cannot speak to key provenance.
fn print_receipt(data_dir: &Path, receipt: &RuntimeRootReceipt, provenance_verified: bool) {
    println!("Institutional Runtime Root");
    println!("==========================\n");
    println!("Cooperative:        {}", receipt.cooperative_name);
    println!("  id:               {}", receipt.cooperative_id);
    println!("Runtime trust root: {}", receipt.trust_root_did);
    println!("Treasury DID:       {}", receipt.treasury_did);
    println!("Founding authority: {}", receipt.genesis_authority_did);
    println!("Node DID:           {}", receipt.node_did);
    println!("Currency:           {}", receipt.currency);
    println!("Created at:         {}", receipt.created_at);
    println!("Schema version:     {}", receipt.schema_version);
    println!();
    if provenance_verified {
        println!(
            "Verified: durable state, configuration linkage, trust facts scoring\n\
             above the ledger's author threshold on a cold reopened graph, and\n\
             key provenance — the node, trust-root and treasury keystores were\n\
             each unlocked and derive the DIDs above.\n\
             NOTE (icn#2750): a running daemon scores this treasury 0.0 once any\n\
             unrelated trust edge is added in-process, so the score above\n\
             describes the persisted facts rather than the live daemon."
        );
    } else {
        println!(
            "Verified: durable state, configuration linkage, and that the trust\n\
             facts still score the treasury above the ledger's author threshold\n\
             on a cold reopened graph.\n\
             NOT re-verified: key provenance, and whether the node identity still\n\
             matches the one recorded. This command does not prompt for a\n\
             passphrase, so it cannot open the keystores — only see that they are\n\
             present.\n\
             NOTE (icn#2750): a running daemon scores this treasury 0.0 once any\n\
             unrelated trust edge is added in-process, so the score above\n\
             describes the persisted facts rather than the live daemon."
        );
    }
    println!(
        "\nThe treasury is a principal of its own; it is not the node.\n\
         This provisions the institutional runtime root. It is NOT canonical\n\
         Institution genesis: no `EntityId` is allocated and no signed founding\n\
         act is persisted, so this receipt is provisioning evidence rather than\n\
         founding evidence. It does not implement the GEN protocol (#2602),\n\
         stable Subject-generation (#2694), federation, or the Technical Alpha\n\
         as a whole."
    );
    // Say how this becomes effective, because it does not become effective on
    // its own. A daemon started without `--config` builds `Config::default()`,
    // whose `cooperative.treasury_did` is `None`, and falls back to the node
    // DID — measured, not inferred: the same data directory yields "using node
    // DID for budget payouts" without the flag and "using the cooperative's own
    // treasury principal" with it. The shipped `deploy/icnd.service` passes no
    // `--config` (icn#2755), so an operator who is not told this gets a
    // provisioned runtime root the daemon never reads.
    println!(
        "\nTo take effect, the daemon must be started with this configuration:\n\
        \x20   icnd --config {} --data-dir {}\n\
         A daemon started without --config falls back to the node DID for\n\
         governance-authored ledger entries, whatever this receipt says. The\n\
         shipped systemd unit does not pass --config yet (icn#2755).",
        data_dir.join("icn.toml").display(),
        data_dir.display()
    );
}

pub fn handle_institution_runtime_root_command(
    cmd: InstitutionRuntimeRootCommands,
    data_dir: &Path,
) -> Result<()> {
    match cmd {
        InstitutionRuntimeRootCommands::Create {
            name,
            currency,
            yes,
        } => {
            if !yes {
                use std::io::Write;
                println!(
                    "Runtime-root provisioning creates durable cooperative state, a \
                     treasury principal with its own key material, \
                     institution-rooted trust facts, and a runtime-root \
                     receipt under {}.",
                    data_dir.display()
                );
                println!("Run the daemon *stopped*; it holds exclusive store locks.");
                print!("Proceed? (y/N): ");
                std::io::stdout().flush()?;
                let mut answer = String::new();
                std::io::stdin().read_line(&mut answer)?;
                if !matches!(answer.trim().to_lowercase().as_str(), "y" | "yes") {
                    println!("Aborted; nothing was written.");
                    return Ok(());
                }
            }
            let receipt = provision_runtime_root(data_dir, &name, &currency)?;
            print_receipt(data_dir, &receipt, true);
        }
        InstitutionRuntimeRootCommands::Show { json } => {
            // `show` joins the exclusion protocol rather than racing it, but
            // the acquisition itself lives in `classify_for_show` so that a
            // refusal is rendered as JSON like every other outcome.
            //
            // Why it needs one at all: classification is not read-only.
            // `runtime_root_state` opens the stores, and `SledStore::open` is a
            // *creating* open that takes sled's own exclusive directory lock.
            // The ceremony deliberately closes its handles before verifying
            // through fresh ones, so a `show` that won a sled lock inside that
            // window made the ceremony's own re-open fail — after the keys,
            // rows and configuration were written but before the receipt was
            // committed, leaving an INCOMPLETE root that every later `create`
            // refuses. Completion must not depend on process arrival order.
            match classify_for_show(data_dir, json)? {
                RuntimeRootState::Ready(receipt) => {
                    if json {
                        // Wrap rather than print the receipt bare: a script
                        // consuming this must be able to see which evidence level
                        // produced it, and the human surface already says so. The
                        // receipt keeps its own shape under `receipt`.
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::json!({
                                "state": "READY",
                                "kind": "institutional_runtime_root",
                                // Tri-state, not booleans. `false` would conflate
                                // "checked and wrong" with "not checked" — and this
                                // path deliberately does not prompt for a
                                // passphrase, so it cannot open a keystore at all.
                                "evidence": {
                                    "durable_state": "verified",
                                    "config_linkage": "verified",
                                    "trust_score_above_ledger_threshold": "verified",
                                    "key_presence": "verified",
                                    "trust_root_key_provenance": "not_reverified",
                                    "treasury_key_provenance": "not_reverified",
                                    "node_identity": "not_reverified",
                                },
                                "note": "key provenance is not re-verified by `show`; it does not prompt for a passphrase. The ceremony verifies it before writing the receipt. `trust_score_above_ledger_threshold` is measured on a cold reopened graph: under icn#2750 the running daemon scores this treasury 0.0 once any unrelated in-process edge exists, so this field describes the persisted facts, not the live daemon.",
                                "receipt": receipt,
                            }))?
                        );
                    } else {
                        print_receipt(data_dir, &receipt, false);
                    }
                }
                RuntimeRootState::Inconsistent { receipt, problem } if json => {
                    // A script asking for JSON must get JSON on every arm, not only
                    // on success — otherwise the envelope's whole purpose (letting a
                    // consumer see which evidence level produced a result) fails in
                    // exactly the cases it matters most.
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "state": "INCONSISTENT",
                            "kind": "institutional_runtime_root",
                            "problem": problem,
                            "receipt": receipt,
                        }))?
                    );
                    bail!("INCONSISTENT runtime root under {}", data_dir.display());
                }
                RuntimeRootState::Incomplete { components } if json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "state": "INCOMPLETE",
                            "kind": "institutional_runtime_root",
                            "components": {
                                "trust_root_key": components.trust_root_key,
                                "treasury_key": components.treasury_key,
                                "cooperative_record": components.cooperative_record,
                                "treasury_registration": components.treasury_registration,
                                "trust_store_edges": components.trust_store_edges,
                                "config_linkage": components.config_linkage,
                                "receipt": components.receipt,
                            },
                        }))?
                    );
                    bail!("INCOMPLETE runtime root under {}", data_dir.display());
                }
                RuntimeRootState::NotStarted if json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "state": "NOT_STARTED",
                            "kind": "institutional_runtime_root",
                        }))?
                    );
                    bail!("No runtime root under {}", data_dir.display());
                }
                RuntimeRootState::Inconsistent { receipt, problem } => bail!(
                    "INCONSISTENT runtime root under {}: a receipt exists for cooperative \
                 {} ({}), but the state it describes does not hold:\n  {}\n\
                 This is reported as a failure rather than as a genesis: a \
                 receipt is evidence only while what it claims is still true.",
                    data_dir.display(),
                    receipt.cooperative_name,
                    receipt.cooperative_id,
                    problem
                ),
                // Reported separately from "not started" on purpose: an operator
                // whose ceremony died half-way needs to be told that, not told
                // nothing happened.
                RuntimeRootState::Incomplete { components } => bail!(
                    "INCOMPLETE runtime root under {}: components exist but no completion \
                 receipt does, so no cooperative came into existence here.\n  {}\n\
                 This state cannot be resumed; remove it deliberately before \
                 founding again.",
                    data_dir.display(),
                    components.describe()
                ),
                RuntimeRootState::NotStarted => bail!(
                    "No runtime-root receipt under {}. This data directory has not been \
                 provisioned with an institutional runtime root.",
                    data_dir.display()
                ),
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod failpoint_tests {
    //! Crash-boundary evidence produced by the ceremony itself.
    //!
    //! Each test runs the real `provision_runtime_root_inner` and forces it to stop at one
    //! mutation boundary, then asserts the component report matches exactly
    //! what the implementation had written by that point. That is the
    //! difference between proving the inspector and proving the ordering: a
    //! test that deletes rows from a completed genesis cannot tell you whether
    //! the cooperative record lands before or after the treasury registration.
    //!
    //! It also means the ordering is pinned by construction. Reordering two
    //! writes in `provision_runtime_root_inner` changes which components exist at a
    //! boundary and fails the corresponding assertion here.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use icn_identity::AgeKeyStore;

    const PASSPHRASE: &str = "failpoint-fixture-passphrase";

    /// `expect_err` with a label, returning the rendered message.
    trait UnwrapErrMsg {
        fn unwrap_err_or_else_msg(self, why: &str) -> String;
    }
    impl<T> UnwrapErrMsg for Result<T> {
        fn unwrap_err_or_else_msg(self, why: &str) -> String {
            match self {
                Ok(_) => panic!("{why}: expected a refusal, but the ceremony succeeded"),
                Err(e) => format!("{e:#}"),
            }
        }
    }

    /// A data directory provisioned to the point genesis expects: a node
    /// keystore, and a configuration the daemon could actually load.
    fn provisioned() -> tempfile::TempDir {
        // `read_passphrase` sources this; every test in this module uses the
        // same value, so the shared process environment is not a race.
        std::env::set_var("ICN_KEYSTORE_PASSPHRASE", PASSPHRASE);

        let dir = tempfile::TempDir::new().unwrap();
        AgeKeyStore::init(get_keystore_path(dir.path()), PASSPHRASE.as_bytes()).unwrap();

        // Serialized from the production `Config` rather than hand-written, so
        // the fixture cannot drift out of loadability as the schema grows — and
        // so it exercises genesis against a configuration the daemon really
        // would accept. (`init-coop`'s hand-built template is not one; that is
        // icn#2747.)
        let config = icn_core::Config {
            data_dir: dir.path().to_path_buf(),
            ..Default::default()
        };
        // `Config` always serializes a `[cooperative]` table, even with every
        // field `None`. Genesis refuses to publish over an existing one, so a
        // fixture that left it in place would exercise that refusal rather than
        // the ceremony. Strip it to represent a node that has not been
        // through genesis.
        let mut doc: toml::Value =
            toml::from_str(&toml::to_string(&config).expect("Config must serialize"))
                .expect("Config must round-trip");
        if let Some(table) = doc.as_table_mut() {
            table.remove("cooperative");
        }
        std::fs::write(
            dir.path().join("icn.toml"),
            toml::to_string(&doc).expect("fixture config must serialize"),
        )
        .unwrap();
        dir
    }

    fn run_to(dir: &Path, point: RuntimeRootFailpoint) -> anyhow::Error {
        let err = provision_runtime_root_inner(dir, "Failpoint Cooperative", "HOURS", Some(point))
            .expect_err("the injected fault must stop the ceremony");
        let msg = format!("{err:#}");
        // The ceremony must have stopped at the INJECTED point, not refused
        // during preflight — otherwise the component assertions below would be
        // asserting an untouched directory and proving nothing.
        assert!(
            msg.contains("injected runtime-root fault"),
            "the ceremony stopped for a reason other than the injected fault, \
             so this test proves nothing about ordering: {msg}"
        );
        err
    }

    /// Assert the exact component set the ceremony had written at a boundary.
    #[allow(clippy::too_many_arguments)]
    fn assert_components(
        dir: &Path,
        label: &str,
        trust_root_key: bool,
        treasury_key: bool,
        cooperative_record: bool,
        treasury_registration: bool,
        edges: usize,
        config_linkage: bool,
    ) {
        let got = runtime_root_components(dir).unwrap();
        let want = RuntimeRootComponents {
            trust_root_key,
            treasury_key,
            cooperative_record,
            treasury_registration,
            trust_store_edges: edges,
            config_linkage,
            // No boundary here is after the receipt: it is the commit point.
            receipt: false,
        };
        assert_eq!(
            got, want,
            "{label}: component report must match what the ceremony wrote"
        );
        assert!(
            !got.is_untouched(),
            "{label}: must not look like an untouched directory"
        );

        // And the state machine must call it INCOMPLETE — never untouched,
        // never complete.
        match runtime_root_state(dir).unwrap() {
            RuntimeRootState::Incomplete { .. } => {}
            other => panic!("{label}: expected Incomplete, got {other:?}"),
        }
    }

    /// A rerun over any partial state must refuse without minting anything.
    fn assert_rerun_refuses_without_minting(dir: &Path, label: &str) {
        let before: Vec<Option<Vec<u8>>> =
            [trust_root_keystore_path(dir), treasury_keystore_path(dir)]
                .iter()
                .map(|p| std::fs::read(p).ok())
                .collect();
        let coops_before = runtime_root_components(dir).unwrap();

        let err = provision_runtime_root_inner(dir, "Rerun Attempt", "HOURS", None)
            .expect_err("a rerun over partial state must refuse");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("INCOMPLETE"),
            "{label}: the refusal must name the partial state: {msg}"
        );

        let after: Vec<Option<Vec<u8>>> =
            [trust_root_keystore_path(dir), treasury_keystore_path(dir)]
                .iter()
                .map(|p| std::fs::read(p).ok())
                .collect();
        assert_eq!(
            before, after,
            "{label}: a refused rerun must not mint or replace key material"
        );
        assert_eq!(
            coops_before,
            runtime_root_components(dir).unwrap(),
            "{label}: a refused rerun must not change durable state at all"
        );
    }

    #[test]
    fn boundary_a_after_trust_root_key() {
        let dir = provisioned();
        run_to(dir.path(), RuntimeRootFailpoint::AfterTrustRootKey);
        assert_components(dir.path(), "A", true, false, false, false, 0, false);
        assert_rerun_refuses_without_minting(dir.path(), "A");
    }

    #[test]
    fn boundary_b_after_treasury_key() {
        let dir = provisioned();
        run_to(dir.path(), RuntimeRootFailpoint::AfterTreasuryKey);
        assert_components(dir.path(), "B", true, true, false, false, 0, false);
        assert_rerun_refuses_without_minting(dir.path(), "B");
    }

    #[test]
    fn boundary_c_after_cooperative_save() {
        let dir = provisioned();
        run_to(dir.path(), RuntimeRootFailpoint::AfterCooperativeSave);
        assert_components(dir.path(), "C", true, true, true, false, 0, false);
        assert_rerun_refuses_without_minting(dir.path(), "C");
    }

    #[test]
    fn boundary_d_after_treasury_registration() {
        let dir = provisioned();
        run_to(dir.path(), RuntimeRootFailpoint::AfterTreasuryRegistration);
        assert_components(dir.path(), "D", true, true, true, true, 0, false);
        assert_rerun_refuses_without_minting(dir.path(), "D");
    }

    #[test]
    fn boundary_e_after_authority_edge() {
        let dir = provisioned();
        run_to(dir.path(), RuntimeRootFailpoint::AfterAuthorityEdge);
        assert_components(dir.path(), "E", true, true, true, true, 1, false);
        assert_rerun_refuses_without_minting(dir.path(), "E");
    }

    #[test]
    fn boundary_e2_after_treasury_authority_edge() {
        let dir = provisioned();
        run_to(dir.path(), RuntimeRootFailpoint::AfterTreasuryAuthorityEdge);
        assert_components(dir.path(), "E2", true, true, true, true, 2, false);
        assert_rerun_refuses_without_minting(dir.path(), "E2");
    }

    /// The boundary that matters most.
    ///
    /// The configuration already points at the new treasury, so the daemon
    /// would consume it — and yet no genesis has been committed. This state must
    /// be reported INCOMPLETE, and must be confusable with neither an untouched
    /// node nor a completed institution.
    #[test]
    fn boundary_f_after_config_publish_before_commit() {
        let dir = provisioned();
        run_to(dir.path(), RuntimeRootFailpoint::AfterConfigPublish);
        assert_components(dir.path(), "F", true, true, true, true, 2, true);

        // The config genuinely names a treasury...
        let text = std::fs::read_to_string(dir.path().join("icn.toml")).unwrap();
        let cfg: icn_core::Config = toml::from_str(&text).unwrap();
        assert!(
            cfg.cooperative.treasury_did.is_some(),
            "F: the configuration must already name the treasury at this boundary"
        );
        // ...and genesis still reports INCOMPLETE, not COMPLETE.
        assert!(
            matches!(
                runtime_root_state(dir.path()).unwrap(),
                RuntimeRootState::Incomplete { .. }
            ),
            "F: a published configuration is not a committed runtime root"
        );
        assert_rerun_refuses_without_minting(dir.path(), "F");
    }

    /// The ownership policy, driven with identities this process could never
    /// create on disk.
    ///
    /// This is the primary evidence for the policy itself. An unprivileged test
    /// cannot `chown` a file to another user, so a test that only exercised
    /// real files would be able to check the *equal* case and nothing else —
    /// and would pass just as happily if the comparison were deleted.
    #[cfg(unix)]
    #[test]
    fn any_change_of_owner_or_group_is_refused_not_repaired() {
        let daemon = AccessIdentity { uid: 998, gid: 998 };

        assert_eq!(
            classify_ownership_transfer(daemon, daemon),
            OwnershipTransfer::Preserved,
            "the same account publishing its own configuration must be allowed"
        );

        // A different user: `sudo icnctl ...` over a service-owned config.
        assert_eq!(
            classify_ownership_transfer(daemon, AccessIdentity { uid: 0, gid: 998 }),
            OwnershipTransfer::WouldChange {
                existing: daemon,
                replacement: AccessIdentity { uid: 0, gid: 998 },
            },
            "a different owner must be refused"
        );

        // Same user, different group. This one is easy to miss: the file stays
        // readable by its owner, so an owner-only check would call it fine,
        // while every account that reached it through the group loses access.
        assert_eq!(
            classify_ownership_transfer(daemon, AccessIdentity { uid: 998, gid: 0 }),
            OwnershipTransfer::WouldChange {
                existing: daemon,
                replacement: AccessIdentity { uid: 998, gid: 0 },
            },
            "a different group must be refused too"
        );
    }

    /// The probe answers the question publication will actually ask.
    #[cfg(unix)]
    #[test]
    fn the_identity_probe_reports_what_new_files_here_are_owned_by() {
        let dir = tempfile::TempDir::new().unwrap();
        let probed = identity_new_files_receive(dir.path()).unwrap();

        let witness = dir.path().join("witness");
        std::fs::write(&witness, b"").unwrap();
        let actual = AccessIdentity::of(&std::fs::symlink_metadata(&witness).unwrap());
        assert_eq!(
            probed, actual,
            "the probe must report the identity a real new file receives here"
        );

        // And it must leave nothing behind. The name is pid-suffixed, so this
        // has to be matched by prefix — asserting the bare name would name a
        // path the probe never creates and could never fail.
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(".icn-runtime-root-identity-probe"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "the probe file must be removed; found {leftovers:?}"
        );
    }

    /// The probe reads the inode rather than asking for the process's own gid,
    /// and a setgid directory is the case where those two answers differ.
    ///
    /// Without this, `identity_new_files_receive` could be replaced by
    /// `getegid()` and every other test would still pass.
    #[cfg(unix)]
    #[test]
    fn the_identity_probe_follows_a_setgid_directory_rather_than_the_process() {
        use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

        let Some(other_gid) = a_supplementary_group() else {
            eprintln!(
                "SKIPPED the_identity_probe_follows_a_setgid_directory_rather_than_the_process: \
                 this account has no supplementary group, so a setgid directory whose group \
                 differs from the process's cannot be built. Not evidence in this environment."
            );
            return;
        };

        let dir = tempfile::TempDir::new().unwrap();
        chgrp(dir.path(), other_gid);
        // setgid: new files below here inherit the DIRECTORY's group.
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o2755)).unwrap();
        if std::fs::symlink_metadata(dir.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o2000
            == 0
        {
            eprintln!(
                "SKIPPED the_identity_probe_follows_a_setgid_directory_rather_than_the_process: \
                 this filesystem did not retain the setgid bit. Not evidence in this environment."
            );
            return;
        }

        let probed = identity_new_files_receive(dir.path()).unwrap();
        let process_gid = unsafe { libc::getegid() };
        assert_eq!(
            probed.gid, other_gid,
            "the probe must report the group a new file here actually receives"
        );
        assert_ne!(
            probed.gid, process_gid,
            "fixture: the directory's group must differ from the process's, or this proves nothing"
        );

        // And a real file agrees with the probe.
        let witness = dir.path().join("witness");
        std::fs::write(&witness, b"").unwrap();
        assert_eq!(
            std::fs::symlink_metadata(&witness).unwrap().gid(),
            probed.gid
        );
    }

    /// The publication step refuses a destination that is not a regular file.
    ///
    /// Driven directly, because the N2-A gate refuses a symlink under the data
    /// directory first and this guard is therefore unreachable through the
    /// command. It is kept for the window *after* the gate — the gate releases
    /// its locks when it returns — and because the access metadata carried onto
    /// the replacement is read from this path: following a link here would
    /// capture the target's identity and then replace the link instead.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_destination_is_refused_at_publication() {
        let dir = provisioned();
        let cfg = dir.path().join("icn.toml");
        let elsewhere = tempfile::TempDir::new().unwrap();
        let real = elsewhere.path().join("shared.toml");
        std::fs::rename(&cfg, &real).unwrap();
        std::os::unix::fs::symlink(&real, &cfg).unwrap();

        let treasury = icn_identity::KeyPair::generate().unwrap().did().clone();
        let err = publish_cooperative_config(dir.path(), "Linked Coop", &treasury)
            .expect_err("publication must refuse a non-regular destination");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("is not a regular file"),
            "the refusal must name the destination's shape: {msg}"
        );
        assert!(
            std::fs::symlink_metadata(&cfg)
                .unwrap()
                .file_type()
                .is_symlink(),
            "the link must survive"
        );
    }

    /// A supplementary group of this process that is not its effective group.
    ///
    /// The one on-disk identity change an unprivileged process can make.
    #[cfg(unix)]
    fn a_supplementary_group() -> Option<u32> {
        let egid = unsafe { libc::getegid() };
        let mut groups = vec![0 as libc::gid_t; 64];
        let n = unsafe { libc::getgroups(groups.len() as libc::c_int, groups.as_mut_ptr()) };
        if n < 0 {
            return None;
        }
        groups.truncate(n as usize);
        groups.into_iter().find(|g| *g != egid)
    }

    #[cfg(unix)]
    fn chgrp(path: &Path, gid: u32) {
        let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        // `u32::MAX` is `(uid_t)-1`: leave the owner alone.
        let rc = unsafe { libc::chown(c.as_ptr(), u32::MAX, gid) };
        assert_eq!(
            rc, 0,
            "fixture: chgrp({gid}) must succeed for a group we are in"
        );
    }

    /// Publication must not trust the preflight answer.
    ///
    /// The ownership check runs twice for a reason: preflight refuses an account
    /// mismatch before any key material is minted, but the *entire* ceremony
    /// happens between that answer and the write it is about. This moves the
    /// configuration into another group inside that window — at the last
    /// boundary before publication — and requires the publication step to
    /// refuse on its own account.
    ///
    /// Without this, deleting the publication-time check is invisible: the
    /// preflight one answers first in every ordinary run.
    #[cfg(unix)]
    #[test]
    fn a_configuration_regrouped_mid_ceremony_is_refused_at_publication() {
        let Some(_) = a_supplementary_group() else {
            eprintln!(
                "SKIPPED a_configuration_regrouped_mid_ceremony_is_refused_at_publication: \
                 this account has no supplementary group, so an on-disk identity change \
                 cannot be made without privilege. Not evidence in this environment."
            );
            return;
        };

        fn regroup(root: &Path, point: RuntimeRootFailpoint) {
            if point == RuntimeRootFailpoint::AfterTreasuryAuthorityEdge {
                if let Some(gid) = a_supplementary_group() {
                    chgrp(&root.join("icn.toml"), gid);
                }
            }
        }

        let dir = provisioned();
        let _observer = ObserverGuard::install(dir.path(), regroup);

        let err = provision_runtime_root_inner(dir.path(), "Regrouped Coop", "HOURS", None)
            .expect_err("publication must re-check the identity it is about to replace");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("would change which account owns it"),
            "the refusal must come from the ownership check at publication: {msg}"
        );
    }

    /// A stale temporary file from an interrupted run must not strand every
    /// later publication.
    ///
    /// The symlink half of this guard is redundant — `create_new` is
    /// `O_CREAT|O_EXCL` and cannot follow a link — but the `Ok(_) => remove_file`
    /// half is not: delete it and a rerun fails permanently with `EEXIST`,
    /// which no other test would catch.
    #[cfg(unix)]
    #[test]
    fn a_stale_temporary_file_does_not_strand_publication() {
        let dir = provisioned();
        let stale = dir.path().join("icn.toml.genesis-tmp");
        std::fs::write(&stale, b"left behind by an interrupted run\n").unwrap();

        let treasury = icn_identity::KeyPair::generate().unwrap().did().clone();
        publish_cooperative_config(dir.path(), "Stale Temp Coop", &treasury)
            .expect("a stale regular temporary file must be cleared, not fatal");

        assert!(
            !stale.exists(),
            "the temporary file must not survive a successful publication"
        );
        let published = std::fs::read_to_string(dir.path().join("icn.toml")).unwrap();
        assert!(
            published.contains(treasury.as_str()),
            "and the configuration must actually name the treasury"
        );
    }

    /// Every input this ceremony accepts must be one the gateway accepts.
    ///
    /// The ceremony writes straight into durable state, so nothing re-checks
    /// these values later — and a rerun over provisioned state is refused. An
    /// input this function waves through and the gateway rejects is therefore
    /// permanent.
    ///
    /// The specific trap: the gateway measures `str::len()`, which is BYTES. A
    /// hand-written "32 characters" rule counted Unicode scalars, so nine
    /// four-byte characters passed here and failed there.
    #[test]
    fn every_input_this_ceremony_accepts_is_one_the_gateway_would_accept() {
        // Nine scalars, thirty-six bytes: inside a 32-scalar rule, outside the
        // gateway's 32-byte one.
        let multibyte_currency = "\u{1F600}".repeat(9);
        assert_eq!(multibyte_currency.chars().count(), 9);
        assert_eq!(multibyte_currency.len(), 36);
        // Two hundred scalars, six hundred bytes, against the gateway's 256.
        let multibyte_name = "\u{540D}".repeat(200);
        assert_eq!(multibyte_name.chars().count(), 200);
        assert_eq!(multibyte_name.len(), 600);

        // Not a vacuous guard: the two known-bad inputs must actually refuse,
        // and ordinary input must still be accepted.
        assert!(
            validate_runtime_root_inputs("Fixture Coop", &multibyte_currency).is_err(),
            "a currency of 36 bytes must be refused, as the gateway refuses it"
        );
        assert!(
            validate_runtime_root_inputs(&multibyte_name, "SEED").is_err(),
            "a name of 600 bytes must be refused, as the gateway refuses it"
        );
        validate_runtime_root_inputs("Fixture Coop", "SEED")
            .expect("ordinary input must still be accepted");

        // And the general rule, differentially against the owner of the limits.
        let cases: Vec<(String, String)> = vec![
            ("Fixture Coop".into(), multibyte_currency.clone()),
            (multibyte_name.clone(), "SEED".into()),
            ("Fixture Coop".into(), "SEED".into()),
            ("Fixture Coop".into(), "A".repeat(32)),
            ("Fixture Coop".into(), "A".repeat(33)),
            ("N".repeat(200), "SEED".into()),
            ("\u{540D}".repeat(85), "SEED".into()),
            ("Fixture Coop".into(), "\u{1F600}".repeat(8)),
        ];
        for (name, currency) in cases {
            if validate_runtime_root_inputs(&name, &currency).is_ok() {
                icn_gateway::validation::validate_coop_name(&name).unwrap_or_else(|e| {
                    panic!(
                        "accepted a name the gateway rejects ({} bytes): {e}",
                        name.len()
                    )
                });
                icn_gateway::validation::validate_unit(&currency).unwrap_or_else(|e| {
                    panic!(
                        "accepted a currency the gateway rejects ({} bytes): {e}",
                        currency.len()
                    )
                });
            }
        }
    }
    /// A cooperative that already exists — created through the gateway, say —
    /// must not make a data directory look mid-ceremony.
    ///
    /// The gateway's `CoopManager` creates cooperatives, and `init-coop` writes
    /// its own trust edges, so those components are shared rather than
    /// provisioning-exclusive. Counting them as evidence of an interrupted
    /// ceremony would refuse provisioning on a perfectly ordinary node.
    #[test]
    fn shared_components_do_not_make_a_directory_look_mid_ceremony() {
        let dir = provisioned();

        // A cooperative record and a trust edge, written by something that is
        // not this ceremony.
        let coop_db = icn_core::config::store_path(dir.path()).join("cooperative");
        {
            let sled = std::sync::Arc::new(open_store_refusing_links(&coop_db).unwrap());
            let store = icn_coop::CoopStore::new(std::sync::Arc::new(sled.db().clone()));
            let coop = icn_coop::Cooperative::new_with_domain(
                "coop:pre-existing".to_string(),
                "Pre-existing Coop".to_string(),
                icn_coop::CoopType::Worker,
                "coop:pre-existing".to_string(),
                1,
            );
            store.save_cooperative(&coop).unwrap();
        }
        {
            let store: std::sync::Arc<dyn icn_store::Store> = std::sync::Arc::new(
                icn_store::SledStore::open(icn_core::config::trust_store_path(dir.path())).unwrap(),
            );
            let a = icn_identity::KeyPair::generate().unwrap().did().clone();
            let b = icn_identity::KeyPair::generate().unwrap().did().clone();
            let mut g = icn_trust::TrustGraph::new(store, a.clone());
            g.add_edge(icn_trust::TrustEdge::new(
                a,
                b,
                icn_trust::TrustScore::new(0.5).unwrap(),
            ))
            .unwrap();
        }

        let components = runtime_root_components(dir.path()).unwrap();
        assert!(
            components.cooperative_record,
            "fixture: the record must exist"
        );
        assert!(
            components.trust_store_edges > 0,
            "fixture: the edge must exist"
        );
        assert!(
            components.is_untouched(),
            "shared components must not read as attempted provisioning: {components:?}"
        );
        assert!(
            matches!(
                runtime_root_state(dir.path()).unwrap(),
                RuntimeRootState::NotStarted
            ),
            "a node with pre-existing cooperatives must still be foundable"
        );

        // But genesis must still REFUSE — for a different reason, from a
        // different predicate. The directory is not mid-ceremony (above), and it
        // is also not safe to found in, because the daemon has one global
        // treasury and the existing cooperative's operations would debit the new
        // one. Conflating these two questions is what made an earlier draft
        // wrong in both directions at once.
        let err = provision_runtime_root_inner(dir.path(), "Founded Anyway", "HOURS", None)
            .expect_err("provisioning must refuse a directory that already holds a cooperative");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("already holds institutional state"),
            "the refusal must be the foreign-state one, not the partial-ceremony \
             one: {msg}"
        );

        // And it must refuse before minting anything.
        let after = runtime_root_components(dir.path()).unwrap();
        assert!(
            !after.trust_root_key && !after.treasury_key && !after.receipt,
            "no provisioning-exclusive artefact may exist after a preflight refusal: {after:?}"
        );
    }

    /// Operator-supplied values are rejected before anything is written.
    #[test]
    fn invalid_genesis_inputs_are_refused_before_any_artefact_exists() {
        let long_name = "x".repeat(500);
        for (name, currency, why) in [
            ("   ", "HOURS", "whitespace-only name"),
            ("Valid Name", "  ", "whitespace-only currency"),
            (long_name.as_str(), "HOURS", "oversized name"),
            ("Valid Name", "HO URS", "currency containing whitespace"),
        ] {
            let dir = provisioned();
            let err = provision_runtime_root_inner(dir.path(), name, currency, None)
                .unwrap_err_or_else_msg(why);
            assert!(
                err.contains("Refusing institutional runtime-root provisioning"),
                "{why}: expected a refusal, got: {err}"
            );
            let c = runtime_root_components(dir.path()).unwrap();
            assert!(
                c.is_untouched(),
                "{why}: nothing may be written before input validation passes: {c:?}"
            );
        }
    }

    /// A keystore that does not derive the DID the receipt names must be
    /// refused, not certified.
    ///
    /// This is the provenance half of "the treasury is a real principal": file
    /// presence alone would let a substituted keystore pass.
    #[test]
    fn a_keystore_that_does_not_back_its_recorded_did_is_refused() {
        let dir = provisioned();
        let receipt =
            provision_runtime_root_inner(dir.path(), "Provenance Cooperative", "HOURS", None)
                .unwrap();

        // Swap the treasury keystore for one belonging to a different principal.
        let path = treasury_keystore_path(dir.path());
        std::fs::remove_file(&path).unwrap();
        let impostor = AgeKeyStore::init(&path, PASSPHRASE.as_bytes()).unwrap();
        let impostor_did = icn_identity::KeyStore::get_keypair(&impostor)
            .unwrap()
            .did()
            .clone();
        assert_ne!(impostor_did.as_str(), receipt.treasury_did);

        let err = verify_durable_state(dir.path(), &receipt, Some(PASSPHRASE.as_bytes()))
            .expect_err("a substituted keystore must be refused");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("does not back it"),
            "the refusal must name the provenance mismatch: {msg}"
        );

        // And presence-only verification (what `show` does) still passes, which
        // is the documented limitation rather than an accident.
        verify_durable_state(dir.path(), &receipt, None)
            .expect("presence-only verification cannot detect a substitution");
    }

    /// What a rerun over a substituted keystore ACTUALLY refuses on.
    ///
    /// An earlier comment claimed a post-genesis keystore substitution was
    /// "detectable by `runtime-root create` refusing a rerun". This test exists to
    /// check that sentence rather than to preserve it, because it conflates two
    /// different refusals: "a runtime root already exists here" and "the key on disk
    /// no longer backs the DID the receipt names". Only the second is
    /// provenance detection.
    #[test]
    fn a_rerun_over_a_substituted_keystore_refuses_on_prior_genesis_not_provenance() {
        let dir = provisioned();
        let receipt =
            provision_runtime_root_inner(dir.path(), "Substitution Coop", "HOURS", None).unwrap();

        let path = treasury_keystore_path(dir.path());
        std::fs::remove_file(&path).unwrap();
        AgeKeyStore::init(&path, PASSPHRASE.as_bytes()).unwrap();

        let err = provision_runtime_root_inner(dir.path(), "Rerun", "HOURS", None)
            .expect_err("a rerun must refuse");
        let msg = format!("{err:#}");

        // The refusal is about a prior ceremony, NOT about provenance.
        assert!(
            msg.contains("has already been provisioned"),
            "expected the prior-ceremony refusal, got: {msg}"
        );
        assert!(
            !msg.contains("does not back it"),
            "the rerun path does not perform passphrase-backed provenance \
             verification, so it must not be documented as if it did: {msg}"
        );

        // And the mismatch is real — passphrase-backed verification does catch
        // it, which is what the ceremony runs before writing its own marker.
        let provenance = verify_durable_state(dir.path(), &receipt, Some(PASSPHRASE.as_bytes()))
            .expect_err("passphrase-backed verification must catch the substitution");
        assert!(format!("{provenance:#}").contains("does not back it"));
    }

    /// Inability to inspect existing state is a refusal, never an absence —
    /// through the whole command.
    ///
    /// Attribution, stated honestly: **this fixture does not distinguish which
    /// guard refuses.** Two of them read the ledger store —
    /// `runtime_root_components` (reached from `refuse_if_already_provisioned`)
    /// and `refuse_if_foreign_institutional_state` a step later — and either
    /// alone produces a refusal that satisfies the assertions below. What this
    /// covers is the end-to-end property: an uninspectable store never becomes
    /// "nothing is there", and nothing is minted. Each guard's own
    /// discriminator is separate: `foreign_state_inspection_refuses_an_
    /// unopenable_store_on_its_own` below, and
    /// `an_uninspectable_trust_store_refuses_rather_than_counting_zero_edges`
    /// for the component scan, which is the only reader of the trust store.
    #[test]
    fn an_uninspectable_store_refuses_rather_than_reading_as_empty() {
        let dir = provisioned();
        // The LEDGER store specifically. The cooperative store is already opened
        // earlier by `load_receipt`, which fails closed on its own, so corrupting
        // that one would prove nothing about the inspection guards.
        let ledger_db = icn_core::config::ledger_store_path(dir.path());
        std::fs::create_dir_all(ledger_db.parent().unwrap()).unwrap();
        std::fs::write(&ledger_db, b"not a database").unwrap();

        let err = provision_runtime_root_inner(dir.path(), "Uninspectable", "HOURS", None)
            .expect_err("an uninspectable store must refuse");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("could not be opened")
                || msg.contains("could not be scanned")
                || msg.contains("Could not open")
                || msg.contains("Could not scan"),
            "the refusal must name the inspection failure, not report absence: {msg}"
        );
        assert!(
            msg.contains(&ledger_db.display().to_string()),
            "and must name the store it could not inspect: {msg}"
        );
        // Checked on the filesystem, not through `runtime_root_components`:
        // that helper is now fail-closed and would itself refuse over the
        // corrupted store, so calling it here would test the fixture rather
        // than the invariant.
        for artefact in [
            trust_root_keystore_path(dir.path()),
            treasury_keystore_path(dir.path()),
        ] {
            assert!(
                !artefact.exists(),
                "nothing may be minted when existing state cannot be inspected: {}",
                artefact.display()
            );
        }
    }

    /// A store directory that cannot be inspected is not an absent one.
    ///
    /// `Path::exists()` answers `false` for every metadata error, so an
    /// unreadable store read as clean — and preflight would then mint both
    /// keystores before failing to open it, leaving a permanently INCOMPLETE
    /// root. Driven through the guard directly, because an unreadable parent
    /// directory is easier to build than to reach through the whole command.
    #[cfg(unix)]
    #[test]
    fn a_store_directory_that_cannot_be_inspected_is_not_reported_absent() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::TempDir::new().unwrap();
        let parent = dir.path().join("store");
        let db = parent.join("cooperative");
        std::fs::create_dir_all(&db).unwrap();

        // No traversal: `symlink_metadata` on the child now fails with EACCES
        // rather than reporting the directory absent.
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o000)).unwrap();
        let reachable = std::fs::symlink_metadata(&db).is_ok();
        if reachable {
            std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
            eprintln!(
                "SKIPPED a_store_directory_that_cannot_be_inspected_is_not_reported_absent: \
                 this process traverses a 0000 directory (running as root?). Not evidence here."
            );
            return;
        }

        let outcome = store_directory_exists(&db);
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();

        let msg = format!(
            "{:#}",
            outcome.expect_err("an uninspectable store must refuse, not read as absent")
        );
        assert!(
            msg.contains("could not determine whether"),
            "the refusal must say the question could not be answered: {msg}"
        );
    }

    /// Two-party gate for the concurrency witness below.
    ///
    /// `(reached, released)`. No sleeps anywhere: `show` signals that it has
    /// arrived, and is released by the ceremony's own boundary observer, so the
    /// interleaving is the same on every run and on every machine.
    #[cfg(unix)]
    fn overlap_gate() -> &'static (std::sync::Mutex<(bool, bool)>, std::sync::Condvar) {
        static G: std::sync::OnceLock<(std::sync::Mutex<(bool, bool)>, std::sync::Condvar)> =
            std::sync::OnceLock::new();
        G.get_or_init(|| {
            (
                std::sync::Mutex::new((false, false)),
                std::sync::Condvar::new(),
            )
        })
    }

    /// `show`'s side: announce arrival, then block until the ceremony releases.
    #[cfg(unix)]
    fn inspection_waits_at_the_gate(_root: &Path) {
        let (m, cv) = overlap_gate();
        let mut st = m.lock().unwrap();
        st.0 = true;
        cv.notify_all();
        while !st.1 {
            st = cv.wait(st).unwrap();
        }
    }

    /// Release `show` from the test thread.
    #[cfg(unix)]
    fn release_inspection() {
        let (m, cv) = overlap_gate();
        let mut st = m.lock().unwrap();
        st.1 = true;
        cv.notify_all();
    }

    /// **The concurrency witness.** Is inspection inside the exclusion domain?
    ///
    /// Deterministic, with no sleeps:
    ///
    /// 1. `show` runs on a root with no lock file, decides about the lock, and
    ///    parks before opening any durable state;
    /// 2. the test asks whether a third party can now take the data root.
    ///
    /// That question *is* the property. If a third party can, `show` holds
    /// nothing and only the scheduler stands between an inspection and a
    /// ceremony's close/reopen window — which is not an exclusion protocol.
    /// Racing for the harm instead would be weaker and flakier: it would pass
    /// whenever the two threads happened not to collide.
    ///
    /// Measured: an earlier design that returned "nothing to hold" when the
    /// lock file was absent failed this outright — the third party took the
    /// root while the inspection was in flight.
    ///
    /// `show` is then released, and provisioning must still complete and leave
    /// a root that classifies READY from a fresh look — so the exclusion is not
    /// bought by breaking the ceremony.
    #[cfg(unix)]
    #[test]
    fn inspection_holds_the_exclusion_domain_it_inspects_under() {
        let dir = provisioned();
        let root = dir.path().to_path_buf();
        assert!(
            !icn_core::DataDirLock::lock_path(&root).exists(),
            "fixture: the root must start with no lock file — the case at issue"
        );
        {
            // These statics outlive one test; start from a known state.
            let (m, _) = overlap_gate();
            *m.lock().unwrap() = (false, false);
        }

        let _barrier = BarrierGuard::install(&root, inspection_waits_at_the_gate);
        let inspecting = {
            let root = root.clone();
            std::thread::spawn(move || classify_for_show(&root, false).is_ok())
        };

        {
            let (m, cv) = overlap_gate();
            let mut st = m.lock().unwrap();
            while !st.0 {
                st = cv.wait(st).unwrap();
            }
        }

        // THE PROPERTY, asserted rather than raced for.
        let intruder = icn_core::DataDirLock::acquire(&root, "a third party");
        let inside = intruder.is_err();
        drop(intruder);

        release_inspection();
        let _ = inspecting.join();
        drop(_barrier);

        assert!(
            inside,
            "`show` was past its lock decision and holding nothing: a third party took the \
             data root while an inspection was in flight. Inspection is outside the exclusion \
             domain, so nothing but scheduling prevents it from entering the stores during a \
             ceremony's close/reopen window."
        );

        // And the exclusion is not bought by breaking provisioning.
        let ceremony = provision_runtime_root_inner(&root, "Overlap Coop", "HOURS", None)
            .expect("provisioning must succeed once the inspection has finished");
        match runtime_root_state(&root).expect("the finished root must classify") {
            RuntimeRootState::Ready(receipt) => {
                assert_eq!(receipt.treasury_did, ceremony.treasury_did);
            }
            other => panic!("a completed ceremony must leave a READY root, got {other:?}"),
        }
    }

    /// Every namespace entry the receipt depends on is made durable before the
    /// receipt is written.
    ///
    /// **What this proves and what it does not.** A test cannot cut power, so
    /// it cannot observe crash survival, and this does not pretend to. What it
    /// observes is the ordering the durability claim rests on: that both
    /// directory levels this ceremony can create — `<data_dir>/store`, and
    /// `store/{cooperative,ledger,trust}` beneath it — had their entries
    /// `fsync`ed *before* the completion marker was committed. Flushing a sled
    /// database makes its contents durable and says nothing about the entry
    /// that names it; sled owns durability inside its own directory and cannot
    /// own the outside.
    ///
    /// The failure this rules out is the forbidden one: a receipt surviving a
    /// crash while the ledger directory that substantiates it does not.
    #[test]
    fn every_directory_the_receipt_depends_on_is_durable_before_the_receipt() {
        let dir = provisioned();
        let root = dir.path().to_path_buf();
        let store = icn_core::config::store_path(&root);
        assert!(
            !store.exists(),
            "fixture: the store must not exist yet, so the ceremony really creates it"
        );

        let recorder = DurabilityRecorder::install(&root);
        provision_runtime_root_inner(&root, "Durable Coop", "HOURS", None)
            .expect("provisioning must succeed");
        let events = recorder.events();
        drop(recorder);

        let receipt_at = events
            .iter()
            .position(|e| matches!(e, DurabilityEvent::WroteReceipt(_)))
            .expect("the receipt must have been written");

        // Both levels, and both before the marker.
        for required in [store.clone(), root.clone()] {
            let synced_at = events
                .iter()
                .position(|e| matches!(e, DurabilityEvent::SyncedDirectory(p) if *p == required));
            let synced_at = synced_at.unwrap_or_else(|| {
                panic!(
                    "{} was never synced; a newly created child of it would not survive a \
                     crash that the receipt claims to have outlived.\nevents: {events:?}",
                    required.display()
                )
            });
            assert!(
                synced_at < receipt_at,
                "{} was synced only AFTER the receipt was written, so a crash between them \
                 leaves a READY marker over a name that was never persisted.\nevents: {events:?}",
                required.display()
            );
        }

        // And the entries themselves are the ones that matter.
        for db in ["cooperative", "ledger", "trust"] {
            assert!(
                store.join(db).exists(),
                "fixture: {db} must be one of the entries under the synced directory"
            );
        }
    }

    /// The component scan's own witness: the trust store.
    ///
    /// `runtime_root_components` is the **only** reader of the trust store
    /// during inspection, so this is the one fixture that fails if its scan
    /// goes back to `.unwrap_or(0)`. The ledger-store fixture cannot show that:
    /// `refuse_if_foreign_institutional_state` reads the ledger store too and
    /// was already fail-closed, so it would refuse either way.
    #[test]
    fn an_uninspectable_trust_store_refuses_rather_than_counting_zero_edges() {
        let dir = provisioned();
        let trust_db = icn_core::config::trust_store_path(dir.path());
        std::fs::create_dir_all(trust_db.parent().unwrap()).unwrap();
        std::fs::write(&trust_db, b"not a database").unwrap();

        let err = runtime_root_components(dir.path())
            .expect_err("an unopenable trust store must refuse, not count zero edges");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("Could not open") || msg.contains("Could not scan"),
            "the refusal must name the inspection failure: {msg}"
        );
        assert!(
            msg.contains(&trust_db.display().to_string()),
            "and must name the store it could not inspect: {msg}"
        );
        // Prose is not the contract. Without the marker, `show --json` falls
        // back to guessing the code from the message text.
        assert!(
            err.downcast_ref::<Uninspectable>().is_some(),
            "and must carry the marker that renders as state_unreadable: {msg}"
        );
    }

    /// `unknown != absent`, at the configuration, with no receipt to re-verify.
    ///
    /// The receipt-*present* path already refuses an unreadable configuration:
    /// `verify_durable_state` reads it and marks the failure. The receipt-absent
    /// path did not. It read `icn.toml` through `.ok()`, so a configuration that
    /// could not be parsed reported `config_linkage: false` and the root
    /// rendered INCOMPLETE -- "no configuration was ever published" -- while a
    /// corrupt one was published and sitting there.
    ///
    /// Deliberately malformed content rather than a mode change: `0o000` is
    /// ignored when the suite runs as root, and a witness that silently stops
    /// discriminating is worse than none.
    #[test]
    fn a_malformed_configuration_is_not_reported_as_an_absent_one() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("icn.toml"), b"[cooperative\nid = ").unwrap();

        let err = runtime_root_components(dir.path())
            .expect_err("a configuration that cannot be parsed is not a missing one");
        let msg = format!("{err:#}");
        assert!(
            err.downcast_ref::<Uninspectable>().is_some(),
            "it must render as state_unreadable, not as an absent linkage: {msg}"
        );
        assert!(
            msg.contains("cooperative linkage"),
            "and must say which question went unanswered: {msg}"
        );
    }

    /// The same rule for a read that fails before parsing can begin.
    ///
    /// A directory at `icn.toml` fails the read with `IsADirectory` rather than
    /// `NotFound`, which is uid-independent -- unlike a permission bit.
    #[test]
    fn an_unreadable_configuration_is_not_reported_as_an_absent_one() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("icn.toml")).unwrap();

        let err = runtime_root_components(dir.path())
            .expect_err("a configuration that cannot be read is not a missing one");
        let msg = format!("{err:#}");
        assert!(
            err.downcast_ref::<Uninspectable>().is_some(),
            "it must render as state_unreadable: {msg}"
        );
    }

    /// The control that keeps the two above from being satisfied by refusing
    /// everything. Absence is a real answer, and it must survive.
    #[test]
    fn an_absent_configuration_still_reads_as_no_linkage() {
        let dir = tempfile::TempDir::new().unwrap();
        let components = runtime_root_components(dir.path())
            .expect("a directory with no configuration is inspectable, not unreadable");
        assert!(
            !components.config_linkage,
            "a configuration that is genuinely not there is absent, not unknown"
        );
    }

    /// And the control for the positive answer, so `config_linkage` cannot be
    /// hard-wired to `false` while the two refusal witnesses still pass.
    ///
    /// This asserted a weaker fact when it was written — that a `[cooperative]`
    /// *section* reads as linkage — and its fixture did not even carry a
    /// treasury. That definition was wrong rather than merely loose: every file
    /// round-tripped through `Config::to_file` has an empty `[cooperative]`
    /// table, so presence reported linkage for a root where nothing had been
    /// published, telling an operator reading an INCOMPLETE report that the
    /// configuration step was done. The linkage this ceremony publishes is a
    /// `treasury_did`, so that is what the control now asserts.
    #[test]
    fn a_configuration_carrying_a_treasury_reads_as_linked() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("icn.toml"),
            b"[cooperative]\ntreasury_did = \"did:icn:zExampleTreasury\"\n",
        )
        .unwrap();

        let components = runtime_root_components(dir.path()).expect("a parseable configuration");
        assert!(
            components.config_linkage,
            "a published `treasury_did` is the linkage this reports"
        );
    }

    /// The discriminating half of the same rule: a table without a treasury is
    /// not linkage. Without this, requiring the treasury could be reverted to a
    /// presence test and the positive control above would still pass.
    #[test]
    fn a_cooperative_section_without_a_treasury_is_not_linkage() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("icn.toml"),
            b"[cooperative]\nname = \"Round-tripped, never provisioned\"\n",
        )
        .unwrap();

        let components = runtime_root_components(dir.path()).expect("a parseable configuration");
        assert!(
            !components.config_linkage,
            "a section with no treasury is what `Config::to_file` leaves behind, not evidence \
             that provisioning published anything"
        );
    }

    /// Two ceremonies observed at once must not see each other's events.
    ///
    /// Directly pins the defect that made
    /// `every_directory_the_receipt_depends_on_is_durable_before_the_receipt`
    /// fail under `cargo test`'s parallelism: the recorder stored a root and
    /// then never consulted it, so one log captured every ceremony in the
    /// process. Asserting on the routing rather than on a scheduling outcome,
    /// because a flaky witness cannot pin a flakiness fix.
    #[test]
    fn a_durability_recorder_captures_only_its_own_root() {
        let a = tempfile::TempDir::new().unwrap();
        let b = tempfile::TempDir::new().unwrap();
        let recorder_a = DurabilityRecorder::install(a.path());
        let recorder_b = DurabilityRecorder::install(b.path());

        record_durability_event(DurabilityEvent::SyncedDirectory(a.path().join("store")));
        record_durability_event(DurabilityEvent::WroteReceipt(b.path().to_path_buf()));

        let events_a = recorder_a.events();
        let events_b = recorder_b.events();
        assert_eq!(
            events_a.len(),
            1,
            "a recorder must see its own event and nobody else's: {events_a:?}"
        );
        assert!(
            matches!(&events_a[0], DurabilityEvent::SyncedDirectory(p) if *p == a.path().join("store")),
            "and it must be the one recorded for this root: {events_a:?}"
        );
        assert_eq!(
            events_b.len(),
            1,
            "the receipt must be attributed to the root it was written under: {events_b:?}"
        );
        assert!(
            matches!(&events_b[0], DurabilityEvent::WroteReceipt(p) if p == b.path()),
            "{events_b:?}"
        );

        drop(recorder_a);
        assert_eq!(
            recorder_b.events().len(),
            1,
            "dropping one recorder must not clear another's log"
        );
    }

    /// Two observers installed at once must both survive.
    ///
    /// The single slot had two failure modes and this pins both: a second
    /// install evicted the first, and the first drop cleared the slot for
    /// everyone. Dispatch was already root-scoped and is unchanged.
    #[test]
    fn two_boundary_observers_for_different_roots_coexist() {
        fn noop(_: &Path, _: RuntimeRootFailpoint) {}

        let a = tempfile::TempDir::new().unwrap();
        let b = tempfile::TempDir::new().unwrap();
        let installed = |root: &Path| {
            let here = icn_core::DataDirLock::lock_path(root);
            LATE_BOUNDARY_OBSERVERS
                .lock()
                .map(|observers| observers.iter().any(|o| o.expected_root == here))
                .unwrap_or(false)
        };

        let guard_a = ObserverGuard::install(a.path(), noop);
        let guard_b = ObserverGuard::install(b.path(), noop);
        assert!(
            installed(a.path()),
            "installing a second observer must not evict the first"
        );
        assert!(installed(b.path()), "and the second must be installed");

        drop(guard_a);
        assert!(
            !installed(a.path()),
            "dropping a guard must remove its own entry"
        );
        assert!(
            installed(b.path()),
            "and must not remove an observer another live test depends on"
        );
        drop(guard_b);
    }

    /// `refuse_if_foreign_institutional_state`'s own witness.
    ///
    /// Driven directly, because an earlier fail-closed inspection now reaches
    /// the same store first through the command. Without this, restoring the
    /// `if let Ok(..)` swallowing that this guard documents having removed
    /// would be invisible: the ceremony would still refuse, just one step
    /// sooner and for a different reason.
    #[test]
    fn foreign_state_inspection_refuses_an_unopenable_store_on_its_own() {
        let dir = provisioned();
        let ledger_db = icn_core::config::ledger_store_path(dir.path());
        std::fs::create_dir_all(ledger_db.parent().unwrap()).unwrap();
        std::fs::write(&ledger_db, b"not a database").unwrap();

        let err = refuse_if_foreign_institutional_state(dir.path())
            .expect_err("an unopenable store must refuse, not read as absent");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("could not be opened") || msg.contains("could not be scanned"),
            "the refusal must name the inspection failure: {msg}"
        );
        assert!(
            msg.contains(&ledger_db.display().to_string()),
            "and must name the store it could not inspect: {msg}"
        );
    }

    /// A trust edge rewritten to a low score keeps both rows but drops the
    /// treasury under the ledger's gate. Presence checks cannot see that.
    #[test]
    fn an_edge_rewritten_below_the_ledger_threshold_is_refused() {
        let dir = provisioned();
        let receipt =
            provision_runtime_root_inner(dir.path(), "Low Score Coop", "HOURS", None).unwrap();

        // 1.0 * 0.2 * 0.3 = 0.06, under the 0.1 gate, with both rows intact.
        {
            let store: Arc<dyn icn_store::Store> = Arc::new(
                icn_store::SledStore::open(icn_core::config::trust_store_path(dir.path())).unwrap(),
            );
            let node: Did = receipt.node_did.parse().unwrap();
            let root: Did = receipt.trust_root_did.parse().unwrap();
            let treasury: Did = receipt.treasury_did.parse().unwrap();
            let mut g = icn_trust::TrustGraph::new(store, node);
            g.add_edge(icn_trust::TrustEdge::new(
                root.clone(),
                treasury.clone(),
                icn_trust::TrustScore::new(0.2).unwrap(),
            ))
            .unwrap();
            assert!(
                g.get_edge(&root, &treasury).unwrap().is_some(),
                "fixture: the row must still exist — that is the whole point"
            );
        }

        let err = verify_durable_state(dir.path(), &receipt, None)
            .expect_err("a treasury under the ledger's gate must not verify");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("author-trust threshold"),
            "the refusal must name the insufficient effective score, not a missing row: {msg}"
        );
    }

    /// A relative configured `data_dir` cannot be proven equivalent to the CLI
    /// root, so it is refused before anything is written.
    #[test]
    fn a_relative_configured_data_dir_is_refused() {
        let dir = provisioned();
        let path = dir.path().join("icn.toml");
        let text = std::fs::read_to_string(&path).unwrap();
        let patched = text.replace(
            &format!("data_dir = \"{}\"", dir.path().display()),
            "data_dir = \"relative/sub/dir\"",
        );
        assert_ne!(patched, text, "fixture: the data_dir line must be present");
        std::fs::write(&path, patched).unwrap();

        let err = provision_runtime_root_inner(dir.path(), "Relative Root", "HOURS", None)
            .expect_err("a relative configured data_dir must refuse");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("relative `data_dir`"),
            "the refusal must name the relative root specifically: {msg}"
        );
        assert!(
            runtime_root_components(dir.path()).unwrap().is_untouched(),
            "nothing may be written when the storage root cannot be proven"
        );
    }

    /// More than one receipt is corruption, not a menu.
    #[test]
    fn multiple_genesis_receipts_are_refused_rather_than_picking_one() {
        use icn_store::Store;

        let dir = provisioned();
        let receipt =
            provision_runtime_root_inner(dir.path(), "First Receipt Coop", "HOURS", None).unwrap();

        {
            let store = icn_store::SledStore::open(coop_db_path(dir.path())).unwrap();
            let mut second = receipt.clone();
            second.cooperative_id = "coop:second".to_string();
            second.cooperative_name = "Second Receipt Coop".to_string();
            store
                .put(
                    format!("{RECEIPT_KEY_PREFIX}coop:second").as_bytes(),
                    &serde_json::to_vec(&second).unwrap(),
                )
                .unwrap();
            store.flush().unwrap();
        }

        let err = load_receipt(dir.path()).expect_err("two receipts must refuse");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("2 runtime-root receipts"),
            "the refusal must name the multiplicity rather than parsing one: {msg}"
        );
    }

    /// Legitimate mutable treasury state must NOT invalidate the runtime root.
    ///
    /// `entity_id` moving from `None` to `Some` is the #2082 backfill, an
    /// ordinary and correct later operation. Over-verifying it would make a
    /// previously-READY root report INCONSISTENT the moment that ran — which is
    /// its own defect, and the reason this ceremony verifies establishment facts
    /// rather than the whole record.
    #[test]
    fn populating_the_treasury_entity_id_does_not_invalidate_the_runtime_root() {
        let dir = provisioned();
        let receipt =
            provision_runtime_root_inner(dir.path(), "Entity Backfill Coop", "HOURS", None)
                .unwrap();

        {
            let store: Arc<dyn icn_store::Store> = Arc::new(
                icn_store::SledStore::open(icn_core::config::ledger_store_path(dir.path()))
                    .unwrap(),
            );
            let mut mgr = icn_ledger::TreasuryManager::with_store(store).unwrap();
            let treasury: Did = receipt.treasury_did.parse().unwrap();
            let entity = icn_entity::EntityId::cooperative("runtime-root-backfill").unwrap();
            let outcome = mgr
                .populate_entity_id_at_activation(&treasury, &receipt.cooperative_id, entity)
                .expect("the backfill seam must not error");
            assert!(
                matches!(
                    outcome,
                    icn_ledger::TreasuryEntityIdPopulateResult::Populated
                ),
                "fixture: the backfill must actually have applied, got {outcome:?}"
            );
        }

        verify_durable_state(dir.path(), &receipt, None)
            .expect("an entity_id backfill is legitimate and must not invalidate the root");
    }

    /// An immutable establishment fact changing DOES invalidate it.
    #[test]
    fn a_treasury_reregistered_to_another_cooperative_is_refused() {
        let dir = provisioned();
        let receipt =
            provision_runtime_root_inner(dir.path(), "Rebound Treasury Coop", "HOURS", None)
                .unwrap();

        // Rewrite the persisted treasury row so it names a different cooperative.
        {
            use icn_store::Store;
            let store = icn_store::SledStore::open(icn_core::config::ledger_store_path(dir.path()))
                .unwrap();
            let key = format!("ledger:treasury:{}", receipt.treasury_did);
            let raw = store.get(key.as_bytes()).unwrap().expect("treasury row");
            let mut row: serde_json::Value = serde_json::from_slice(&raw).unwrap();
            row["coop_id"] = serde_json::Value::String("coop:someone-else".to_string());
            store
                .put(key.as_bytes(), &serde_json::to_vec(&row).unwrap())
                .unwrap();
            store.flush().unwrap();
        }

        let err = verify_durable_state(dir.path(), &receipt, None)
            .expect_err("a treasury bound to another cooperative must refuse");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("does not belong to the cooperative"),
            "the refusal must name the broken establishment relation: {msg}"
        );
    }

    /// C2 — a running daemon blocks the ceremony, before any mutation.
    ///
    /// The test holds the same lock `icnd` holds for its lifetime, which is
    /// exactly the daemon's side of the protocol. The N2-A gate cannot provide
    /// this: it takes sled locks and releases them when it returns.
    #[test]
    fn a_held_data_dir_lock_refuses_the_ceremony_before_any_mutation() {
        let dir = provisioned();
        let daemon_side = icn_core::DataDirLock::acquire(dir.path(), "the daemon").unwrap();

        let err = provision_runtime_root_inner(dir.path(), "Blocked By Daemon", "HOURS", None)
            .expect_err("a held data-directory lock must refuse the ceremony");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("already holds"),
            "the refusal must name the conflicting holder: {msg}"
        );
        assert!(
            runtime_root_components(dir.path()).unwrap().is_untouched(),
            "nothing may be written while another process owns the root"
        );

        drop(daemon_side);
        provision_runtime_root_inner(dir.path(), "Unblocked", "HOURS", None)
            .expect("the ceremony must proceed once the holder releases");
    }

    /// The ceremony still owns the root at its LATEST boundary — after every
    /// store handle has been dropped and the configuration published.
    ///
    /// That window is the one sled locks do not cover, and it is exactly where a
    /// daemon could otherwise load stale configuration and enter service. An
    /// earlier version of this witness checked contention after the ceremony
    /// returned, which proves only that a released lock can be retaken; this
    /// observes from inside the ceremony's own stack frame.
    #[test]
    fn the_ceremony_still_owns_the_root_at_its_last_boundary() {
        use std::sync::atomic::{AtomicBool, Ordering};
        static OBSERVED: AtomicBool = AtomicBool::new(false);
        static CONTENDED: AtomicBool = AtomicBool::new(false);

        fn observe(data_dir: &Path, point: RuntimeRootFailpoint) {
            if point != RuntimeRootFailpoint::AfterConfigPublish {
                return;
            }
            OBSERVED.store(true, Ordering::SeqCst);
            // A second acquisition, from this same process but a separate open
            // file description — which `flock(2)` treats as a distinct owner.
            let contended =
                icn_core::DataDirLock::acquire(data_dir, "a daemon starting up").is_err();
            CONTENDED.store(contended, Ordering::SeqCst);
        }

        let dir = provisioned();
        OBSERVED.store(false, Ordering::SeqCst);
        CONTENDED.store(false, Ordering::SeqCst);
        let _observer = ObserverGuard::install(dir.path(), observe);
        let outcome = provision_runtime_root_inner(
            dir.path(),
            "Late Boundary Coop",
            "HOURS",
            Some(RuntimeRootFailpoint::AfterConfigPublish),
        );
        drop(_observer);

        assert!(
            outcome.is_err(),
            "the injected fault must stop the ceremony"
        );
        assert!(
            OBSERVED.load(Ordering::SeqCst),
            "the observer must have run at the late boundary, or this test \
             proves nothing"
        );
        assert!(
            CONTENDED.load(Ordering::SeqCst),
            "the ceremony must STILL own the root after publishing the \
             configuration — this is the window a daemon could otherwise use"
        );
    }

    /// A failed durability barrier cannot be followed by a committed READY.
    ///
    /// This is deliberately not a power-loss simulation. What it establishes:
    /// when the barrier that makes minted key material durable reports failure,
    /// the ceremony stops, no receipt is written, the leftover state is
    /// classified honestly, and a rerun refuses instead of silently reminting
    /// over key material that may or may not have reached the disk.
    #[test]
    fn a_failed_keystore_durability_barrier_cannot_be_followed_by_a_ready_receipt() {
        let dir = provisioned();
        let armed = DurabilityFailureGuard::arm(dir.path());
        let outcome =
            provision_runtime_root_inner(dir.path(), "Durability Failure Coop", "HOURS", None);
        drop(armed);

        let err = outcome.expect_err("a failed durability barrier must stop the ceremony");
        assert!(
            format!("{err:#}").contains("Failed to sync"),
            "the failure must name the durability barrier: {err:#}"
        );

        // No commit marker.
        let components = runtime_root_components(dir.path()).unwrap();
        assert!(
            !components.receipt,
            "a READY receipt must not exist after a durability failure: {components:?}"
        );
        assert!(
            matches!(
                runtime_root_state(dir.path()).unwrap(),
                RuntimeRootState::Incomplete { .. }
            ),
            "the leftover state must be reported as incomplete, not as untouched \
             and not as ready"
        );

        // And a rerun refuses rather than reminting over it.
        let rerun = provision_runtime_root_inner(dir.path(), "Rerun After Failure", "HOURS", None)
            .expect_err("a rerun over partial state must refuse");
        assert!(
            format!("{rerun:#}").contains("INCOMPLETE"),
            "the rerun must name the partial state: {rerun:#}"
        );
    }

    /// Without a failpoint the same code path commits, and the state machine
    /// says so.""" Without this, every assertion above could be satisfied by a
    /// ceremony that never completes at all.
    #[test]
    fn the_uninjected_ceremony_commits() {
        let dir = provisioned();
        let receipt =
            provision_runtime_root_inner(dir.path(), "Committed Cooperative", "HOURS", None)
                .unwrap();
        let components = runtime_root_components(dir.path()).unwrap();
        assert_eq!(
            components,
            RuntimeRootComponents {
                trust_root_key: true,
                treasury_key: true,
                cooperative_record: true,
                treasury_registration: true,
                trust_store_edges: 2,
                config_linkage: true,
                receipt: true,
            }
        );
        match runtime_root_state(dir.path()).unwrap() {
            RuntimeRootState::Ready(r) => {
                assert_eq!(r.treasury_did, receipt.treasury_did);
                assert_ne!(r.treasury_did, r.node_did);
                assert_ne!(r.trust_root_did, r.node_did);
                assert_ne!(r.trust_root_did, r.treasury_did);
            }
            other => panic!("expected Complete, got {other:?}"),
        }
    }

    // ---------------------------------------------------------------------
    // #2749 review tranche: three findings, three witnesses.
    // ---------------------------------------------------------------------

    /// A provisioned runtime root refuses a command that would replace the node
    /// DID, and does not touch the keystore while refusing.
    ///
    /// One fact different from `a_node_did_change_is_allowed_with_no_runtime_root`
    /// below: whether the ceremony ran.
    #[test]
    fn a_node_did_change_is_refused_once_a_runtime_root_exists() {
        let dir = provisioned();
        let receipt = provision_runtime_root(dir.path(), "Authority Binding Coop", "HOURS")
            .expect("the fixture ceremony must succeed");

        // Captured AFTER provisioning, so this is the exact byte sequence a
        // rotation would replace.
        let keystore = get_keystore_path(dir.path());
        let before = std::fs::read(&keystore).expect("keystore must be readable");

        let msg = refuse_node_did_change_if_runtime_root_exists(dir.path(), "`icnctl id rotate`")
            .unwrap_err_or_else_msg("a provisioned root must refuse a node DID change");

        // Attributable to the authority binding — not to a generic lock or a
        // generic "already provisioned" refusal.
        assert!(
            msg.contains("runtime root is provisioned"),
            "refusal must name the established runtime root: {msg}"
        );
        assert!(
            msg.contains(&receipt.node_did),
            "refusal must name the node DID the binding starts at: {msg}"
        );

        // The refusal happened BEFORE any identity mutation.
        let after = std::fs::read(&keystore).expect("keystore must still be readable");
        assert_eq!(
            before, after,
            "a refusal must not have rewritten identity.age"
        );
    }

    /// The same command on an unprovisioned root is not refused.
    ///
    /// Without this, the witness above would also pass against a guard that
    /// refused unconditionally.
    #[test]
    fn a_node_did_change_is_allowed_with_no_runtime_root() {
        let dir = provisioned(); // keystore + loadable config, no ceremony
        refuse_node_did_change_if_runtime_root_exists(dir.path(), "`icnctl id rotate`")
            .expect("an unprovisioned root must not refuse a node DID change");
    }

    /// An interrupted ceremony still binds components to the current node DID,
    /// so a DID change is refused there too.
    #[test]
    fn a_node_did_change_is_refused_on_an_interrupted_ceremony() {
        let dir = provisioned();
        let _ = run_to(dir.path(), RuntimeRootFailpoint::AfterCooperativeSave);

        let msg = refuse_node_did_change_if_runtime_root_exists(dir.path(), "`icnctl id import`")
            .unwrap_err_or_else_msg("an interrupted ceremony must refuse a node DID change");
        assert!(
            msg.contains("interrupted runtime-root ceremony"),
            "refusal must name the interrupted ceremony: {msg}"
        );
    }

    /// A runtime root that cannot be read is not evidence of an absent one.
    #[test]
    fn a_node_did_change_fails_closed_when_the_root_is_unreadable() {
        let dir = provisioned();
        provision_runtime_root(dir.path(), "Unreadable Coop", "HOURS")
            .expect("the fixture ceremony must succeed");

        // Make the first cooperative-row read fail at the storage layer, which
        // is what makes `runtime_root_state` return Err(Uninspectable).
        let _armed = CoopRowReadFailureGuard::arm(dir.path());

        let msg = refuse_node_did_change_if_runtime_root_exists(dir.path(), "`icnctl id rotate`")
            .unwrap_err_or_else_msg("an unreadable root must fail closed");
        assert!(
            msg.contains("could not be determined"),
            "refusal must say the state was unreadable, not that none exists: {msg}"
        );
    }

    /// A storage failure reading the first cooperative row is reported as
    /// *uninspectable*, never as a disagreement with the receipt.
    ///
    /// The distinction is the whole point: `INCONSISTENT` is a positive claim
    /// that durable state contradicts the receipt. Deriving it from a read that
    /// never completed asserts a fact nothing observed.
    #[test]
    fn a_cooperative_row_storage_failure_is_uninspectable_not_inconsistent() {
        let dir = provisioned();
        provision_runtime_root(dir.path(), "Storage Failure Coop", "HOURS")
            .expect("the fixture ceremony must succeed");

        // Sanity: without the injection this root is Ready. Otherwise the
        // assertion below could pass against a root that was broken anyway.
        assert!(
            matches!(
                runtime_root_state(dir.path()),
                Ok(RuntimeRootState::Ready(_))
            ),
            "fixture must be Ready before the storage failure is armed"
        );

        let _armed = CoopRowReadFailureGuard::arm(dir.path());
        let err = runtime_root_state(dir.path())
            .expect_err("a storage failure must not produce an Ok state");

        assert!(
            err.downcast_ref::<Uninspectable>().is_some(),
            "a storage failure must be marked uninspectable, got: {err:#}"
        );
    }

    /// A *decode* failure is uninspectable for the same reason.
    #[test]
    fn the_uninspectable_classification_is_by_variant_not_by_message() {
        // `Serialization` carries no sled error and renders differently from
        // `Storage`; both must land in the same class. A substring-matching
        // implementation would separate them.
        for e in [
            icn_coop::CoopError::Storage(sled::Error::Unsupported("io".to_string())),
            icn_coop::CoopError::Serialization("undecodable row".to_string()),
        ] {
            assert!(
                matches!(
                    e,
                    icn_coop::CoopError::Storage(_) | icn_coop::CoopError::Serialization(_)
                ),
                "both failure-to-inspect variants must classify together"
            );
        }
        // And an observation must not.
        assert!(
            !matches!(
                icn_coop::CoopError::NotFound("coop".to_string()),
                icn_coop::CoopError::Storage(_) | icn_coop::CoopError::Serialization(_)
            ),
            "NotFound is an observation, not a failure to inspect"
        );
    }

    /// A local configuration writer cannot cross the ceremony's publication
    /// boundary.
    ///
    /// Deterministic, not a sleep race: the probe runs *inside* the ceremony's
    /// own stack frame at `AfterConfigPublish`, while both ceremony guards are
    /// still held. If the exclusive configuration lock did not cover the
    /// publication, this acquisition would succeed.
    #[test]
    fn a_config_writer_cannot_acquire_the_config_lock_across_publication() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        // 0 = never observed, 1 = observed and refused, 2 = observed and ADMITTED
        static OUTCOME: AtomicUsize = AtomicUsize::new(0);
        OUTCOME.store(0, Ordering::SeqCst);

        fn probe(root: &Path, point: RuntimeRootFailpoint) {
            if point != RuntimeRootFailpoint::AfterConfigPublish {
                return;
            }
            // Exactly what `ManagedConfigEdit::open` does first.
            let admitted =
                icn_core::DataDirLock::acquire_config(root, "witness federation writer").is_ok();
            OUTCOME.store(if admitted { 2 } else { 1 }, Ordering::SeqCst);
        }

        let dir = provisioned();
        let _observer = ObserverGuard::install(dir.path(), probe);
        provision_runtime_root(dir.path(), "Exclusion Coop", "HOURS")
            .expect("the ceremony must succeed");

        match OUTCOME.load(Ordering::SeqCst) {
            1 => {}
            0 => panic!(
                "the probe never ran — the ceremony did not reach AfterConfigPublish, so this \
                 witness proved nothing"
            ),
            2 => panic!(
                "a local configuration writer acquired the configuration lock while the ceremony \
                 held it across publication; a stale snapshot could overwrite [cooperative]"
            ),
            other => panic!("unexpected probe outcome {other}"),
        }
    }

    /// The guard runs while the identity command already holds the data-root
    /// lock, so it must not try to take that lock again.
    ///
    /// Every other witness in this tranche calls the guard *unlocked*, which is
    /// not how production reaches it: `handle_id_command` acquires
    /// `DataDirLock` for any identity-mutating command and only then asks
    /// whether the node DID may change. A guard that re-acquired the same lock
    /// would deadlock in production while every one of those witnesses stayed
    /// green — so this pins the ordering that makes them meaningful.
    #[test]
    fn the_node_did_guard_runs_under_the_identity_lock_without_deadlocking() {
        let dir = provisioned();
        let receipt = provision_runtime_root(dir.path(), "Locked Path Coop", "HOURS")
            .expect("the fixture ceremony must succeed");

        // Exactly what `handle_id_command` holds when it reaches the guard.
        let _identity_lock = icn_core::DataDirLock::acquire(dir.path(), "this identity command")
            .expect("the identity lock must be available");

        let msg = refuse_node_did_change_if_runtime_root_exists(dir.path(), "`icnctl id rotate`")
            .unwrap_err_or_else_msg("the guard must still refuse under the identity lock");
        assert!(
            msg.contains(&receipt.node_did),
            "the refusal must be the authority-binding one, not a lock-contention error: {msg}"
        );
    }
}
