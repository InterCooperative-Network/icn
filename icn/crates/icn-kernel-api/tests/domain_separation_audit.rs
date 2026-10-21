//! Workspace domain-separation audit (`CANONICAL_ENCODING.md` §4.1.2, §4.1.3 Rule 1).
//!
//! A domain string separates two hash uses only through the bytes that
//! surround it in the preimage. Whether two domains can produce the same
//! preimage depends on their **framing**, not on whether one string is a prefix
//! of the other:
//!
//! - `domain || 0x00 || data` (a `typed_hash` domain, or a NUL-separated one)
//!   can be a proper prefix of a printable tag without any shared preimage,
//!   because the tag has a printable byte where the domain has 0x00.
//! - `u32be(len) || domain || ...` (the `icn-identity` authority log) puts the
//!   length first, so two such domains of different lengths differ at byte 3.
//! - A §4.1.3 tag is followed directly by field bytes, so a tag that equals or
//!   prefixes another domain can be extended into that domain's preimage.
//!
//! # Model
//!
//! Each registered domain is reduced to its **fixed leading bytes** (the domain
//! plus any separator or length prefix that always surrounds it) and a tail
//! that is either empty ([`Framing::Whole`]) or unconstrained (every other
//! framing). Two domains under the same primitive are separated when their
//! fixed bytes differ at some offset both have, or when the shorter preimage is
//! exactly its fixed bytes. Otherwise the audit reports the pair as able to
//! share a preimage.
//!
//! The model is conservative: the real first field after a tag (often a u64-LE
//! length) is treated as unconstrained, so a flagged pair may in fact be
//! separated. Every flagged pair must therefore be resolved explicitly: listed
//! in [`ACCEPTED`] with its reason, or fixed in the hashing code.
//!
//! # Scope and limits
//!
//! - The registry is an inventory of **leading** domain strings found by a
//!   review of the Rust workspace on 2026-10-21. A domain that does not start
//!   its preimage (a suffix tag, a tag nested after another tag) is not
//!   modelled. Untagged hashes are not modelled at all: they share their
//!   primitive with every domain here, and `CANONICAL_ENCODING.md` §4.1.5 lists
//!   the known ones.
//! - Each entry's first source must contain the domain literal, so a renamed or
//!   deleted domain fails here. The framing is a reviewed claim, not checked
//!   against the code. Byte layouts are pinned by the owning crates' golden
//!   vectors.
//! - [`every_discoverable_icn_domain_is_registered`] ratchets one discoverable
//!   class: `icn`-namespaced literals in a hashing context (see
//!   [`hashing_context`]). Domains outside that class are registered and
//!   checked pairwise, but a new one is not forced into the registry. This is
//!   not an exhaustive proof.
//!
//! Print the full classification of every equal or prefix-related pair with
//! `cargo test -p icn-kernel-api --test domain_separation_audit -- --ignored --nocapture`.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use Framing::*;
use Primitive::*;
use Scope::*;

/// The function or construction the preimage is fed to. Preimages under
/// different primitives never collide with each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Primitive {
    Blake3,
    Sha256,
    Sha3_256,
    /// A message that is signed rather than hashed (Ed25519, ML-DSA).
    SignatureMessage,
    HmacSha3_256,
    HkdfInfo,
    HkdfSalt,
}

impl Primitive {
    /// Text that a source naming this primitive contains (lowercase).
    fn markers(self) -> &'static [&'static str] {
        match self {
            Blake3 => &["blake3"],
            Sha256 => &["sha256", "sha-256"],
            Sha3_256 => &["sha3"],
            SignatureMessage => &["sign"],
            HmacSha3_256 => &["hmac"],
            HkdfInfo | HkdfSalt => &["hkdf"],
        }
    }
}

/// What surrounds the domain bytes in the preimage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Framing {
    /// `domain || ...` with no separator: §4.1.3 field-wise tags, strings that
    /// end in their own separator (`steward:`), literals with a built-in 0x00,
    /// signing contexts, and the fixed prefix of a domain built at runtime.
    Raw,
    /// `domain || 0x00 || ...`, with the 0x00 written separately (§4.1.2).
    Nul,
    /// `u32be(len(domain)) || domain || ...`.
    U32BeLen,
    /// `u32le(len(domain)) || domain || ...`.
    U32LeLen,
    /// The domain bytes are the entire preimage.
    Whole,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    Production,
    TestOnly,
    /// Declared in `CANONICAL_ENCODING.md`; no code hashes it (v1.1).
    SpecOnly,
}

#[derive(Clone, Copy, Debug)]
struct Domain {
    bytes: &'static [u8],
    primitive: Primitive,
    framing: Framing,
    scope: Scope,
    /// Paths from the repository root. The first defines the domain literal;
    /// any others hash it.
    sources: &'static [&'static str],
    /// Distinguishes two layouts registered under one domain.
    layout: &'static str,
}

const fn d(
    bytes: &'static [u8],
    primitive: Primitive,
    framing: Framing,
    scope: Scope,
    sources: &'static [&'static str],
) -> Domain {
    dn(bytes, primitive, framing, scope, sources, "")
}

const fn dn(
    bytes: &'static [u8],
    primitive: Primitive,
    framing: Framing,
    scope: Scope,
    sources: &'static [&'static str],
    layout: &'static str,
) -> Domain {
    Domain {
        bytes,
        primitive,
        framing,
        scope,
        sources,
        layout,
    }
}

impl Domain {
    /// The bytes every preimage under this domain starts with.
    fn fixed(&self) -> Vec<u8> {
        let len = u32::try_from(self.bytes.len()).expect("a short domain");
        let mut out = Vec::new();
        match self.framing {
            U32BeLen => out.extend_from_slice(&len.to_be_bytes()),
            U32LeLen => out.extend_from_slice(&len.to_le_bytes()),
            Raw | Nul | Whole => {}
        }
        out.extend_from_slice(self.bytes);
        if self.framing == Nul {
            out.push(0x00);
        }
        out
    }

    /// How the literal is spelled in Rust source: printable ASCII as is, 0x00
    /// as `\0`, and a lone non-printable byte as its hex value (`0x10`).
    fn needle(&self) -> String {
        if let [b] = self.bytes {
            if !b.is_ascii_graphic() {
                return format!("0x{b:02x}");
            }
        }
        self.bytes
            .iter()
            .map(|&b| match b {
                0x00 => "\\0".to_string(),
                b => char::from(b).to_string(),
            })
            .collect()
    }

