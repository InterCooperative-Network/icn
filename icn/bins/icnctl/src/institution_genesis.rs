//! Institutional genesis — a cooperative comes into existence as an
//! institution, not as a string (#2744).
//!
//! # Why this is a separate, offline ceremony
//!
//! Before this existed, the only production path that mentioned a cooperative
//! was `icnctl init-coop`, which wrote a keystore and an `icn.toml` with **no**
//! `[cooperative]` section. `CooperativeConfig.treasury_did` was therefore
//! always `None`, and `supervisor/lifecycle.rs` fell back to the node's own DID
//! for every governance-authored ledger entry. The cooperative's treasury *was*
//! the node operator.
//!
//! The treasury-creation code existed but had no runtime producer:
//! `CoopHandle::create_treasury` and `activate_cooperative` had callers only
//! behind `#[cfg(test)]`.
//!
//! Three candidate homes were rejected on evidence:
//!
//! * `institution bootstrap apply` authenticates to a **running gateway** with
//!   `--coop-id`, so it requires the institution to exist in order to create
//!   it, and its `BootstrapOperation` set has no treasury operation at all.
//! * The gateway/RPC surface cannot write the facts genesis needs: production
//!   builds `TrustManager::with_trust_service`, which leaves `trust_graph:
//!   None`, so trust edges land in an in-memory `DashMap` and never reach
//!   `<data_dir>/store/trust`.
//! * Extending `init-coop` would inherit #2725 (its existing-config branch
//!   ignores the config's own `data_dir`) and would conflate provisioning a
//!   node with founding an institution.
//!
//! What is left is a privileged **local** ceremony run with the daemon
//! stopped, alongside the existing maintenance commands that already open
//! these sled databases directly and already cross the N2-A startup gate.
//!
//! # Why the treasury needs its own key material
//!
//! `Did::from_str` requires the bytes after `did:icn:` to decode to exactly 32
//! bytes **and** to be a valid Ed25519 point, and `Deserialize for Did` calls
//! it. Measured against the two spellings this repository already produces:
//!
//! * `derive_treasury_did` -> `did:icn:treasury:<bs58>` never parses (the extra
//!   label is not multibase);
//! * `Did::from_anchor_id` -> parsed 140 of 300 sampled values; it is 16 hashed
//!   bytes zero-padded to 32, so whether it lands on the curve is chance.
//!
//! `lifecycle.rs` parses the configured treasury DID with `.ok()` and falls back
//! to the node DID, so either spelling would be *silently* replaced by the node
//! DID — the exact collapse this issue exists to prevent. A genesis treasury
//! DID must therefore come from a real keypair. `AgeKeyStore` holds a single
//! identity bundle, so a distinct principal is necessarily a distinct keystore
//! file rather than a second string.
//!
//! # Scope
//!
//! This is the **Principal-generation** runtime genesis slice. It does not
//! implement or satisfy the GEN protocol (#2602), stable Subject-generation
//! (#2694), federation, or the Technical Alpha as a whole.

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
pub const GENESIS_RECEIPT_SCHEMA_VERSION: u32 = 1;

/// Storage key of the genesis receipt inside the cooperative store.
///
/// It lives in the same sled database as the `coop:` rows it describes so that
/// a receipt cannot survive a cooperative store that was replaced or removed.
const RECEIPT_KEY_PREFIX: &str = "genesis:receipt:";

#[derive(Subcommand, Debug)]
pub enum InstitutionGenesisCommands {
    /// Perform institutional genesis for a new cooperative.
    ///
    /// Creates durable cooperative state, a treasury principal with its own key
    /// material, the institution-rooted trust facts a governance-authored
    /// ledger entry needs, and a versioned genesis receipt.
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

    /// Read back the genesis receipt.
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
/// ceremony completed (see `Ceremony ordering` on [`run_genesis`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisReceipt {
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

