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
    /// material, the trust facts a governance-authored
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

    /// Read back the genesis receipt, re-verified against durable state.
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
    /// Components exist but no completion receipt does — a ceremony that did
    /// not finish. Carries a component-level report, so a refusal can say what
    /// is actually on disk rather than only that "something" is.
    Incomplete { components: GenesisComponents },
    /// A completion receipt whose claims still hold against durable state.
    ///
    /// "Still hold" means every non-secret relationship: the cooperative record
    /// and its trust-root binding, the treasury registration, both trust facts,
    /// the presence of both keystores, and the configuration linkage. It does
    /// **not** include key provenance — `genesis_state` runs verification
    /// without a passphrase, so a keystore substituted after the ceremony still
    /// classifies as `Complete`. The ceremony itself verifies provenance before
    /// writing the marker; nothing re-verifies it afterwards.
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
pub struct GenesisComponents {
    pub trust_root_key: bool,
    pub treasury_key: bool,
    pub cooperative_record: bool,
    pub treasury_registration: bool,
    pub trust_store_edges: usize,
    pub config_linkage: bool,
    pub receipt: bool,
}

impl GenesisComponents {
    /// True when no **genesis-exclusive** marker is present.
    ///
    /// Only three components are written exclusively by this ceremony, and only
    /// those may be read as evidence that a genesis was attempted here:
    ///
    /// | Component | Exclusive to genesis? |
    /// |---|---|
    /// | `genesis-trust-root.age` | **yes** |
    /// | `treasury.age` | **yes** |
    /// | `genesis:receipt:` rows | **yes** |
    /// | cooperative record | no — the gateway's `CoopManager` creates cooperatives |
    /// | treasury registration | no — reachable from the activation path |
    /// | trust store edges | no — `init-coop` writes `my_did -> member` edges |
    /// | `[cooperative]` config | ambiguous — an operator can hand-write it |
    ///
    /// Counting the shared ones would tell a node that had merely created a
    /// cooperative through the gateway that it holds "incomplete genesis
    /// state", and refuse to found on it. They are still *reported* — they are
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
pub fn genesis_components(data_dir: &Path) -> Result<GenesisComponents> {
    use icn_store::Store;

    let exists = |p: PathBuf| std::fs::symlink_metadata(&p).is_ok();

    let scan_any = |db: PathBuf, prefix: &[u8]| -> bool {
        if !db.exists() {
            return false;
        }
        match icn_store::SledStore::open(&db) {
            Ok(store) => {
                let found = store.scan(prefix).map(|r| !r.is_empty()).unwrap_or(false);
                drop(store);
                found
            }
            Err(_) => false,
        }
    };
    let scan_count = |db: PathBuf, prefix: &[u8]| -> usize {
        if !db.exists() {
            return 0;
        }
        match icn_store::SledStore::open(&db) {
            Ok(store) => {
                let n = store.scan(prefix).map(|r| r.len()).unwrap_or(0);
                drop(store);
                n
            }
            Err(_) => 0,
        }
    };

    let coop_db = coop_db_path(data_dir);
    let config_linkage = {
        let path = data_dir.join("icn.toml");
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| toml::from_str::<toml::Value>(&t).ok())
            .map(|v| v.get("cooperative").is_some())
            .unwrap_or(false)
    };

    Ok(GenesisComponents {
        trust_root_key: exists(trust_root_keystore_path(data_dir)),
        treasury_key: exists(treasury_keystore_path(data_dir)),
        cooperative_record: scan_any(coop_db.clone(), b"coop:"),
        treasury_registration: scan_any(
            icn_core::config::ledger_store_path(data_dir),
            b"ledger:treasury:",
        ),
        trust_store_edges: scan_count(
            icn_core::config::trust_store_path(data_dir),
            b"trust/edges/",
        ),
        config_linkage,
        receipt: scan_any(coop_db, RECEIPT_KEY_PREFIX.as_bytes()),
    })
}