    fn id(&self) -> (&'static [u8], Primitive, Framing, &'static str) {
        (self.bytes, self.primitive, self.framing, self.layout)
    }
}

impl fmt::Display for Domain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} [{:?}, {:?}, {:?}",
            String::from_utf8_lossy(self.bytes),
            self.primitive,
            self.framing,
            self.scope
        )?;
        if !self.layout.is_empty() {
            write!(f, ", layout: {}", self.layout)?;
        }
        write!(f, "] at {}", self.sources.join(", "))
    }
}

/// Whether two domains can produce the same preimage, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Verdict {
    /// Different primitives: the preimages are never compared.
    DistinctPrimitive,
    /// The fixed bytes differ at this offset, which both preimages have.
    Divergent { offset: usize },
    /// The shorter preimage is exactly its fixed bytes, which end before the
    /// longer domain's fixed bytes do.
    Terminated,
    /// A preimage under one domain can equal a preimage under the other.
    Reachable(&'static str),
}

impl Verdict {
    fn separated(&self) -> bool {
        !matches!(self, Verdict::Reachable(_))
    }
}

/// The audit's rule: see the module docs.
fn classify(a: &Domain, b: &Domain) -> Verdict {
    if a.primitive != b.primitive {
        return Verdict::DistinctPrimitive;
    }
    let (fa, fb) = (a.fixed(), b.fixed());
    let common = fa.iter().zip(&fb).take_while(|(x, y)| x == y).count();
    if common < fa.len() && common < fb.len() {
        return Verdict::Divergent { offset: common };
    }
    let shorter = if fa.len() <= fb.len() { a } else { b };
    if fa.len() == fb.len() {
        return Verdict::Reachable("both start with the same fixed bytes");
    }
    if shorter.framing == Whole {
        Verdict::Terminated
    } else {
        Verdict::Reachable("the shorter domain's fixed bytes are followed by unconstrained bytes")
    }
}

/// A flagged pair accepted with a reason. Each side is (domain, layout).
struct Accepted {
    a: (&'static [u8], &'static str),
    b: (&'static [u8], &'static str),
    reason: &'static str,
}

const RECEIPT_CHAIN: &str =
    "receipt chain: prior digest, then length-prefixed ids, then postcard(body)";
const PROJECTOR: &str = "projector: postcard(value)";
const TWO_LAYOUTS: &str = "test-only icn-baseline-lock fixture; recorded in CANONICAL_ENCODING.md \
    §4.1.3 'Two layouts under one tag' and frozen under Rule 2";

const ACCEPTED: &[Accepted] = &[
    Accepted {
        a: (b"icn:baseline:deliberation_entry:v1", RECEIPT_CHAIN),
        b: (b"icn:baseline:deliberation_entry:v1", PROJECTOR),
        reason: TWO_LAYOUTS,
    },
    Accepted {
        a: (b"icn:baseline:notice_delivered:v1", RECEIPT_CHAIN),
        b: (b"icn:baseline:notice_delivered:v1", PROJECTOR),
        reason: TWO_LAYOUTS,
    },
    Accepted {
        a: (b"icn:baseline:standing_snapshot:v1", RECEIPT_CHAIN),
        b: (b"icn:baseline:standing_snapshot:v1", PROJECTOR),
        reason: TWO_LAYOUTS,
    },
];

/// Literals the discovery scan finds that are deliberately not registered.
/// Each is (literal, file under the repository root, reason).
const NOT_REGISTERED: &[(&str, &str, &str)] = &[
    ("icn:x:v1", CANONICAL_RS, CANONICAL_FIXTURE),
    ("icn:a:v1", CANONICAL_RS, CANONICAL_FIXTURE),
    ("icn:a:v1:x", CANONICAL_RS, CANONICAL_FIXTURE),
    ("icn:b:v1", CANONICAL_RS, CANONICAL_FIXTURE),
    (
        "icn:test:canonical-hasher:v1",
        CANONICAL_RS,
        CANONICAL_FIXTURE,
    ),
    ("icn:test:ordered:v1", CANONICAL_RS, CANONICAL_FIXTURE),
];

const CANONICAL_RS: &str = "icn/crates/icn-kernel-api/src/canonical.rs";
const CANONICAL_FIXTURE: &str = "a unit-test input of canonical.rs, which builds prefix pairs \
    and collisions on purpose to test the helpers";

