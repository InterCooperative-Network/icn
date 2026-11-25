//! P2: the `InstitutionalAuthorityBindingV1` canonical codec and store key (design spec §2.2).
//! EXPERIMENTAL, unratified.
//!
//! The golden vectors were computed by an independent Python model of the spec's byte layout
//! (`hashlib` + `struct`), which first reproduced P1's `served_domain_commitment("coop-a")`
//! vector. They are not derived from this crate's encoder.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use ed25519_dalek::SigningKey;
use icn_authz::effect_v0::{CanonicalV0, ContextInceptionV1};
use icn_governance::domain::{served_domain_commitment, GovernanceDomainId};
use icn_governance::institutional_domain::authority_binding::{
    authority_binding_key, AuthorityBindingCodecError as E, BindingBasisV1, ContextRef,
    InstitutionalAuthorityBindingV1 as Binding, AUTHORITY_BINDING_DOMAIN,
    AUTHORITY_BINDING_KEY_PREFIX, MAX_BINDING_DOMAIN_ID_BYTES,
};
use icn_identity::authority_log::PrincipalKey;
use sha2::{Digest, Sha256};

// Independent golden vectors (see the module docs). Generated mechanically; do not hand-edit.
const GENESIS_LEN: usize = 226;
const GENESIS_HEX: &str = concat!(
    "0000003169636e2e676f7665726e616e63652e696e737469747574696f6e616c",
    "2d617574686f726974792d62696e64696e672e7631000100000006636f6f702d",
    "6111111111111111111111111111111111111111111111111111111111111111",
    "1122222222222222222222222222222222222222222222222222222222222222",
    "22dc0235281c57ac74fa3106a07927ff206c7bfe8117e340501d4a3df30d534e",
    "5202333333333333333333333333333333333333333333333333333333333333",
    "3333444444444444444444444444444444444444444444444444444444444444",
    "4444",
);
const GENESIS_SHA256: &str = "330571b7b18709bc30a8cbf0f9d9ccb275c622a8bb82be9443543f7ac74b8a27";
const CUTOVER_LEN: usize = 236;
const CUTOVER_HEX: &str = concat!(
    "0000003169636e2e676f7665726e616e63652e696e737469747574696f6e616c",
    "2d617574686f726974792d62696e64696e672e7631000100000006636f6f702d",
    "62a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1",
    "a1b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2",
    "b2fd812bc361c3f235a84a912386205a5d64222b35577c60d4425cd5bfecf761",
    "5e0100000006636f6f702d61c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3",
    "c3c3c3c3c3c3c3c3c3c3c3c3d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4d4",
    "d4d4d4d4d4d4d4d4d4d4d4d4",
);
const CUTOVER_SHA256: &str = "3c196078cc6225f5870766f3082af16d943ca6b8799530f6365ea88155c8ebb0";
const KEY_GOLDENS: &[(&str, &str)] = &[
    (
        "coop-a",
        "676f763a696e737469747574696f6e616c2d617574686f726974793a76313a00000006636f6f702d61",
    ),
    (
        "a:b",
        "676f763a696e737469747574696f6e616c2d617574686f726974793a76313a00000003613a62",
    ),
    (
        "caf\u{e9}",
        "676f763a696e737469747574696f6e616c2d617574686f726974793a76313a00000005636166c3a9",
    ),
    (
        "cafe\u{301}",
        "676f763a696e737469747574696f6e616c2d617574686f726974793a76313a0000000663616665cc81",
    ),
];

fn id(s: &str) -> GovernanceDomainId {
    GovernanceDomainId::new(s)
}

fn genesis_golden() -> Binding {
    Binding::new(
        id("coop-a"),
        ContextRef([0x11; 32]),
        [0x22; 32],
        BindingBasisV1::DeclaredGenesis {
            declaration_decision_hash: [0x33; 32],
            mandate_id: [0x44; 32],
        },
    )
    .unwrap()
}