/// Classify the data directory's genesis state.
///
/// **Not side-effect free.** When a receipt exists this runs
/// `verify_durable_state`, which opens the ledger and trust stores
/// unconditionally — and `SledStore::open` is a *creating* open that will
/// `create_dir_all` the path and take an exclusive lock. Inspecting a data
/// directory whose trust store was deleted therefore recreates it empty before
/// reporting INCONSISTENT. It writes no domain row, but it is not read-only,
/// and it needs the daemon stopped.
pub fn genesis_state(data_dir: &Path) -> Result<GenesisState> {
    if let Some(receipt) = load_receipt(data_dir)? {
        // Re-verify rather than trusting the row. This is what makes the
        // receipt evidence: it cannot outlive the cooperative record, the
        // treasury registration, the trust facts, or the configuration linkage
        // that make its claims true. It is also what keeps the commit ordering
        // honest — a receipt written before the configuration was published
        // would surface here as inconsistent, not as a genesis.
        // `None`: a read-only inspection must not prompt for a passphrase.
        return Ok(match verify_durable_state(data_dir, &receipt, None) {
            Ok(()) => GenesisState::Complete(Box::new(receipt)),
            Err(problem) => GenesisState::Inconsistent {
                receipt: Box::new(receipt),
                problem: format!("{problem:#}"),
            },
        });
    }
    // No receipt: report which components a partial ceremony left behind.
    //
    // Keystore presence is probed with `symlink_metadata`, not `exists()`:
    // `exists()` follows symlinks and so reports `false` for a DANGLING one,
    // which would let a rerun create key material *through* that link.
    let components = genesis_components(data_dir)?;
    if components.is_untouched() {
        Ok(GenesisState::NotStarted)
    } else {
        Ok(GenesisState::Incomplete { components })
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
        GenesisState::Incomplete { components } => bail!(
            "Refusing institutional genesis: this data directory holds \
             INCOMPLETE genesis state — a ceremony that did not finish, with no \
             completion receipt:\n  {}\n\
             Genesis never overwrites key material, and it cannot resume: \
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
    Ok(did)
}

/// Read the genesis receipt back out of the cooperative store.
pub fn load_receipt(data_dir: &Path) -> Result<Option<GenesisReceipt>> {
    let coop_db = coop_db_path(data_dir);
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

/// Deterministic fault-injection points, compiled only for tests.
///
/// These exist so that partial-genesis tests exercise the **ceremony's own
/// ordering** rather than a partial state assembled by hand. Constructing the
/// state manually proves the inspector; forcing the real `run_genesis` to stop
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
pub(crate) enum GenesisFailpoint {
    AfterTrustRootKey,
    AfterTreasuryKey,
    AfterCooperativeSave,
    AfterTreasuryRegistration,
    AfterAuthorityEdge,
    AfterTreasuryAuthorityEdge,
    AfterConfigPublish,
}

/// Stop the ceremony at `point` when the test asked for it.
macro_rules! failpoint {
    ($injected:expr, $point:expr) => {
        #[cfg(test)]
        {
            if $injected == Some($point) {
                anyhow::bail!("injected genesis fault at {:?}", $point);
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
/// 2. cross the N2-A startup gate (before any store is opened);
/// 3. refuse if key material already exists, if there is no node keystore to
///    found under, or if the configuration cannot be linked (nothing written);
/// 4. prove founding authority by unlocking the node keystore;
/// 5. refuse if a receipt already exists;
/// 6. mint institution and treasury key material;
/// 7. write the cooperative record;
/// 8. register the treasury durably;
/// 9. write the trust facts;
/// 10. publish the configuration linkage;
/// 11. verify every component through freshly opened handles;
/// 12. write the receipt — **the commit point** — last.
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
    run_genesis_inner(
        data_dir,
        name,
        currency,
        #[cfg(test)]
        None,
    )
}

fn run_genesis_inner(
    data_dir: &Path,
    name: &str,
    currency: &str,
    #[cfg(test)] injected: Option<GenesisFailpoint>,
) -> Result<GenesisReceipt> {
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
    failpoint!(injected, GenesisFailpoint::AfterTrustRootKey);
    let treasury_did = mint_principal(&treasury_keystore_path(data_dir), &passphrase, "treasury")?;
    failpoint!(injected, GenesisFailpoint::AfterTreasuryKey);

    // The invariant this whole issue exists to establish. Asserted rather than
    // assumed: these come from independent `KeyPair::generate()` calls, so an
    // equality here would mean the identity layer itself is broken, and
    // continuing would reintroduce exactly the collapse being fixed.
    if treasury_did == node_did || trust_root_did == node_did {
        bail!(
            "Refusing institutional genesis: a generated institutional \
             principal collided with the node DID ({node_did}). Genesis fails \
             rather than letting the machine stand in for the cooperative."
        );
    }

    let coop_id = format!("coop:{}", uuid::Uuid::new_v4());

    // (7) Durable cooperative record, in the database the daemon opens.
    let coop_db = coop_db_path(data_dir);
    let coop_sled = Arc::new(icn_store::SledStore::open(&coop_db).with_context(|| {
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
    failpoint!(injected, GenesisFailpoint::AfterCooperativeSave);

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
    failpoint!(injected, GenesisFailpoint::AfterTreasuryRegistration);

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
    failpoint!(injected, GenesisFailpoint::AfterAuthorityEdge);
    trust_graph
        .add_edge(icn_trust::TrustEdge::new_typed(
            trust_root_did.clone(),
            treasury_did.clone(),
            full,
            graph_type,
        ))
        .map_err(|e| anyhow::anyhow!("Failed to record the trust root's authority edge: {e}"))?;
    failpoint!(injected, GenesisFailpoint::AfterTreasuryAuthorityEdge);

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
    failpoint!(injected, GenesisFailpoint::AfterConfigPublish);

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
    verify_durable_state(data_dir, &receipt, Some(&passphrase))?;

    // (12) The completion marker, written last and only over verified state.
    {
        use icn_store::Store;
        let coop_db = coop_db_path(data_dir);
        let coop_sled = icn_store::SledStore::open(&coop_db)
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
fn verify_durable_state(
    data_dir: &Path,
    receipt: &GenesisReceipt,
    passphrase: Option<&[u8]>,
) -> Result<()> {
    // The cooperative record, and its link to the treasury.
    let coop_db = coop_db_path(data_dir);
    let coop_sled = Arc::new(
        icn_store::SledStore::open(&coop_db)
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
    for (path, what, expected) in [
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
        if !path.is_file() {
            bail!(
                "Verification: {what} key material is missing from {}. The \
                 principal named by the receipt has no key behind it.",
                path.display()
            );
        }
        if let Some(passphrase) = passphrase {
            let mut keystore = AgeKeyStore::open(&path)
                .with_context(|| format!("Verification: failed to open {}", path.display()))?;
            keystore
                .unlock(passphrase)
                .with_context(|| format!("Verification: failed to unlock {}", path.display()))?;
            let actual = keystore
                .get_keypair()
                .with_context(|| format!("Verification: failed to read the {what} keypair"))?
                .did()
                .clone();
            if actual.as_str() != expected {
                bail!(
                    "Verification: {} holds key material for {}, but the receipt names {}. \
                     Refusing to certify a principal whose key does not back it.",
                    path.display(),
                    actual.as_str(),
                    expected
                );
            }
        }
    }

    // Through the daemon's own loader, not a sub-table read: a receipt that
    // certifies "the daemon will consume this treasury" must be backed by the
    // same parse the daemon performs.
    let config: icn_core::Config = toml::from_str(&text).with_context(|| {
        format!(
            "Verification: {} is not loadable by the daemon, so a genesis linked into it \
             would never be consumed (see icn#2747)",
            config_path.display()
        )
    })?;
    let resolved = config
        .cooperative
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
    if let Err(e) = toml::from_str::<icn_core::Config>(&text) {
        bail!(
            "Refusing institutional genesis: {} is not loadable by the daemon \
             ({e}).\n\
             `icnd --config` parses this file with `Config::from_file`, so a \
             genesis linked into it would never be consumed. If this names a \
             missing `[network] bootstrap_peers`, that is icn#2747 — the \
             configuration `init-coop` generates cannot be loaded by the daemon \
             it tells you to run. Fix the configuration and re-run.",
            config_path.display()
        );
    }
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
        // Created 0600, then widened to the original file's mode after the
        // content is written. The published `icn.toml` may hold a plaintext
        // `jwt_secret`, and creating the temp file at the process umask would
        // expose it world-readable for the duration of the write.
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
        f.sync_all()
            .with_context(|| format!("Failed to flush {}", tmp_path.display()))?;
    }
    // Carry the original file's permissions onto the replacement. `init-coop`'s
    // template invites a plaintext secret into this very file
    // (`# jwt_secret = "CHANGE_ME"  # Set this before starting!`), so a
    // deployment that hardened it to 0600 must not silently get 0644 back from
    // the process umask.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if let Ok(meta) = std::fs::metadata(&config_path) {
            let mode = meta.permissions().mode();
            std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(mode))
                .with_context(|| {
                    format!("Failed to carry permissions onto {}", tmp_path.display())
                })?;
        }
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

/// Print the receipt with an explicit statement of what was actually verified.
///
/// `provenance_verified` separates two genuinely different evidence levels, so
/// the output cannot claim more than the path that produced it checked:
///
/// * the ceremony unlocks both keystores and confirms they derive the recorded
///   DIDs before writing its commit marker;
/// * `show` re-checks every non-secret durable relationship but does not
///   prompt for a passphrase, so it cannot speak to key provenance.
fn print_receipt(receipt: &GenesisReceipt, provenance_verified: bool) {
    println!("Institutional Genesis");
    println!("=====================\n");
    println!("Cooperative:        {}", receipt.cooperative_name);
    println!("  id:               {}", receipt.cooperative_id);
    println!("Genesis trust root: {}", receipt.trust_root_did);
    println!("Treasury DID:       {}", receipt.treasury_did);
    println!("Genesis authority:  {}", receipt.genesis_authority_did);
    println!("Node DID:           {}", receipt.node_did);
    println!("Currency:           {}", receipt.currency);
    println!("Created at:         {}", receipt.created_at);
    println!("Schema version:     {}", receipt.schema_version);
    println!();
    if provenance_verified {
        println!(
            "Verified: durable state, configuration linkage, and key provenance\n\
             (both keystores were unlocked and derive the DIDs above)."
        );
    } else {
        println!(
            "Verified: durable state and configuration linkage.\n\
             NOT re-verified: key provenance. This command does not prompt for a\n\
             passphrase, so it cannot confirm the keystores still derive the DIDs\n\
             above — only that they are present."
        );
    }
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
            print_receipt(&receipt, true);
        }
        InstitutionGenesisCommands::Show { json } => match genesis_state(data_dir)? {
            GenesisState::Complete(receipt) => {
                if json {
                    // Wrap rather than print the receipt bare: a script
                    // consuming this must be able to see which evidence level
                    // produced it, and the human surface already says so. The
                    // receipt keeps its own shape under `receipt`.
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "state": "COMPLETE",
                            "verified": {
                                "durable_state": true,
                                "config_linkage": true,
                                "key_presence": true,
                                "key_provenance": false,
                            },
                            "note": "key provenance is not re-verified by `show`;                                      it does not prompt for a passphrase. The ceremony                                      verifies it before writing the receipt.",
                            "receipt": receipt,
                        }))?
                    );
                } else {
                    print_receipt(&receipt, false);
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
            GenesisState::Incomplete { components } => bail!(
                "INCOMPLETE genesis under {}: components exist but no completion \
                 receipt does, so no cooperative came into existence here.\n  {}\n\
                 This state cannot be resumed; remove it deliberately before \
                 founding again.",
                data_dir.display(),
                components.describe()
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

#[cfg(test)]
mod failpoint_tests {
    //! Crash-boundary evidence produced by the ceremony itself.
    //!
    //! Each test runs the real `run_genesis_inner` and forces it to stop at one
    //! mutation boundary, then asserts the component report matches exactly
    //! what the implementation had written by that point. That is the
    //! difference between proving the inspector and proving the ordering: a
    //! test that deletes rows from a completed genesis cannot tell you whether
    //! the cooperative record lands before or after the treasury registration.
    //!
    //! It also means the ordering is pinned by construction. Reordering two
    //! writes in `run_genesis_inner` changes which components exist at a
    //! boundary and fails the corresponding assertion here.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use icn_identity::AgeKeyStore;

    const PASSPHRASE: &str = "failpoint-fixture-passphrase";

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

    fn run_to(dir: &Path, point: GenesisFailpoint) -> anyhow::Error {
        let err = run_genesis_inner(dir, "Failpoint Cooperative", "HOURS", Some(point))
            .expect_err("the injected fault must stop the ceremony");
        let msg = format!("{err:#}");
        // The ceremony must have stopped at the INJECTED point, not refused
        // during preflight — otherwise the component assertions below would be
        // asserting an untouched directory and proving nothing.
        assert!(
            msg.contains("injected genesis fault"),
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
        let got = genesis_components(dir).unwrap();
        let want = GenesisComponents {
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
        match genesis_state(dir).unwrap() {
            GenesisState::Incomplete { .. } => {}
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
        let coops_before = genesis_components(dir).unwrap();

        let err = run_genesis_inner(dir, "Rerun Attempt", "HOURS", None)
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
            genesis_components(dir).unwrap(),
            "{label}: a refused rerun must not change durable state at all"
        );
    }

    #[test]
    fn boundary_a_after_trust_root_key() {
        let dir = provisioned();
        run_to(dir.path(), GenesisFailpoint::AfterTrustRootKey);
        assert_components(dir.path(), "A", true, false, false, false, 0, false);
        assert_rerun_refuses_without_minting(dir.path(), "A");
    }

    #[test]
    fn boundary_b_after_treasury_key() {
        let dir = provisioned();
        run_to(dir.path(), GenesisFailpoint::AfterTreasuryKey);
        assert_components(dir.path(), "B", true, true, false, false, 0, false);
        assert_rerun_refuses_without_minting(dir.path(), "B");
    }

    #[test]
    fn boundary_c_after_cooperative_save() {
        let dir = provisioned();
        run_to(dir.path(), GenesisFailpoint::AfterCooperativeSave);
        assert_components(dir.path(), "C", true, true, true, false, 0, false);
        assert_rerun_refuses_without_minting(dir.path(), "C");
    }

    #[test]
    fn boundary_d_after_treasury_registration() {
        let dir = provisioned();
        run_to(dir.path(), GenesisFailpoint::AfterTreasuryRegistration);
        assert_components(dir.path(), "D", true, true, true, true, 0, false);
        assert_rerun_refuses_without_minting(dir.path(), "D");
    }

    #[test]
    fn boundary_e_after_authority_edge() {
        let dir = provisioned();
        run_to(dir.path(), GenesisFailpoint::AfterAuthorityEdge);
        assert_components(dir.path(), "E", true, true, true, true, 1, false);
        assert_rerun_refuses_without_minting(dir.path(), "E");
    }

    #[test]
    fn boundary_e2_after_treasury_authority_edge() {
        let dir = provisioned();
        run_to(dir.path(), GenesisFailpoint::AfterTreasuryAuthorityEdge);
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
        run_to(dir.path(), GenesisFailpoint::AfterConfigPublish);
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
                genesis_state(dir.path()).unwrap(),
                GenesisState::Incomplete { .. }
            ),
            "F: a published configuration is not a committed genesis"
        );
        assert_rerun_refuses_without_minting(dir.path(), "F");
    }

    /// A cooperative that already exists — created through the gateway, say —
    /// must not make a data directory look mid-ceremony.
    ///
    /// The gateway's `CoopManager` creates cooperatives, and `init-coop` writes
    /// its own trust edges, so those components are shared rather than
    /// genesis-exclusive. Counting them as evidence of an interrupted ceremony
    /// would refuse genesis on a perfectly ordinary node.
    #[test]
    fn shared_components_do_not_make_a_directory_look_mid_ceremony() {
        let dir = provisioned();

        // A cooperative record and a trust edge, written by something that is
        // not this ceremony.
        let coop_db = icn_core::config::store_path(dir.path()).join("cooperative");
        {
            let sled = std::sync::Arc::new(icn_store::SledStore::open(&coop_db).unwrap());
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

        let components = genesis_components(dir.path()).unwrap();
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
            "shared components must not read as an attempted genesis: {components:?}"
        );
        assert!(
            matches!(genesis_state(dir.path()).unwrap(), GenesisState::NotStarted),
            "a node with pre-existing cooperatives must still be foundable"
        );

        // And genesis must actually proceed on such a node.
        run_genesis_inner(dir.path(), "Founded Anyway", "HOURS", None)
            .expect("genesis must not be refused because a cooperative already existed");
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
            run_genesis_inner(dir.path(), "Provenance Cooperative", "HOURS", None).unwrap();

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
    /// "detectable by `genesis create` refusing a rerun". This test exists to
    /// check that sentence rather than to preserve it, because it conflates two
    /// different refusals: "a genesis already exists here" and "the key on disk
    /// no longer backs the DID the receipt names". Only the second is
    /// provenance detection.
    #[test]
    fn a_rerun_over_a_substituted_keystore_refuses_on_prior_genesis_not_provenance() {
        let dir = provisioned();
        let receipt = run_genesis_inner(dir.path(), "Substitution Coop", "HOURS", None).unwrap();

        let path = treasury_keystore_path(dir.path());
        std::fs::remove_file(&path).unwrap();
        AgeKeyStore::init(&path, PASSPHRASE.as_bytes()).unwrap();

        let err =
            run_genesis_inner(dir.path(), "Rerun", "HOURS", None).expect_err("a rerun must refuse");
        let msg = format!("{err:#}");

        // The refusal is about a prior ceremony, NOT about provenance.
        assert!(
            msg.contains("already undergone genesis"),
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

    /// Without a failpoint the same code path commits, and the state machine
    /// says so. Without this, every assertion above could be satisfied by a
    /// ceremony that never completes at all.
    #[test]
    fn the_uninjected_ceremony_commits() {
        let dir = provisioned();
        let receipt =
            run_genesis_inner(dir.path(), "Committed Cooperative", "HOURS", None).unwrap();
        let components = genesis_components(dir.path()).unwrap();
        assert_eq!(
            components,
            GenesisComponents {
                trust_root_key: true,
                treasury_key: true,
                cooperative_record: true,
                treasury_registration: true,
                trust_store_edges: 2,
                config_linkage: true,
                receipt: true,
            }
        );
        match genesis_state(dir.path()).unwrap() {
            GenesisState::Complete(r) => {
                assert_eq!(r.treasury_did, receipt.treasury_did);
                assert_ne!(r.treasury_did, r.node_did);
                assert_ne!(r.trust_root_did, r.node_did);
                assert_ne!(r.trust_root_did, r.treasury_did);
            }
            other => panic!("expected Complete, got {other:?}"),
        }
    }
}