/// Every leading domain string found in the workspace, one entry per
/// (domain, primitive, framing, layout). Sorted by domain.
#[rustfmt::skip]
const REGISTRY: &[Domain] = &[
    d(b"1.0.0", Sha256, Raw, Production, &["icn/apps/trust-app/src/reducer.rs"]),
    d(b"DID-PQ-BINDING-V1", SignatureMessage, Raw, Production, &["icn/crates/icn-net/src/protocol.rs"]),
    d(b"ICN-GOV-OP-v1\0", Sha256, Raw, Production, &["icn/crates/icn-governance/src/replication.rs"]),
    d(b"ICN-GOV-OP-v1\0", SignatureMessage, Raw, Production, &["icn/crates/icn-governance/src/replication.rs"]),
    d(b"ICN-GOV-STATIC-MEMBERSHIP-v1\0", Sha256, Raw, Production, &["icn/crates/icn-governance/src/replication.rs"]),
    d(b"ICN-HYBRID-KEM:", HkdfInfo, Raw, Production, &["icn/crates/icn-net/src/encryption.rs", "icn/crates/icn-crypto-pq/src/kdf.rs"]),
    d(b"ICN-NONCE-V1", Sha256, Raw, Production, &["icn/crates/icn-net/src/encryption.rs"]),
    d(b"ICN-PAYLOAD-ENCRYPTION-V1", Sha256, Raw, Production, &["icn/crates/icn-net/src/encryption.rs"]),
    d(b"ICN_ADD_DEVICE:", SignatureMessage, Raw, Production, &["icn/crates/icn-gateway/src/identity_mgr.rs"]),
    d(b"ICN_RECOVERY_ATTESTATION:", SignatureMessage, Raw, Production, &["icn/crates/icn-identity/src/recovery.rs"]),
    d(b"ICN_REVOKE_DEVICE:", SignatureMessage, Raw, Production, &["icn/crates/icn-gateway/src/identity_mgr.rs"]),
    d(b"OUT_OF_FUEL", Blake3, Whole, Production, &["icn/crates/icn-compute/src/result_quorum.rs"]),
    d(b"TIMEOUT", Blake3, Whole, Production, &["icn/crates/icn-compute/src/result_quorum.rs"]),
    d(b"\x10", Blake3, Raw, Production, &["icn/crates/icn-authz/src/model/hash.rs"]),
    d(b"\x11", Blake3, Raw, Production, &["icn/crates/icn-authz/src/model/hash.rs"]),
    d(b"\x12", Blake3, Raw, Production, &["icn/crates/icn-authz/src/model/hash.rs"]),
    d(b"\x13", Blake3, Raw, Production, &["icn/crates/icn-authz/src/model/hash.rs"]),
    d(b"\x14", Blake3, Raw, Production, &["icn/crates/icn-authz/src/model/hash.rs"]),
    d(b"\x15", Blake3, Raw, Production, &["icn/crates/icn-authz/src/model/hash.rs"]),
    d(b"\x16", Blake3, Raw, Production, &["icn/crates/icn-authz/src/model/hash.rs"]),
    d(b"bootstrap:", Sha256, Raw, Production, &["icn/crates/icn-net/src/actor/mod.rs"]),
    d(b"charter-sign:", SignatureMessage, Raw, Production, &["icn/bins/icnctl/src/main.rs"]),
    d(b"complete:", SignatureMessage, Raw, Production, &["icn/crates/icn-gateway/src/api/sdis/simple_enrollment.rs", "icn/crates/icn-gateway/tests/fp01_credential_holder_escalation.rs"]),
    d(b"federation:clearing:", Sha256, Raw, Production, &["icn/crates/icn-core/src/services/federation_service.rs"]),
    d(b"federation:join:", Sha256, Raw, Production, &["icn/crates/icn-core/src/services/federation_service.rs"]),
    d(b"federation:leave:", Sha256, Raw, Production, &["icn/crates/icn-core/src/services/federation_service.rs"]),
    d(b"federation:revoke_vouch:", Sha256, Raw, Production, &["icn/crates/icn-core/src/services/federation_service.rs"]),
    d(b"federation:settle:", Sha256, Raw, Production, &["icn/crates/icn-core/src/services/federation_service.rs"]),
    d(b"federation:terminate_clearing:", Sha256, Raw, Production, &["icn/crates/icn-core/src/services/federation_service.rs"]),
    d(b"federation:vouch:", Sha256, Raw, Production, &["icn/crates/icn-core/src/services/federation_service.rs"]),
    d(b"federation_accept:", SignatureMessage, Raw, Production, &["icn/crates/icn-federation/src/gossip.rs"]),
    d(b"force_close", Sha256, Raw, Production, &["icn/crates/icn-core/src/services/control_service.rs"]),
    d(b"gateway-dev-placeholder-key:", Sha256, Raw, Production, &["icn/crates/icn-commons/src/inner.rs"]),
    d(b"gateway-enrollment-vui:", Sha256, Raw, Production, &["icn/crates/icn-commons/src/inner.rs"]),
    d(b"gov:action-item:", Blake3, Raw, Production, &["icn/apps/governance/src/actor.rs"]),
    d(b"icn-accumulator-element", Sha3_256, Raw, Production, &["icn/crates/icn-zkp/src/accumulator.rs"]),
    d(b"icn-anchor-v1", Sha256, Raw, Production, &["icn/crates/icn-identity/src/anchor.rs"]),
    d(b"icn-blind-commitment", Sha3_256, Raw, Production, &["icn/crates/icn-crypto-pq/src/blind.rs"]),
    d(b"icn-blind-hash", Sha3_256, Raw, Production, &["icn/crates/icn-crypto-pq/src/blind.rs"]),
    d(b"icn-blind-sig-v1", HmacSha3_256, Raw, Production, &["icn/crates/icn-steward/src/enrollment.rs"]),
    d(b"icn-enrollment-ack-v1", SignatureMessage, Raw, Production, &["icn/crates/icn-steward/src/gossip.rs"]),
    d(b"icn-enrollment-ceremony-v1", Sha256, Raw, Production, &["icn/crates/icn-steward/src/ceremony/enrollment.rs"]),
    d(b"icn-enrollment-complete-v1", SignatureMessage, Raw, Production, &["icn/crates/icn-steward/src/gossip.rs"]),
    d(b"icn-enrollment-failed-v1", SignatureMessage, Raw, Production, &["icn/crates/icn-steward/src/gossip.rs"]),
    d(b"icn-enrollment-participation-v1", SignatureMessage, Raw, Production, &["icn/crates/icn-steward/src/gossip.rs"]),
    d(b"icn-enrollment-prf-delivery-v1", SignatureMessage, Raw, Production, &["icn/crates/icn-steward/src/gossip.rs"]),
    d(b"icn-enrollment-token-v1", Sha256, Raw, Production, &["icn/crates/icn-steward/src/token.rs"]),
    d(b"icn-enrollment-token-v1", SignatureMessage, Raw, Production, &["icn/crates/icn-steward/src/token.rs"]),
    d(b"icn-enrollment-uniqueness-check-v1", SignatureMessage, Raw, Production, &["icn/crates/icn-steward/src/gossip.rs"]),
    d(b"icn-enrollment-uniqueness-response-v1", SignatureMessage, Raw, Production, &["icn/crates/icn-steward/src/gossip.rs"]),
    d(b"icn-federation:action:v1", Blake3, Nul, SpecOnly, &["docs/architecture/CANONICAL_ENCODING.md"]),
    d(b"icn-federation:constitution:v1", Blake3, Nul, SpecOnly, &["docs/architecture/CANONICAL_ENCODING.md"]),
    d(b"icn-federation:governance-proof:v1", Blake3, Nul, SpecOnly, &["docs/architecture/CANONICAL_ENCODING.md"]),
    d(b"icn-federation:state-root:v1", Blake3, Nul, SpecOnly, &["docs/architecture/CANONICAL_ENCODING.md"]),
    d(b"icn-governance:proposal:v1", Blake3, Nul, SpecOnly, &["docs/architecture/CANONICAL_ENCODING.md"]),
    d(b"icn-governance:vote:v1", Blake3, Nul, SpecOnly, &["docs/architecture/CANONICAL_ENCODING.md"]),
    d(b"icn-id-data-v1", Sha256, Raw, Production, &["icn/crates/icn-identity/src/vui.rs"]),
    d(b"icn-ledger:journal-entry:v1", Blake3, Nul, SpecOnly, &["docs/architecture/CANONICAL_ENCODING.md"]),
    d(b"icn-ledger:settlement:v1", Blake3, Nul, SpecOnly, &["docs/architecture/CANONICAL_ENCODING.md"]),
    d(b"icn-ledger:settlement:v1:", Sha256, Raw, Production, &["icn/crates/icn-ledger/src/settlement.rs"]),
    d(b"icn-merkle-accumulator-element", Blake3, Raw, Production, &["icn/crates/icn-zkp/src/merkle_accumulator.rs"]),
    d(b"icn-merkle-accumulator-node", Blake3, Raw, Production, &["icn/crates/icn-zkp/src/merkle_accumulator.rs"]),
    d(b"icn-ml-dsa-key-v1", HkdfSalt, Whole, Production, &["icn/crates/icn-crypto-pq/src/ml_dsa.rs"]),
    d(b"icn-pop-attestation-v1", Sha256, Raw, Production, &["icn/crates/icn-identity/src/personhood.rs"]),
    d(b"icn-receipt-attester-v1:", SignatureMessage, Raw, Production, &["icn/crates/icn-compute/src/receipt.rs"]),
    d(b"icn-receipt-executor-v1:", SignatureMessage, Raw, Production, &["icn/crates/icn-compute/src/receipt.rs"]),
    d(b"icn-receipt-submitter-v1:", SignatureMessage, Raw, Production, &["icn/crates/icn-compute/src/receipt.rs"]),
    d(b"icn-recovery-attestation-v1", SignatureMessage, Raw, Production, &["icn/crates/icn-steward/src/gossip.rs"]),
    d(b"icn-recovery-ceremony-v1", Sha256, Raw, Production, &["icn/crates/icn-steward/src/ceremony/recovery.rs"]),
    d(b"icn-recovery-complete-v1", SignatureMessage, Raw, Production, &["icn/crates/icn-steward/src/gossip.rs"]),
    d(b"icn-recovery-evidence", Sha256, Raw, Production, &["icn/crates/icn-steward/src/recovery.rs"]),
    d(b"icn-recovery-proof-v1", Sha256, Raw, Production, &["icn/crates/icn-steward/src/recovery.rs"]),
    d(b"icn-recovery-request-v1", SignatureMessage, Raw, Production, &["icn/crates/icn-steward/src/gossip.rs"]),
    d(b"icn-sdis-ed25519-v", HkdfInfo, Raw, Production, &["icn/crates/icn-crypto-pq/src/kdf.rs"]),
    d(b"icn-sdis-ml-dsa-v", HkdfInfo, Raw, Production, &["icn/crates/icn-crypto-pq/src/kdf.rs"]),
    d(b"icn-sdis-x25519-v", HkdfInfo, Raw, Production, &["icn/crates/icn-crypto-pq/src/kdf.rs"]),
    d(b"icn-steward-announce-v1", SignatureMessage, Raw, Production, &["icn/crates/icn-steward/src/gossip.rs"]),
    d(b"icn-steward-newvui-v1", SignatureMessage, Raw, Production, &["icn/crates/icn-steward/src/gossip.rs"]),
    d(b"icn-token-request-v1", Sha3_256, Raw, Production, &["icn/crates/icn-steward/src/enrollment.rs"]),
    d(b"icn-vui-checkpoint-v1", Sha256, Raw, Production, &["icn/crates/icn-steward/src/vui_registry.rs"]),
    d(b"icn-vui-commitment", Sha256, Raw, Production, &["icn/crates/icn-identity/src/anchor.rs", "icn/crates/icn-identity/src/vui.rs"]),
    d(b"icn-vui-commitment-v1", Sha3_256, Raw, Production, &["icn/crates/icn-steward/src/enrollment.rs"]),
    d(b"icn-vui-hash", Sha256, Raw, Production, &["icn/crates/icn-steward/src/recovery.rs"]),
    d(b"icn.authority-log", Sha256, U32BeLen, Production, &["icn/crates/icn-identity/src/authority_log/mod.rs", "icn/crates/icn-identity/src/authority_log/body.rs", "icn/crates/icn-identity/tests/authority_log_derived.rs", "icn/crates/icn-identity/tests/reference/gen_subject_context_reference.py"]),
    d(b"icn.authority-log.commit", Sha256, U32BeLen, Production, &["icn/crates/icn-identity/src/authority_log/mod.rs", "icn/crates/icn-identity/src/authority_log/derive.rs", "icn/crates/icn-identity/tests/reference/gen_subject_context_reference.py"]),
    d(b"icn.authority-log.kdf", Sha256, U32BeLen, Production, &["icn/crates/icn-identity/src/authority_log/mod.rs", "icn/crates/icn-identity/src/authority_log/construct.rs", "icn/crates/icn-identity/tests/reference/gen_subject_context_reference.py"]),
    d(b"icn.authority-log.sig", SignatureMessage, U32BeLen, Production, &["icn/crates/icn-identity/src/authority_log/mod.rs", "icn/crates/icn-identity/src/authority_log/body.rs", "icn/crates/icn-identity/src/authority_log/construct.rs", "icn/crates/icn-identity/src/authority_log/admission.rs", "icn/crates/icn-identity/tests/authority_log_adversarial_orders.rs", "icn/crates/icn-identity/tests/authority_log_derived.rs", "icn/crates/icn-identity/tests/authority_log_durable.rs", "icn/crates/icn-identity/tests/gen_subject_context.rs", "icn/crates/icn-identity/tests/authority_log_encoding.rs"]),
    d(b"icn.gen.initial-device-binding-ref", Sha256, U32BeLen, Production, &["icn/crates/icn-identity/src/subject_context.rs", "icn/crates/icn-identity/tests/reference/gen_subject_context_reference.py"]),
    d(b"icn.gen.subject-context", Sha256, U32BeLen, Production, &["icn/crates/icn-identity/src/subject_context.rs", "icn/crates/icn-identity/tests/reference/gen_subject_context_reference.py"]),
    d(b"icn.gen.subject-context-ref", Sha256, U32BeLen, Production, &["icn/crates/icn-identity/src/subject_context.rs", "icn/crates/icn-identity/tests/reference/gen_subject_context_reference.py"]),
    d(b"icn:anti-entropy-probe:v1", Blake3, Raw, Production, &["icn/crates/icn-kernel-api/src/proofs.rs"]),
    d(b"icn:artifact-receipt:v1", Blake3, Raw, Production, &["icn/crates/icn-kernel-api/src/proofs.rs", "icn/crates/icn-kernel-api/src/canonical.rs"]),
    d(b"icn:baseline:agreement_ctx:fixture", Blake3, Whole, TestOnly, &["icn/crates/icn-baseline-lock/src/projector.rs"]),
    d(b"icn:baseline:allocation_requested:v1", Blake3, Raw, TestOnly, &["icn/crates/icn-baseline-lock/src/receipt_types.rs"]),
    d(b"icn:baseline:authority_ctx:fixture", Blake3, Whole, TestOnly, &["icn/crates/icn-baseline-lock/src/projector.rs"]),
    d(b"icn:baseline:canonical_facts:v1", Blake3, Raw, TestOnly, &["icn/crates/icn-baseline-lock/src/projector.rs"]),
    dn(b"icn:baseline:deliberation_entry:v1", Blake3, Raw, TestOnly, &["icn/crates/icn-baseline-lock/src/receipt_types.rs"], "receipt chain: prior digest, then length-prefixed ids, then postcard(body)"),
    dn(b"icn:baseline:deliberation_entry:v1", Blake3, Raw, TestOnly, &["icn/crates/icn-baseline-lock/src/receipt_types.rs", "icn/crates/icn-baseline-lock/src/projector.rs"], "projector: postcard(value)"),
    dn(b"icn:baseline:notice_delivered:v1", Blake3, Raw, TestOnly, &["icn/crates/icn-baseline-lock/src/receipt_types.rs"], "receipt chain: prior digest, then length-prefixed ids, then postcard(body)"),
    dn(b"icn:baseline:notice_delivered:v1", Blake3, Raw, TestOnly, &["icn/crates/icn-baseline-lock/src/receipt_types.rs", "icn/crates/icn-baseline-lock/src/projector.rs"], "projector: postcard(value)"),
    d(b"icn:baseline:process_gate_result:v1", Blake3, Raw, TestOnly, &["icn/crates/icn-baseline-lock/src/receipt_emit.rs"]),
    d(b"icn:baseline:process_session_opened:v1", Blake3, Raw, TestOnly, &["icn/crates/icn-baseline-lock/src/receipt_types.rs"]),
    d(b"icn:baseline:reservation_state:v1", Blake3, Raw, TestOnly, &["icn/crates/icn-baseline-lock/src/receipt_types.rs"]),
    dn(b"icn:baseline:standing_snapshot:v1", Blake3, Raw, TestOnly, &["icn/crates/icn-baseline-lock/src/receipt_types.rs"], "receipt chain: prior digest, then length-prefixed ids, then postcard(body)"),
    dn(b"icn:baseline:standing_snapshot:v1", Blake3, Raw, TestOnly, &["icn/crates/icn-baseline-lock/src/receipt_types.rs", "icn/crates/icn-baseline-lock/src/projector.rs"], "projector: postcard(value)"),
    d(b"icn:baseline:state_resolution_capsule:v1", Blake3, Raw, TestOnly, &["icn/crates/icn-baseline-lock/src/capsule.rs"]),
    d(b"icn:bond:payable:v1\0", Sha256, Raw, Production, &["icn/crates/icn-core/src/services/ledger_service.rs"]),
    d(b"icn:budget:liability:v1\0", Sha256, Raw, Production, &["icn/crates/icn-core/src/services/ledger_service.rs"]),
    d(b"icn:community:action_completion:v0", Blake3, Raw, TestOnly, &["icn/crates/icn-gateway/src/authority.rs"]),
    d(b"icn:compute-payment-dedup:v1:", Sha256, Raw, Production, &["icn/bins/icnd/src/compute_wiring.rs"]),
    d(b"icn:coop-entity-surrogate:v1", Sha256, Nul, Production, &["icn/crates/icn-entity/src/coop_entity_surrogate.rs"]),
    d(b"icn:divergence-evidence:v1", Blake3, Raw, Production, &["icn/crates/icn-kernel-api/src/proofs.rs"]),
    d(b"icn:federation-sync-window:v1", Blake3, Raw, Production, &["icn/crates/icn-kernel-api/src/proofs.rs"]),
    d(b"icn:gov:action_item_completion:v1", Blake3, Raw, Production, &["icn/crates/icn-governance/src/proof.rs"]),
    d(b"icn:gov:action_item_completion:v2", Blake3, Raw, Production, &["icn/crates/icn-governance/src/proof.rs"]),
    d(b"icn:gov:activation_crossed:v1", Blake3, Raw, Production, &["icn/crates/icn-governance/src/proof.rs"]),
    d(b"icn:gov:activation_gate_basis:v1", Blake3, Raw, Production, &["icn/crates/icn-governance/src/proof.rs"]),
    d(b"icn:gov:attest:v1", SignatureMessage, Raw, Production, &["icn/crates/icn-governance/src/proof.rs"]),
    d(b"icn:gov:decision:v1", Blake3, Raw, Production, &["icn/crates/icn-governance/src/proof.rs"]),
    d(b"icn:gov:decision:v2", Blake3, Raw, Production, &["icn/crates/icn-governance/src/proof.rs"]),
    d(b"icn:gov:decision:v3", Blake3, Raw, Production, &["icn/crates/icn-governance/src/proof.rs"]),
    d(b"icn:gov:decision_recorded:v1", Blake3, Raw, Production, &["icn/crates/icn-governance/src/proof.rs"]),
    d(b"icn:gov:deliberation_entry_recorded:v1", Blake3, Raw, Production, &["icn/crates/icn-governance/src/proof.rs"]),
    d(b"icn:gov:evidence_packet_export_prepared:v1", Blake3, Raw, Production, &["icn/crates/icn-governance/src/proof.rs"]),
    d(b"icn:gov:evidence_packet_made_available:v1", Blake3, Raw, Production, &["icn/crates/icn-governance/src/proof.rs"]),
    d(b"icn:gov:evidence_packet_produced:receipt_set:v1", Blake3, Raw, Production, &["icn/crates/icn-governance/src/proof.rs", "icn/crates/icn-kernel-api/src/canonical.rs"]),
    d(b"icn:gov:evidence_packet_produced:v1", Blake3, Raw, Production, &["icn/crates/icn-governance/src/proof.rs", "icn/crates/icn-kernel-api/src/canonical.rs"]),
    d(b"icn:gov:mandate_grant_ref:v1", Blake3, Raw, Production, &["icn/crates/icn-governance/src/proof.rs"]),
    d(b"icn:gov:meeting_attendance:v1", Blake3, Raw, Production, &["icn/crates/icn-governance/src/proof.rs"]),
    d(b"icn:gov:meeting_attendance:v2", Blake3, Raw, Production, &["icn/crates/icn-governance/src/proof.rs"]),
    d(b"icn:gov:mutation_applied:v1", Blake3, Raw, Production, &["icn/crates/icn-governance/src/proof.rs"]),
    d(b"icn:gov:mutation_plan_recorded:v1", Blake3, Raw, Production, &["icn/crates/icn-governance/src/proof.rs"]),
    d(b"icn:gov:process_gate_result:v1", Blake3, Raw, Production, &["icn/crates/icn-governance/src/proof.rs"]),
    d(b"icn:gov:process_session_opened:v1", Blake3, Raw, Production, &["icn/crates/icn-governance/src/proof.rs"]),
    d(b"icn:gov:rehearsal_apply_result:v1", Blake3, Raw, Production, &["icn/apps/governance/src/rehearsal_workspace.rs"]),
    d(b"icn:gov:rehearsal_plan:v1", Blake3, Raw, Production, &["icn/apps/governance/src/rehearsal_workspace.rs"]),
    d(b"icn:gov:rehearsal_review_decision:v1", Blake3, Raw, Production, &["icn/apps/governance/src/rehearsal_workspace.rs"]),
    d(b"icn:gov:rehearsal_workflow_evidence:v1", Blake3, Raw, Production, &["icn/apps/governance/src/rehearsal_workspace.rs"]),
    d(b"icn:governance-proof:v1", Blake3, Raw, Production, &["icn/crates/icn-governance/src/proof.rs"]),
    d(b"icn:naming:delete:v1", SignatureMessage, U32LeLen, Production, &["icn/crates/icn-naming/src/lib.rs"]),
    d(b"icn:naming:register:v1", SignatureMessage, U32LeLen, Production, &["icn/crates/icn-naming/src/lib.rs"]),
    d(b"icn:naming:update:v1", SignatureMessage, U32LeLen, Production, &["icn/crates/icn-naming/src/lib.rs"]),
    d(b"icn:peer-sync-report:v1", Blake3, Raw, Production, &["icn/crates/icn-kernel-api/src/proofs.rs"]),
    d(b"icn:quorum-sync-check:v1", Blake3, Raw, Production, &["icn/crates/icn-kernel-api/src/proofs.rs"]),
    d(b"icn:redundancy-proof:v1", Blake3, Raw, Production, &["icn/crates/icn-kernel-api/src/proofs.rs"]),
    d(b"icn:repair-plan:v1", Blake3, Raw, Production, &["icn/crates/icn-kernel-api/src/proofs.rs"]),
    d(b"icn:repair-receipt:v1", Blake3, Raw, Production, &["icn/crates/icn-kernel-api/src/proofs.rs"]),
    d(b"icn:routing-proof:v1", Blake3, Raw, Production, &["icn/crates/icn-kernel-api/src/proofs.rs"]),
    d(b"icn:svc-response:v1", SignatureMessage, U32LeLen, Production, &["icn/crates/icn-gossip/src/service_discovery.rs"]),
    d(b"icn:sync-degraded-status:v1", Blake3, Raw, Production, &["icn/crates/icn-kernel-api/src/proofs.rs"]),
    d(b"icn:treasury:", Blake3, Raw, Production, &["icn/apps/membership/src/coop_core/lifecycle.rs", "icn/crates/icn-coop/src/lifecycle.rs"]),
    d(b"icn:vote-hash:v1", Blake3, Raw, Production, &["icn/crates/icn-governance/src/proof.rs"]),
    d(b"member_reload:", Sha256, Raw, TestOnly, &["icn/crates/icn-core/tests/federated_two_node_pilot.rs"]),
    d(b"membership:v2:", Sha256, Raw, Production, &["icn/crates/icn-core/src/services/membership_service.rs"]),
    d(b"ml-dsa-65-keygen", HkdfInfo, Raw, Production, &["icn/crates/icn-crypto-pq/src/ml_dsa.rs"]),
    d(b"protocol:set_parameter:", Sha256, Raw, Production, &["icn/crates/icn-core/src/supervisor/governance_executor.rs"]),
    d(b"revocation:", Sha256, Raw, Production, &["icn/crates/icn-identity/src/revocation.rs"]),
    d(b"sdis:appoint:", Sha256, Raw, Production, &["icn/crates/icn-core/src/services/sdis_service.rs"]),
    d(b"sdis:reconfirm:", Sha256, Raw, Production, &["icn/crates/icn-core/src/services/sdis_service.rs"]),
    d(b"sdis:reinstate:", Sha256, Raw, Production, &["icn/crates/icn-core/src/services/sdis_service.rs"]),
    d(b"sdis:revoke:", Sha256, Raw, Production, &["icn/crates/icn-core/src/services/sdis_service.rs"]),
    d(b"sdis:sanction:", Sha256, Raw, Production, &["icn/crates/icn-core/src/services/sdis_service.rs"]),
    d(b"sdis:suspend:", Sha256, Raw, Production, &["icn/crates/icn-core/src/services/sdis_service.rs"]),
    d(b"steward:", Sha256, Raw, Production, &["icn/crates/icn-governance/src/steward.rs"]),
    d(b"veto", Sha256, Raw, Production, &["icn/crates/icn-core/src/services/control_service.rs"]),
];
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("the repository root")
}