fn cutover_golden() -> Binding {
    Binding::new(
        id("coop-b"),
        ContextRef([0xA1; 32]),
        [0xB2; 32],
        BindingBasisV1::PredecessorCutover {
            predecessor: id("coop-a"),
            decision_hash: [0xC3; 32],
            cutover_commitment: [0xD4; 32],
        },
    )
    .unwrap()
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

// ---- goldens ---------------------------------------------------------------------------------

#[test]
fn declared_genesis_matches_the_independent_golden() {
    let b = genesis_golden();
    let bytes = b.encode();
    assert_eq!(hex::encode(&bytes), GENESIS_HEX);
    assert_eq!(bytes.len(), GENESIS_LEN);
    assert_eq!(sha256_hex(&bytes), GENESIS_SHA256);
    assert_eq!(Binding::decode(&bytes).unwrap(), b);
    assert_eq!(
        b.served_subject(),
        served_domain_commitment(&id("coop-a")).unwrap()
    );
}

#[test]
fn predecessor_cutover_matches_the_independent_golden() {
    let b = cutover_golden();
    let bytes = b.encode();
    assert_eq!(hex::encode(&bytes), CUTOVER_HEX);
    assert_eq!(bytes.len(), CUTOVER_LEN);
    assert_eq!(sha256_hex(&bytes), CUTOVER_SHA256);
    assert_eq!(Binding::decode(&bytes).unwrap(), b);
}

#[test]
fn store_keys_match_the_independent_goldens() {
    for (domain, want) in KEY_GOLDENS {
        assert_eq!(
            hex::encode(authority_binding_key(&id(domain)).unwrap()),
            *want,
            "key for {domain:?}"
        );
    }
}

// ---- store key --------------------------------------------------------------------------------

#[test]
fn store_keys_are_length_framed_and_prefix_free() {
    let ids = [
        "a",
        "a:",
        "a:b",
        "ab",
        "b",
        ":",
        "a\u{0}",
        "caf\u{e9}",
        "cafe\u{301}",
    ];
    let keys: Vec<Vec<u8>> = ids
        .iter()
        .map(|d| authority_binding_key(&id(d)).unwrap())
        .collect();
    for (i, ki) in keys.iter().enumerate() {
        assert!(ki.starts_with(AUTHORITY_BINDING_KEY_PREFIX));
        for (j, kj) in keys.iter().enumerate() {
            if i != j {
                assert!(
                    !kj.starts_with(ki),
                    "key({:?}) is a prefix of key({:?})",
                    ids[i],
                    ids[j]
                );
            }
        }
    }
    // Disjoint from the InstitutionalDomain JSON keyspace and the GovernanceDomain scan prefix.
    assert!(!AUTHORITY_BINDING_KEY_PREFIX.starts_with(b"gov:institutional-domain:"));
    assert!(!AUTHORITY_BINDING_KEY_PREFIX.starts_with(b"gov:domain:"));
}

#[test]
fn store_key_refuses_unbindable_ids() {
    assert!(matches!(
        authority_binding_key(&id("")),
        Err(E::EmptyId { .. })
    ));
    let long = "x".repeat(MAX_BINDING_DOMAIN_ID_BYTES + 1);
    assert!(matches!(
        authority_binding_key(&id(&long)),
        Err(E::IdTooLong { .. })
    ));
}

// ---- construction -----------------------------------------------------------------------------

#[test]
fn construction_refuses_inconsistent_records() {
    let g = genesis_golden();
    let genesis = |ddh: [u8; 32], m: [u8; 32]| BindingBasisV1::DeclaredGenesis {
        declaration_decision_hash: ddh,
        mandate_id: m,
    };
    let cutover = |l: &str, dh: [u8; 32], cc: [u8; 32]| BindingBasisV1::PredecessorCutover {
        predecessor: id(l),
        decision_hash: dh,
        cutover_commitment: cc,
    };
    let c = g.context();
    let h = g.genesis_head();
    let b = g.basis().clone();
    let long = "x".repeat(MAX_BINDING_DOMAIN_ID_BYTES + 1);

    let cases: Vec<(Result<Binding, E>, E)> = vec![
        (
            Binding::new(id(""), c, h, b.clone()),
            E::EmptyId { field: "domain" },
        ),
        (
            Binding::new(id(&long), c, h, b.clone()),
            E::IdTooLong {
                field: "domain",
                len: long.len(),
                max: MAX_BINDING_DOMAIN_ID_BYTES,
            },
        ),
        (
            Binding::new(id("d"), ContextRef([0; 32]), h, b.clone()),
            E::ZeroHash { field: "context" },
        ),
        (
            Binding::new(id("d"), c, [0; 32], b.clone()),
            E::ZeroHash {
                field: "genesis_head",
            },
        ),
        (
            Binding::new(id("d"), c, h, genesis([0; 32], [1; 32])),
            E::ZeroHash {
                field: "declaration_decision_hash",
            },
        ),
        (
            Binding::new(id("d"), c, h, genesis([1; 32], [0; 32])),
            E::ZeroHash {
                field: "mandate_id",
            },
        ),
        (
            Binding::new(id("d"), c, h, cutover("", [1; 32], [1; 32])),
            E::EmptyId {
                field: "predecessor",
            },
        ),
        (
            Binding::new(id("d"), c, h, cutover(&long, [1; 32], [1; 32])),
            E::IdTooLong {
                field: "predecessor",
                len: long.len(),
                max: MAX_BINDING_DOMAIN_ID_BYTES,
            },
        ),
        (
            Binding::new(id("d"), c, h, cutover("d", [1; 32], [1; 32])),
            E::SelfPredecessor,
        ),
        (
            Binding::new(id("d"), c, h, cutover("l", [0; 32], [1; 32])),
            E::ZeroHash {
                field: "decision_hash",
            },
        ),
        (
            Binding::new(id("d"), c, h, cutover("l", [1; 32], [0; 32])),
            E::ZeroHash {
                field: "cutover_commitment",
            },
        ),
    ];
    for (got, want) in cases {
        assert_eq!(got.unwrap_err(), want);
    }
}

#[test]
fn the_id_bound_is_inclusive_and_exact_bytes() {
    let max = "x".repeat(MAX_BINDING_DOMAIN_ID_BYTES);
    let b = Binding::new(
        id(&max),
        ContextRef([1; 32]),
        [2; 32],
        genesis_golden().basis().clone(),
    )
    .unwrap();
    assert_eq!(Binding::decode(&b.encode()).unwrap(), b);
    // No trimming or case folding: these are different domains with different commitments.
    for (a, z) in [
        ("coop-a", "coop-a "),
        ("coop-a", "Coop-A"),
        ("caf\u{e9}", "cafe\u{301}"),
    ] {
        assert_ne!(
            served_domain_commitment(&id(a)).unwrap(),
            served_domain_commitment(&id(z)).unwrap()
        );
    }
}

// ---- strict decoding --------------------------------------------------------------------------

/// Offsets into the genesis golden.
const TAG_LEN_AT: usize = 0;
const TAG_AT: usize = 4;
const VERSION_AT: usize = 4 + 49;
const D_LEN_AT: usize = VERSION_AT + 2;
const D_AT: usize = D_LEN_AT + 4;
const C_AT: usize = D_AT + 6;
const H_AT: usize = C_AT + 32;
const SERVED_AT: usize = H_AT + 32;
const BASIS_AT: usize = SERVED_AT + 32;

fn golden_bytes() -> Vec<u8> {
    hex::decode(GENESIS_HEX).unwrap()
}

fn with(f: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
    let mut v = golden_bytes();
    f(&mut v);
    v
}

#[test]
fn offsets_match_the_layout() {
    let g = golden_bytes();
    assert_eq!(AUTHORITY_BINDING_DOMAIN.len(), 49);
    assert_eq!(&g[TAG_AT..TAG_AT + 49], AUTHORITY_BINDING_DOMAIN);
    assert_eq!(&g[D_AT..D_AT + 6], b"coop-a");
    assert_eq!(g[BASIS_AT], 2);
    assert_eq!(g.len(), BASIS_AT + 1 + 64);
    let _ = TAG_LEN_AT;
}

#[test]
fn decoding_refuses_every_deviation() {
    let served_b = served_domain_commitment(&id("coop-b")).unwrap();
    let cases: Vec<(&str, Vec<u8>, E)> = vec![
        ("retagged", with(|v| v[TAG_AT + 3] ^= 0x01), E::WrongDomain),
        (
            "tag length grown",
            with(|v| v[TAG_LEN_AT + 3] = 50),
            E::WrongDomain,
        ),
        (
            "tag length shrunk",
            with(|v| v[TAG_LEN_AT + 3] = 48),
            E::WrongDomain,
        ),
        (
            "version 0",
            with(|v| v[VERSION_AT + 1] = 0),
            E::UnsupportedVersion(0),
        ),
        (
            "version 2",
            with(|v| v[VERSION_AT + 1] = 2),
            E::UnsupportedVersion(2),
        ),
        (
            "version 0x0101",
            with(|v| v[VERSION_AT] = 1),
            E::UnsupportedVersion(0x0101),
        ),
        (
            "version 0xffff",
            with(|v| {
                v[VERSION_AT] = 0xff;
                v[VERSION_AT + 1] = 0xff
            }),
            E::UnsupportedVersion(0xffff),
        ),
        ("basis 0", with(|v| v[BASIS_AT] = 0), E::UnknownBasis(0)),
        ("basis 3", with(|v| v[BASIS_AT] = 3), E::UnknownBasis(3)),
        (
            "basis 255",
            with(|v| v[BASIS_AT] = 255),
            E::UnknownBasis(255),
        ),
        ("trailing byte", with(|v| v.push(0)), E::TrailingBytes(1)),
        (
            "served of another domain",
            with(|v| v[SERVED_AT..SERVED_AT + 32].copy_from_slice(&served_b)),
            E::ServedMismatch,
        ),
        (
            "served bit flipped",
            with(|v| v[SERVED_AT + 31] ^= 0x80),
            E::ServedMismatch,
        ),
        (
            "zero context",
            with(|v| v[C_AT..C_AT + 32].fill(0)),
            E::ZeroHash { field: "context" },
        ),
        (
            "zero genesis head",
            with(|v| v[H_AT..H_AT + 32].fill(0)),
            E::ZeroHash {
                field: "genesis_head",
            },
        ),
        (
            "invalid utf-8 domain",
            with(|v| v[D_AT] = 0xff),
            E::InvalidUtf8 { field: "domain" },
        ),
        (
            "domain length past the bound",
            with(|v| v[D_LEN_AT..D_LEN_AT + 4].copy_from_slice(&u32::MAX.to_be_bytes())),
            E::IdTooLong {
                field: "domain",
                len: u32::MAX as usize,
                max: MAX_BINDING_DOMAIN_ID_BYTES,
            },
        ),
    ];
    for (name, bytes, want) in cases {
        assert_eq!(Binding::decode(&bytes).unwrap_err(), want, "{name}");
    }
}

#[test]
fn decoding_refuses_an_empty_domain_and_a_self_predecessor() {
    // Re-frame the domain as empty: drop "coop-a" and set its length to 0.
    let empty = with(|v| {
        v[D_LEN_AT + 3] = 0;
        v.drain(D_AT..D_AT + 6);
    });
    assert_eq!(
        Binding::decode(&empty).unwrap_err(),
        E::EmptyId { field: "domain" }
    );
    // A cutover whose predecessor is rewritten to the bound domain itself.
    let mut c = cutover_golden().encode();
    let l_at = c.len() - 64 - 6;
    c[l_at..l_at + 6].copy_from_slice(b"coop-b");
    assert_eq!(Binding::decode(&c).unwrap_err(), E::SelfPredecessor);
}

#[test]
fn every_truncation_is_refused() {
    for g in [golden_bytes(), cutover_golden().encode()] {
        for len in 0..g.len() {
            assert!(
                Binding::decode(&g[..len]).is_err(),
                "truncated to {len} decoded"
            );
        }
    }
}

/// Every single-bit flip of both goldens is either refused or decodes to a binding that
/// re-encodes to exactly the mutated bytes (so it names a different record; nothing aliases).
///
/// Exactly where a flip may survive: inside the opaque 32-byte fields (`C`, `h1`, the two basis
/// hashes), where any non-zero value is a legal different record, and inside the cutover's
/// predecessor id bytes. A flip anywhere else (tag, version, length frames, `D` itself, `served`,
/// basis selector) is always refused.
#[test]
fn exhaustive_single_bit_flips_never_alias_the_original() {
    let mut refused = 0usize;
    let mut distinct = 0usize;
    for original in [golden_bytes(), cutover_golden().encode()] {
        let decoded = Binding::decode(&original).unwrap();
        let hashes_tail = original.len() - 64;
        // Cutover only: `u8 basis || u32 len || "coop-a"` precedes the two hashes.
        let predecessor_id = (BASIS_AT + 5)..hashes_tail;
        let may_survive = |byte: usize| {
            (C_AT..SERVED_AT).contains(&byte)
                || byte >= hashes_tail
                || (original.len() == CUTOVER_LEN && predecessor_id.contains(&byte))
        };
        for i in 0..original.len() * 8 {
            let mut m = original.clone();
            m[i / 8] ^= 1 << (i % 8);
            match Binding::decode(&m) {
                Err(_) => {
                    // A one-bit change cannot zero a non-zero fill byte, so a flip inside an opaque
                    // hash field is never refused.
                    let in_hash = (C_AT..SERVED_AT).contains(&(i / 8)) || i / 8 >= hashes_tail;
                    assert!(!in_hash, "bit {i} in an opaque hash field was refused");
                    refused += 1;
                }
                Ok(b) => {
                    assert!(may_survive(i / 8), "bit {i} (byte {}) was accepted", i / 8);
                    assert_ne!(b, decoded, "bit {i} aliased the original");
                    assert_eq!(b.encode(), m, "bit {i}: non-canonical acceptance");
                    distinct += 1;
                }
            }
        }
    }
    assert_eq!(refused + distinct, (GENESIS_LEN + CUTOVER_LEN) * 8);
}

// ---- identity separation ----------------------------------------------------------------------

/// The binding is its own record kind: neither it nor a V1 inception decodes as the other, and its
/// tag differs from every tag it sits beside.
#[test]
fn binding_and_inception_formats_do_not_cross_decode() {
    let founders: Vec<PrincipalKey> = [0xF1u8, 0xF2, 0xF3]
        .iter()
        .map(|s| {
            PrincipalKey::try_from_verifying_key(SigningKey::from_bytes(&[*s; 32]).verifying_key())
                .unwrap()
        })
        .collect();
    let inception = ContextInceptionV1::new(
        founders,
        2,
        [0x0A; 32],
        served_domain_commitment(&id("coop-a")).unwrap(),
    )
    .unwrap();
    let inception_bytes = inception.canonical_bytes();
    assert_eq!(
        Binding::decode(&inception_bytes).unwrap_err(),
        E::WrongDomain
    );
    assert!(ContextInceptionV1::decode(&genesis_golden().encode()).is_err());
    for other in [
        b"icn.authz.v1.inception".as_slice(),
        b"icn.authz.v0.inception",
        b"icn.authz.v0.context",
        b"icn.governance.served-domain.v1",
    ] {
        assert_ne!(AUTHORITY_BINDING_DOMAIN, other);
    }
}

/// `D`, `C` and `served` are independent inputs: the binding derives none from another, and the
/// same context bound for two domains gives two records with two different commitments.
#[test]
fn domain_context_and_commitment_are_independent_identifiers() {
    let c = ContextRef([0x5A; 32]);
    let basis = genesis_golden().basis().clone();
    let a = Binding::new(id("coop-a"), c, [1; 32], basis.clone()).unwrap();
    let b = Binding::new(id("coop-b"), c, [1; 32], basis).unwrap();
    assert_eq!(a.context(), b.context());
    assert_ne!(a.served_subject(), b.served_subject());
    assert_ne!(a.encode(), b.encode());
    assert_ne!(a.served_subject(), a.context().0);
    // An entity-shaped string is just a domain id here: committed as its exact bytes, not parsed.
    let entity_like = Binding::new(
        id("entity:icn:coop:x"),
        c,
        [1; 32],
        genesis_golden().basis().clone(),
    )
    .unwrap();
    assert_eq!(
        entity_like.served_subject(),
        served_domain_commitment(&id("entity:icn:coop:x")).unwrap()
    );
    assert_ne!(
        entity_like.served_subject(),
        served_domain_commitment(&id("x")).unwrap()
    );
}