    // Compare resolved paths so `/A` and `/A/` — or a relative spelling of the
    // same directory — are not reported as a disagreement. `canonicalize` is
    // used only when the target exists; otherwise the lexical forms are
    // compared, which is the honest answer for a path that is not there yet.
    let cli = std::fs::canonicalize(data_dir).unwrap_or_else(|_| data_dir.to_path_buf());
    let cfg_path = PathBuf::from(configured);
    let cfg = std::fs::canonicalize(&cfg_path).unwrap_or(cfg_path);
    if cli != cfg {
        bail!(
            "Refusing institutional genesis: the storage root on the command \
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

/// What a data directory currently shows about institutional genesis.
///
/// Deliberately three states rather than two. "A receipt exists" and "some
/// artefacts exist but no receipt does" are different operator situations and
/// must not produce the same message.
#[derive(Debug)]
pub enum GenesisState {
    /// Nothing has been written.
    NotStarted,
    /// Artefacts exist but no completion receipt does — a ceremony that did
    /// not finish. Carries the artefacts found, so the refusal can name them.
    Incomplete { artefacts: Vec<String> },
    /// A completion receipt whose claims still hold against durable state.
    Complete(Box<GenesisReceipt>),
    /// A completion receipt exists, but durable state no longer agrees with it.
    ///
    /// A receipt is only evidence if reading it re-checks what it asserts.
    /// Otherwise it is an assertion that outlives the state it describes.
    Inconsistent {
        receipt: Box<GenesisReceipt>,
        problem: String,
    },
}

/// Inspect the data directory without writing to it.
pub fn genesis_state(data_dir: &Path) -> Result<GenesisState> {
    if let Some(receipt) = load_receipt(data_dir)? {
        // Re-verify rather than trusting the row. This is what makes the
        // receipt evidence: it cannot outlive the cooperative record, the
        // treasury registration, the trust facts, or the configuration linkage
        // that make its claims true. It is also what keeps the commit ordering
        // honest — a receipt written before the configuration was published
        // would surface here as inconsistent, not as a genesis.
        return Ok(match verify_durable_state(data_dir, &receipt) {
            Ok(()) => GenesisState::Complete(Box::new(receipt)),
            Err(problem) => GenesisState::Inconsistent {
                receipt: Box::new(receipt),
                problem: format!("{problem:#}"),
            },
        });
    }
    let mut artefacts = Vec::new();
    for (path, what) in [
        (
            trust_root_keystore_path(data_dir),
            "genesis trust-root key material",
        ),
        (treasury_keystore_path(data_dir), "treasury key material"),
    ] {
        // `symlink_metadata`, not `exists()`. `exists()` follows symlinks and so
        // reports `false` for a DANGLING one — which would let the ceremony
        // proceed and then create the keystore *through* that link, writing
        // private key material to a path outside this data directory.
        // `symlink_metadata` stats the link itself, so anything sitting at
        // these paths at all is treated as an artefact and refused.
        if std::fs::symlink_metadata(&path).is_ok() {
            artefacts.push(format!("{what} at {}", path.display()));
        }
    }
    if artefacts.is_empty() {
        Ok(GenesisState::NotStarted)
    } else {
        Ok(GenesisState::Incomplete { artefacts })
    }
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
fn refuse_if_already_started(data_dir: &Path) -> Result<()> {
    match genesis_state(data_dir)? {
        GenesisState::NotStarted => Ok(()),
        GenesisState::Inconsistent { receipt, problem } => bail!(
            "Refusing institutional genesis: this data directory holds a genesis \
             receipt for cooperative {} ({}) whose claims no longer hold:\n  {}\n\
             Founding again over inconsistent state would compound it. Inspect \
             and resolve this deliberately.",
            receipt.cooperative_name,
            receipt.cooperative_id,
            problem
        ),
        GenesisState::Complete(receipt) => bail!(
            "Refusing institutional genesis: this data directory has already \
             undergone genesis.\n  cooperative: {} ({})\n  treasury:    {}\n\
             Founding a second institution over the first would orphan its \
             treasury and trust facts.",
            receipt.cooperative_name,
            receipt.cooperative_id,
            receipt.treasury_did
        ),
        GenesisState::Incomplete { artefacts } => bail!(
            "Refusing institutional genesis: this data directory holds \
             INCOMPLETE genesis state — artefacts from a ceremony that did not \
             finish, with no completion receipt:\n  {}\n\
             Genesis never overwrites key material, and it cannot resume: \
             re-running would mint a second set of principals and leave the \
             first orphaned. Inspect this state and remove it deliberately \
             before founding again.",
            artefacts.join("\n  ")
        ),
    }
}

/// Mint a principal and persist its key material, returning its DID.
///
/// The DID comes from the generated public key (`Did::from_public_key`), so it
/// is backed by a key this node holds and survives the daemon's own config
/// parse. Nothing here fabricates a DID string.
fn mint_principal(path: &Path, passphrase: &[u8], what: &str) -> Result<Did> {
    let keystore = AgeKeyStore::init(path, passphrase)
        .with_context(|| format!("Failed to create {what} keystore at {}", path.display()))?;
    let did = keystore
        .get_keypair()
        .with_context(|| format!("Failed to read the generated {what} keypair"))?
        .did()
        .clone();
    Ok(did)
}

/// Read the genesis receipt back out of the cooperative store.
pub fn load_receipt(data_dir: &Path) -> Result<Option<GenesisReceipt>> {
    let coop_db = icn_core::config::store_path(data_dir).join("cooperative");
    if !coop_db.exists() {
        return Ok(None);
    }
    let store = icn_store::SledStore::open(&coop_db).with_context(|| {
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
    let Some((_, value)) = found.into_iter().next() else {
        return Ok(None);
    };
    let receipt: GenesisReceipt = serde_json::from_slice(&value)
        .context("Genesis receipt is present but unreadable; refusing to guess what it said")?;
    if receipt.schema_version != GENESIS_RECEIPT_SCHEMA_VERSION {
        bail!(
            "Genesis receipt schema version {} is not the {} this binary \
             understands; refusing to interpret it.",
            receipt.schema_version,
            GENESIS_RECEIPT_SCHEMA_VERSION
        );
    }
    Ok(Some(receipt))
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
/// 2. cross the N2-A startup gate (before any store is opened);
/// 3. refuse if key material already exists, if there is no node keystore to
///    found under, or if the configuration cannot be linked (nothing written);
/// 4. prove founding authority by unlocking the node keystore;
/// 5. refuse if a receipt already exists;
/// 6. mint institution and treasury key material;
/// 7. write the cooperative record;
/// 8. register the treasury durably;
/// 9. write the institution-rooted trust facts;
/// 10. write the receipt — **the commit point**;
/// 11. link the configuration.
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
fn run_genesis(data_dir: &Path, name: &str, currency: &str) -> Result<GenesisReceipt> {
    // (1) Never write authoritative state where the daemon will not read it.
    resolve_storage_root(data_dir)?;

    // (2) Before anything opens or writes a store. The gate takes exclusive
    // locks while it audits, so this also fails fast when the daemon is running
    // — which is exactly when this ceremony must not proceed.
    enforce_n2a_gate(data_dir, "institutional genesis")?;

    // (3) Refuse over existing key material before minting anything.
    refuse_if_already_started(data_dir)?;

    // (3b) There is no authority to found under without the node keystore.
    // Checked for *existence* here, among the other cheap refusals, so that a
    // ceremony destined to fail does not first prompt for a passphrase. The
    // authority is actually *proven* at (4), by unlocking it.
    //
    // This precedes the configuration check on purpose: "under whose authority"
    // is the more fundamental question, and a data directory missing both
    // should say so rather than reporting the shallower problem.
    let node_keystore_path = get_keystore_path(data_dir);
    if std::fs::symlink_metadata(&node_keystore_path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        bail!(
            "Refusing institutional genesis: the node keystore at {} is a \
             symlink. Founding authority must be proven against real key \
             material in this data directory, not through a redirection.",
            node_keystore_path.display()
        );
    }
    if !node_keystore_path.exists() {
        bail!(
            "Refusing institutional genesis: no node identity at {}.\n\
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

    // (5) `refuse_if_already_started` already covered a completed or
    // half-finished ceremony above, before the passphrase prompt. Nothing to
    // re-check here.

    // (6) Mint the two principals. Both are keypair-backed, so both survive the
    // daemon's config parse and neither is a fabricated string.
    let trust_root_did = mint_principal(
        &trust_root_keystore_path(data_dir),
        &passphrase,
        "genesis trust-root",
    )?;
    let treasury_did = mint_principal(&treasury_keystore_path(data_dir), &passphrase, "treasury")?;

    // The invariant this whole issue exists to establish. Asserted rather than
    // assumed: these come from independent `KeyPair::generate()` calls, so an
    // equality here would mean the identity layer itself is broken, and
    // continuing would reintroduce exactly the collapse being fixed.
    if treasury_did == node_did || trust_root_did == node_did {
        bail!(
            "Refusing institutional genesis: a generated institutional \
             principal collided with the node DID ({node_did}). Genesis fails \
             rather than letting the machine stand in for the institution."
        );
    }

    let coop_id = format!("coop:{}", uuid::Uuid::new_v4());

    // (7) Durable cooperative record, in the database the daemon opens.
    let coop_db_path = icn_core::config::store_path(data_dir).join("cooperative");
    let coop_sled = Arc::new(icn_store::SledStore::open(&coop_db_path).with_context(|| {
        format!(
            "Failed to open the cooperative store at {} (stop the daemon first; \
             it holds an exclusive lock)",
            coop_db_path.display()
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

    // (8) Register the treasury durably. `with_store` is required: the plain
    // `TreasuryManager::new()` keeps its maps in memory only, and a treasury
    // that vanishes on restart is not institutional state.
    let ledger_db_path = icn_core::config::ledger_store_path(data_dir);
    let ledger_store: Arc<dyn icn_store::Store> = Arc::new(
        icn_store::SledStore::open(&ledger_db_path).with_context(|| {
            format!(
                "Failed to open the ledger store at {} (stop the daemon first; \
                 it holds an exclusive lock)",
                ledger_db_path.display()
            )
        })?,
    );
    let mut treasury_manager = icn_ledger::TreasuryManager::with_store(ledger_store)
        .context("Failed to open the treasury manager over the ledger store")?;
    treasury_manager
        .register_treasury(
            treasury_did.clone(),
            coop_id.clone(),
            currency.to_string(),
            genesis_authority_did.clone(),
            Some(format!("Genesis treasury for {name}")),
        )
        .context("Failed to register the treasury")?;

    // (9) The two trust facts, and precisely what they do and do not mean.
    //
    // The ledger's author-trust gate scores an author through
    // `TrustGraph::compute_trust_score`, which is ego-centric from the node's
    // own DID. An `institution -> treasury` edge *alone* is unreachable from
    // that origin and scores 0.0, so both edges are required:
    //
    //   node -> institution      the operator's local recognition of the
    //                            institution founded here. Not a self-edge.
    //   institution -> treasury  a trust fact whose SOURCE is the institution.
    //
    // Together they score 1.0*1.0 * 0.3 (the transitive weight) = 0.30, above
    // the 0.1 the ledger requires.
    //
    // What the second edge is NOT: it is not signed by the institution's key,
    // and `TrustEdge` carries no signature or provenance field, so the store
    // records no evidence of who established it. It is written through
    // `TrustGraph::add_edge` — the storage primitive, and the only API that
    // accepts an arbitrary source, since the service-level `submit_attestation`
    // hardcodes the caller's own DID — under the privilege of this local
    // ceremony, authenticated by the founding Principal's keystore.
    //
    // So the honest description is an *institution-attributed* trust fact
    // established under the founding Principal's privileged local ceremony.
    // It is NOT a cryptographic authorisation by the institution, and this
    // slice does not claim the institution holds authority independent of the
    // node: removing `node -> institution` also drops the author to 0.0. Both
    // of those are covered by tests, so the claim cannot drift.
    //
    // Genesis never writes the `node -> node` self-edge that ICN_DEV_SELF_TRUST
    // writes.
    let trust_db_path = icn_core::config::trust_store_path(data_dir);
    let trust_store: Arc<dyn icn_store::Store> =
        Arc::new(icn_store::SledStore::open(&trust_db_path).with_context(|| {
            format!(
                "Failed to open the trust store at {} (stop the daemon first; \
                 it holds an exclusive lock)",
                trust_db_path.display()
            )
        })?);
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
    trust_graph
        .add_edge(icn_trust::TrustEdge::new_typed(
            trust_root_did.clone(),
            treasury_did.clone(),
            full,
            graph_type,
        ))
        .map_err(|e| anyhow::anyhow!("Failed to record the trust root's authority edge: {e}"))?;

    // Close every store handle before verification. sled takes an exclusive
    // directory lock, so the fresh handles in (11) cannot open these databases
    // while this ceremony still holds them — and a verification that read back
    // through the writer's own handle would prove nothing about durability.
    drop(coop_store);
    coop_sled
        .flush()
        .context("Failed to flush cooperative state")?;
    drop(coop_sled);
    drop(treasury_manager);
    drop(trust_graph);

    // (10) Publish the configuration. This is what makes the treasury
    // *consumed* by the daemon rather than merely stored, so it precedes the
    // completion marker: a receipt written before this point would claim a
    // genesis the daemon would not act on.
    publish_cooperative_config(data_dir, name, &treasury_did)?;

    let receipt = GenesisReceipt {
        schema_version: GENESIS_RECEIPT_SCHEMA_VERSION,
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
    verify_durable_state(data_dir, &receipt)?;

    // (12) The completion marker, written last and only over verified state.
    {
        use icn_store::Store;
        let coop_db_path = icn_core::config::store_path(data_dir).join("cooperative");
        let coop_sled = icn_store::SledStore::open(&coop_db_path)
            .context("Failed to reopen the cooperative store to record the receipt")?;
        let key = format!("{RECEIPT_KEY_PREFIX}{coop_id}");
        let value = serde_json::to_vec(&receipt).context("Failed to encode the genesis receipt")?;
        // Written into the same sled database as the `coop:` rows, so the
        // receipt cannot outlive the cooperative it describes.
        coop_sled
            .put(key.as_bytes(), &value)
            .context("Failed to persist the genesis receipt")?;
        coop_sled
            .flush()
            .context("Failed to flush the genesis receipt")?;
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
fn verify_durable_state(data_dir: &Path, receipt: &GenesisReceipt) -> Result<()> {
    // The cooperative record, and its link to the treasury.
    let coop_db_path = icn_core::config::store_path(data_dir).join("cooperative");
    let coop_sled = Arc::new(
        icn_store::SledStore::open(&coop_db_path)
            .context("Verification: failed to reopen the cooperative store")?,
    );
    let coop_store = icn_coop::CoopStore::new(Arc::new(coop_sled.db().clone()));
    let stored = coop_store
        .get_cooperative(&receipt.cooperative_id)
        .map_err(|e| {
            anyhow::anyhow!(
                "Verification: the cooperative record {} is not readable back ({e}); refusing to \
                 record a genesis receipt for state that is not there.",
                receipt.cooperative_id
            )
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
        icn_store::SledStore::open(icn_core::config::ledger_store_path(data_dir))
            .context("Verification: failed to reopen the ledger store")?,
    );
    let manager = icn_ledger::TreasuryManager::with_store(ledger_store)
        .context("Verification: the treasury store did not rehydrate")?;
    let treasury_did: Did = receipt
        .treasury_did
        .parse()
        .context("Verification: the treasury DID in the receipt is not a usable DID")?;
    if manager.get_treasury(&treasury_did).is_none() {
        bail!(
            "Verification: treasury {} is not readable back from the ledger store.",
            receipt.treasury_did
        );
    }
    drop(manager);

    // Both trust facts, read through a fresh handle.
    let trust_store: Arc<dyn icn_store::Store> = Arc::new(
        icn_store::SledStore::open(icn_core::config::trust_store_path(data_dir))
            .context("Verification: failed to reopen the trust store")?,
    );
    let node_did: Did = receipt
        .node_did
        .parse()
        .context("Verification: the node DID in the receipt is not a usable DID")?;
    let trust_root_did: Did = receipt
        .trust_root_did
        .parse()
        .context("Verification: the institution DID in the receipt is not a usable DID")?;
    let graph = icn_trust::TrustGraph::new(trust_store, node_did.clone());
    for (source, target, what) in [
        (
            &node_did,
            &trust_root_did,
            "the node's recognition of the institution",
        ),
        (
            &trust_root_did,
            &treasury_did,
            "the institution's authority over its treasury",
        ),
    ] {
        let present = graph
            .get_edge(source, target)
            .map_err(|e| anyhow::anyhow!("Verification: could not read a trust edge ({e})"))?
            .is_some();
        if !present {
            bail!("Verification: {what} is not readable back from the trust store.");
        }
    }
    drop(graph);

    // The configuration the daemon will actually parse, resolved through the
    // owner of that rule rather than by re-reading the key here.
    let config_path = data_dir.join("icn.toml");
    let text = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Verification: failed to read {}", config_path.display()))?;
    let cooperative = read_cooperative_section(&text)
        .with_context(|| {
            format!(
                "Verification: failed to parse the [cooperative] section of {}",
                config_path.display()
            )
        })?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Verification: {} has no [cooperative] section, so the daemon would fall back \
                 to the node DID.",
                config_path.display()
            )
        })?;
    let resolved = cooperative
        .resolve_treasury_did(&node_did)
        .context("Verification: the published configuration does not resolve a treasury")?;
    if !resolved.is_institutional() || resolved.did().as_str() != receipt.treasury_did {
        bail!(
            "Verification: the published configuration resolves treasury {:?}, not the {} this \
             ceremony created. The daemon would not consume the institution's treasury.",
            resolved.did().as_str(),
            receipt.treasury_did
        );
    }
    Ok(())
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
            "Refusing institutional genesis: there is no configuration at {} to \
             link the treasury into.\n\
             The daemon resolves its treasury from `[cooperative] \
             treasury_did`, so without this link it would fall back to the node \
             DID — genesis would appear to succeed while the machine remained \
             the treasury. Run `icnctl init-coop` (or create the configuration) \
             and re-run genesis.",
            config_path.display()
        );
    }
    let text = std::fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    // `toml::from_str`, not `str::parse` — in toml 0.9 `FromStr for Value`
    // parses a bare value rather than a document, so a normal config file with
    // a leading comment fails there. This is the parser the rest of the
    // repository uses.
    let parsed: toml::Value = toml::from_str(&text)
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;
    if parsed.get("cooperative").is_some() {
        bail!(
            "Refusing institutional genesis: {} already has a [cooperative] \
             section. It may already name a different treasury; repointing an \
             institution's spend source is an explicit operator decision.",
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

    out.push_str(
        "\n# Written by `icnctl institution genesis` (#2744).\n\
         # `treasury_did` is a keypair-backed DID whose key material lives in\n\
         # `treasury.age`. Without this section the daemon falls back to the\n\
         # node's own DID for governance-authored ledger entries.\n\
         [cooperative]\n",
    );
    out.push_str(&body);

    // Parse before publishing, so a malformed append fails with the original
    // file still in place.
    //
    // Only the section this ceremony owns is validated. Deserializing the whole
    // file as `icn_core::Config` would couple genesis to the entire config
    // schema — and would fail today for a reason that has nothing to do with
    // genesis: `init-coop`'s generated `icn.toml` omits
    // `[network] bootstrap_peers`, which has no serde default, so
    // `Config::from_file` rejects it. That is a separate pre-existing defect
    // and is not this ceremony's to fix or to be blocked by.
    let _ = read_cooperative_section(&out).context(
        "Refusing to publish a configuration whose [cooperative] section does not parse",
    )?;

    let tmp_path = config_path.with_extension("toml.genesis-tmp");

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
        Err(_) => {}
    }
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
            .with_context(|| format!("Failed to create {}", tmp_path.display()))?;
        f.write_all(out.as_bytes())
            .with_context(|| format!("Failed to write {}", tmp_path.display()))?;
        f.sync_all()
            .with_context(|| format!("Failed to flush {}", tmp_path.display()))?;
    }
    std::fs::rename(&tmp_path, &config_path).with_context(|| {
        format!(
            "Failed to publish {} over {}",
            tmp_path.display(),
            config_path.display()
        )
    })?;
    Ok(())
}

/// Read just the `[cooperative]` table, through the type that owns it.
///
/// Scoped deliberately: this ceremony writes one section and should validate
/// exactly that section. See the note in [`publish_cooperative_config`] for why
/// deserializing the whole file is not an option today.
fn read_cooperative_section(text: &str) -> Result<Option<icn_core::config::CooperativeConfig>> {
    let doc: toml::Value = toml::from_str(text).context("configuration is not valid TOML")?;
    match doc.get("cooperative") {
        None => Ok(None),
        Some(section) => {
            Ok(Some(section.clone().try_into().context(
                "[cooperative] section is not a valid CooperativeConfig",
            )?))
        }
    }
}

fn print_receipt(receipt: &GenesisReceipt) {
    println!("Institutional Genesis");
    println!("=====================\n");
    println!("Cooperative:        {}", receipt.cooperative_name);
    println!("  id:               {}", receipt.cooperative_id);
    println!("Institution DID:    {}", receipt.trust_root_did);
    println!("Treasury DID:       {}", receipt.treasury_did);
    println!("Founding authority: {}", receipt.genesis_authority_did);
    println!("Node DID:           {}", receipt.node_did);
    println!("Currency:           {}", receipt.currency);
    println!("Created at:         {}", receipt.created_at);
    println!("Schema version:     {}", receipt.schema_version);
    println!(
        "\nThe treasury is a principal of its own; it is not the node.\n\
         This is Principal-generation runtime genesis. It does not implement\n\
         the GEN protocol (#2602), stable Subject-generation (#2694),\n\
         federation, or the Technical Alpha as a whole."
    );
}

pub fn handle_institution_genesis_command(
    cmd: InstitutionGenesisCommands,
    data_dir: &Path,
) -> Result<()> {
    match cmd {
        InstitutionGenesisCommands::Create {
            name,
            currency,
            yes,
        } => {
            if !yes {
                use std::io::Write;
                println!(
                    "Institutional genesis creates durable cooperative state, a \
                     treasury principal with its own key material, \
                     institution-rooted trust facts, and a genesis receipt \
                     under {}.",
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
            let receipt = run_genesis(data_dir, &name, &currency)?;
            print_receipt(&receipt);
        }
        InstitutionGenesisCommands::Show { json } => match genesis_state(data_dir)? {
            GenesisState::Complete(receipt) => {
                if json {
                    println!("{}", serde_json::to_string_pretty(&receipt)?);
                } else {
                    print_receipt(&receipt);
                }
            }
            GenesisState::Inconsistent { receipt, problem } => bail!(
                "INCONSISTENT genesis under {}: a receipt exists for cooperative \
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
            GenesisState::Incomplete { artefacts } => bail!(
                "INCOMPLETE genesis under {}: artefacts exist but no completion \
                 receipt does, so no institution came into existence here.\n  {}\n\
                 This state cannot be resumed; remove it deliberately before \
                 founding again.",
                data_dir.display(),
                artefacts.join("\n  ")
            ),
            GenesisState::NotStarted => bail!(
                "No genesis receipt under {}. This data directory has not \
                 undergone institutional genesis.",
                data_dir.display()
            ),
        },
    }
    Ok(())
}