fn read(path: &str) -> String {
    let full = repo_root().join(path);
    fs::read_to_string(&full).unwrap_or_else(|e| panic!("reading {}: {e}", full.display()))
}

/// The reason `ACCEPTED` gives for this pair, if it lists it.
fn accepted_reason(a: &Domain, b: &Domain) -> Option<&'static str> {
    let (ka, kb) = ((a.bytes, a.layout), (b.bytes, b.layout));
    ACCEPTED
        .iter()
        .find(|x| ka != kb && [x.a, x.b].contains(&ka) && [x.a, x.b].contains(&kb))
        .map(|x| x.reason)
}

fn registry_entry(bytes: &[u8], layout: &str) -> Option<&'static Domain> {
    REGISTRY
        .iter()
        .find(|e| e.bytes == bytes && e.layout == layout)
}

/// Whether a line hashes or defines a domain: a hasher update, a `DomainTag`
/// or `CanonicalHasher` constructor, or a `const` whose name contains
/// `DOMAIN`, `TAG` or `CONTEXT`. A const whose value wraps to the next line
/// makes that next line a hashing context too.
fn hashing_context(line: &str, previous: &str) -> bool {
    fn domain_const(line: &str) -> bool {
        let Some(at) = line.find("const ") else {
            return false;
        };
        let rest = &line[at + "const ".len()..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_')
            .collect();
        rest[name.len()..].trim_start().starts_with(':')
            && ["DOMAIN", "TAG", "CONTEXT"]
                .iter()
                .any(|k| name.contains(k))
    }
    [
        ".update(",
        "DomainTag::new(",
        "DomainTag::try_new(",
        "CanonicalHasher::new(",
    ]
    .iter()
    .any(|k| line.contains(k))
        || domain_const(line)
        || (domain_const(previous) && previous.trim_end().ends_with('='))
}

/// The contents of each string literal on a line, as spelled in source.
fn string_literals(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c != '"' {
            continue;
        }
        let mut lit = String::new();
        while let Some(c) = chars.next() {
            match c {
                '"' => break,
                '\\' => {
                    lit.push(c);
                    if let Some(escaped) = chars.next() {
                        lit.push(escaped);
                    }
                }
                c => lit.push(c),
            }
        }
        out.push(lit);
    }
    out
}

fn icn_namespaced(lit: &str) -> bool {
    let bytes = lit.as_bytes();
    bytes.len() > 4
        && (bytes.starts_with(b"icn") || bytes.starts_with(b"ICN"))
        && matches!(bytes[3], b':' | b'-' | b'.' | b'_')
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("listing {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n != "target") {
                rust_sources(&path, out);
            }
        } else if path.extension().is_some_and(|x| x == "rs") {
            out.push(path);
        }
    }
}

/// Every `icn`-namespaced literal in a hashing context, as (literal, file).
fn discover() -> Vec<(String, String)> {
    let root = repo_root();
    let this_file = Path::new(file!()).file_name().expect("this file's name");
    let mut files = Vec::new();
    for top in ["icn/crates", "icn/apps", "icn/bins"] {
        rust_sources(&root.join(top), &mut files);
    }
    let mut found = Vec::new();
    for path in files {
        if path.file_name() == Some(this_file) {
            continue;
        }
        let rel = path
            .strip_prefix(&root)
            .expect("under the root")
            .to_string_lossy()
            .into_owned();
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {rel}: {e}"));
        let mut previous = "";
        for line in text.lines() {
            if !line.trim_start().starts_with("//") && hashing_context(line, previous) {
                for lit in string_literals(line)
                    .into_iter()
                    .filter(|l| icn_namespaced(l))
                {
                    found.push((lit, rel.clone()));
                }
            }
            previous = line;
        }
    }
    found.sort();
    found.dedup();
    found
}

#[test]
fn registry_entries_are_unique_and_anchored_in_their_sources() {
    let mut problems = Vec::new();
    for (i, e) in REGISTRY.iter().enumerate() {
        if REGISTRY[..i].iter().any(|o| o.id() == e.id()) {
            problems.push(format!("{e}: registered twice"));
        }
        let Some(first) = e.sources.first() else {
            problems.push(format!("{e}: no source"));
            continue;
        };
        let texts: Vec<String> = e.sources.iter().map(|s| read(s)).collect();
        if !texts[0].contains(&e.needle()) {
            problems.push(format!(
                "{e}: the defining source {first} no longer contains `{}`",
                e.needle()
            ));
        }
        if !texts.iter().any(|t| {
            let t = t.to_lowercase();
            e.primitive.markers().iter().any(|m| t.contains(m))
        }) {
            problems.push(format!(
                "{e}: no source mentions {:?}, so the registered primitive is unsupported",
                e.primitive
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn registered_domains_are_separated_by_their_framing() {
    let mut violations = Vec::new();
    for (i, a) in REGISTRY.iter().enumerate() {
        for b in &REGISTRY[i + 1..] {
            if let Verdict::Reachable(why) = classify(a, b) {
                if accepted_reason(a, b).is_none() {
                    violations.push(format!(
                        "NOT SEPARATED: {why}\n  - {a}\n  - {b}\n  Fix the hashing code, or add \
                         the pair to ACCEPTED with the reason it is safe."
                    ));
                }
            }
        }
    }
    for x in ACCEPTED {
        match (registry_entry(x.a.0, x.a.1), registry_entry(x.b.0, x.b.1)) {
            (Some(a), Some(b)) if !classify(a, b).separated() => {}
            (Some(a), Some(b)) => violations.push(format!(
                "STALE ACCEPTANCE: the pair is now {:?}; remove it from ACCEPTED\n  - {a}\n  - {b}",
                classify(a, b)
            )),
            _ => violations.push(format!(
                "STALE ACCEPTANCE: {:?} / {:?} is not in the registry",
                String::from_utf8_lossy(x.a.0),
                String::from_utf8_lossy(x.b.0)
            )),
        }
    }
    assert!(violations.is_empty(), "{}", violations.join("\n\n"));
}

#[test]
fn every_discoverable_icn_domain_is_registered() {
    let found = discover();
    let mut problems = Vec::new();
    for (lit, file) in &found {
        let registered = REGISTRY.iter().any(|e| &e.needle() == lit);
        let exempt = NOT_REGISTERED.iter().any(|(l, f, _)| l == lit && f == file);
        if !registered && !exempt {
            problems.push(format!(
                "UNREGISTERED: `{lit}` in {file}. Add it to REGISTRY with its primitive and \
                 framing (or to NOT_REGISTERED with a reason)."
            ));
        }
    }
    for (lit, file, _) in NOT_REGISTERED {
        if !found.iter().any(|(l, f)| l == lit && f == file) {
            problems.push(format!(
                "STALE EXEMPTION: `{lit}` is no longer found in {file}"
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The five string-level proper-prefix pairs noted when `CanonicalHasher` was
/// introduced (#2835), classified one by one under their actual framing.
#[test]
fn the_five_string_prefix_pairs_are_separated_by_their_framing() {
    let get = |bytes: &[u8]| {
        REGISTRY
            .iter()
            .find(|e| e.bytes == bytes)
            .unwrap_or_else(|| panic!("{:?} is registered", String::from_utf8_lossy(bytes)))
    };
    let cases: [(&[u8], &[u8], Verdict); 5] = [
        // SHA-256 (icn-identity) against SHA3-256 (icn-steward).
        (
            b"icn-vui-commitment",
            b"icn-vui-commitment-v1",
            Verdict::DistinctPrimitive,
        ),
        // u32be(17) against u32be(24): byte 3 is 0x11 against 0x18.
        (
            b"icn.authority-log",
            b"icn.authority-log.commit",
            Verdict::Divergent { offset: 3 },
        ),
        // u32be(17) against u32be(21).
        (
            b"icn.authority-log",
            b"icn.authority-log.kdf",
            Verdict::Divergent { offset: 3 },
        ),
        // `.sig` is a signed message, never hashed.
        (
            b"icn.authority-log",
            b"icn.authority-log.sig",
            Verdict::DistinctPrimitive,
        ),
        // u32be(23) against u32be(27).
        (
            b"icn.gen.subject-context",
            b"icn.gen.subject-context-ref",
            Verdict::Divergent { offset: 3 },
        ),
    ];
    for (a, b, expected) in cases {
        let (a, b) = (get(a), get(b));
        assert!(b.bytes.starts_with(a.bytes) && a.bytes != b.bytes);
        assert_eq!(classify(a, b), expected, "\n  - {a}\n  - {b}");
    }
    // Under one primitive, the same `.sig` framing would still diverge at byte 3.
    let sig = get(b"icn.authority-log.sig");
    let as_hash = Domain {
        primitive: Sha256,
        ..*sig
    };
    assert_eq!(
        classify(get(b"icn.authority-log"), &as_hash),
        Verdict::Divergent { offset: 3 }
    );
}

#[test]
fn classify_follows_the_framing_rules() {
    let raw = |b: &'static [u8]| d(b, Blake3, Raw, TestOnly, &[]);
    let nul = |b: &'static [u8]| d(b, Blake3, Nul, TestOnly, &[]);
    let whole = |b: &'static [u8]| d(b, Blake3, Whole, TestOnly, &[]);
    let be = |b: &'static [u8]| d(b, Blake3, U32BeLen, TestOnly, &[]);
    let reachable = |v: Verdict| !v.separated();

    // A tag that prefixes a NUL-separated domain can be extended into it
    // (the collision `canonical.rs` demonstrates). The reverse is separated:
    // the NUL-separated domain has 0x00 where the tag has a printable byte.
    assert!(reachable(classify(
        &raw(b"icn:x:v1"),
        &nul(b"icn:x:v1:ext")
    )));
    assert_eq!(
        classify(&nul(b"icn:x:v1"), &raw(b"icn:x:v1:ext")),
        Verdict::Divergent { offset: 8 }
    );
    // Equal domains under different framings.
    assert!(reachable(classify(&raw(b"icn:x:v1"), &nul(b"icn:x:v1"))));
    assert_eq!(
        classify(&nul(b"icn:x:v1"), &be(b"icn:x:v1")),
        Verdict::Divergent { offset: 0 }
    );
    // Two raw tags in a prefix relation share bytes; two NUL-separated
    // domains never do.
    assert!(reachable(classify(&raw(b"icn:a:v1"), &raw(b"icn:a:v1:x"))));
    assert_eq!(
        classify(&nul(b"icn:a:v1"), &nul(b"icn:a:v1:x")),
        Verdict::Divergent { offset: 8 }
    );
    // A whole-message domain ends before a longer domain's bytes do, but a
    // raw domain equal to it can still have an empty tail.
    assert_eq!(
        classify(&whole(b"TIMEOUT"), &raw(b"TIMEOUT:x")),
        Verdict::Terminated
    );
    assert!(reachable(classify(&whole(b"TIMEOUT"), &raw(b"TIMEOUT"))));
    assert!(reachable(classify(&raw(b"TIME"), &whole(b"TIMEOUT"))));
    // Different primitives never share a preimage space.
    assert_eq!(
        classify(
            &raw(b"icn:x:v1"),
            &d(b"icn:x:v1", Sha256, Raw, TestOnly, &[])
        ),
        Verdict::DistinctPrimitive
    );
}

/// Every equal or prefix-related pair in the registry, with its verdict, and
/// the registered domains the discovery scan does not reach.
#[test]
#[ignore = "report: run with --ignored --nocapture"]
fn print_domain_separation_report() {
    println!("{} registered domains\n", REGISTRY.len());
    for (i, a) in REGISTRY.iter().enumerate() {
        for b in &REGISTRY[i + 1..] {
            if a.bytes.starts_with(b.bytes) || b.bytes.starts_with(a.bytes) {
                let verdict = classify(a, b);
                let status = if verdict.separated() {
                    "separated".to_string()
                } else {
                    accepted_reason(a, b)
                        .map_or("NOT SEPARATED".to_string(), |r| format!("accepted ({r})"))
                };
                println!("{status}: {verdict:?}\n  - {a}\n  - {b}\n");
            }
        }
    }
    let found = discover();
    let unreached: Vec<_> = REGISTRY
        .iter()
        .filter(|e| !found.iter().any(|(l, _)| *l == e.needle()))
        .map(|e| String::from_utf8_lossy(e.bytes).into_owned())
        .collect();
    println!("{} literals discovered in a hashing context", found.len());
    println!(
        "{} registered domains the discovery scan does not reach (coverage gap): {unreached:?}",
        unreached.len()
    );
}
