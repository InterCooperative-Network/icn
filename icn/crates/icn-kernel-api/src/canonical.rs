//! Field-wise canonical hashing: the receipt-hash recipe ICN receipts already use.
//!
//! ICN's receipt and proof types (this crate's `proofs.rs`, `icn-governance`'s
//! `proof.rs`) hash their significant fields by hand, each with the same
//! recipe:
//!
//! ```text
//! BLAKE3( domain_tag
//!         || per string or variable-length byte field: u64-LE byte length || the bytes
//!         || per fixed-width field (a digest, a request id): its raw bytes
//!         || per integer: u64-LE )
//! ```
//!
//! in a fixed field order chosen by each record type. [`CanonicalHasher`] is
//! that recipe as one reusable mechanism, byte for byte: it reproduces
//! `ArtifactReceipt::compute_receipt_hash` and the pinned `icn-governance`
//! receipt goldens (see the tests). [`ordered_commitment`] adds a positional
//! commitment to an ordered list of digests.
//!
//! # What this module is not
//!
//! - **Not `CANONICAL_ENCODING.md` §4.1.2 `typed_hash`** (`BLAKE3(domain || 0x00
//!   || CBOR)`). That normative layout is a different byte format, which no
//!   receipt hash implements today. This module codifies the implemented
//!   receipt recipe; reconciling the two is a documentation and governance
//!   decision this module does not make. A domain string must never be used
//!   with both formats.
//! - **Not a schema.** The caller chooses the tag and the field order, and
//!   how optional values are represented. Nothing here encodes an absent
//!   value: a record type that has one must define its own explicit encoding
//!   (for example a presence byte via [`CanonicalHasher::fixed`]).
//! - **Not a signature, an attestation or an authorization.** A matching hash
//!   proves that two parties committed to the same bytes. It does not say who
//!   committed, that anything committed is true, or that anyone may act on it.
//!
//! # Domain separation
//!
//! The tag is hashed first with no separator, so separation relies on the tag
//! itself: tags are non-empty printable ASCII without spaces ([`DomainTag`]),
//! and the tags hashed under this recipe in one deployment must be
//! prefix-free, meaning no tag is a proper prefix of another (check with
//! [`tags_are_prefix_free`]). With a prefix pair, `tag || fields` and
//! `tag' || fields'` can share bytes.

use std::collections::BTreeMap;

use crate::types::Hash;

/// A domain-separation tag: non-empty printable ASCII (0x21..=0x7E), with no
/// spaces or control bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DomainTag(&'static [u8]);

impl DomainTag {
    /// A tag from a byte-string literal or constant.
    ///
    /// Intended for `const` items, where an invalid tag is a compile error
    /// (const evaluation fails). Tags are protocol constants, never input;
    /// for a tag that is not known at compile time, use [`DomainTag::try_new`].
    pub const fn new(tag: &'static [u8]) -> Self {
        match Self::try_new(tag) {
            Some(t) => t,
            None => panic!("a domain tag is non-empty printable ASCII without spaces"),
        }
    }

    /// A tag, or `None` if it is empty or holds a byte outside 0x21..=0x7E.
    pub const fn try_new(tag: &'static [u8]) -> Option<Self> {
        if tag.is_empty() {
            return None;
        }
        let mut i = 0;
        while i < tag.len() {
            if tag[i] < 0x21 || tag[i] > 0x7E {
                return None;
            }
            i += 1;
        }
        Some(Self(tag))
    }

    /// The tag's bytes.
    pub const fn as_bytes(&self) -> &'static [u8] {
        self.0
    }
}

/// `true` if no tag is a proper prefix of another and no tag repeats.
///
/// Use it in a test over every tag a record family hashes under this recipe.
pub fn tags_are_prefix_free(tags: &[DomainTag]) -> bool {
    tags.iter().enumerate().all(|(i, a)| {
        tags.iter()
            .enumerate()
            .all(|(j, b)| i == j || !b.as_bytes().starts_with(a.as_bytes()))
    })
}

/// BLAKE3 over a domain tag and explicitly typed fields (see the module docs).
///
/// Every method appends exactly the bytes its documentation names, in call
/// order. There is no method for an unprefixed variable-length slice: a raw
/// field's width is fixed by its type ([`CanonicalHasher::fixed`],
/// [`CanonicalHasher::digest`]), so adjacent fields cannot trade bytes.
#[derive(Clone, Debug)]
pub struct CanonicalHasher {
    inner: blake3::Hasher,
}

impl CanonicalHasher {
    /// Start a hash with `tag` as its first bytes (no separator).
    pub fn new(tag: DomainTag) -> Self {
        let mut inner = blake3::Hasher::new();
        inner.update(tag.as_bytes());
        Self { inner }
    }

    /// A string: its UTF-8 byte length as u64-LE, then the bytes.
    pub fn str(&mut self, value: &str) -> &mut Self {
        self.bytes(value.as_bytes())
    }

    /// A variable-length byte field: its length as u64-LE, then the bytes.
    pub fn bytes(&mut self, value: &[u8]) -> &mut Self {
        // usize is at most 64 bits on every target ICN builds for: lossless.
        self.inner.update(&(value.len() as u64).to_le_bytes());
        self.inner.update(value);
        self
    }

    /// A fixed-width field: its raw bytes, with no length prefix. The width is
    /// part of the type, so it is the same for every value of the field.
    pub fn fixed<const N: usize>(&mut self, value: &[u8; N]) -> &mut Self {
        self.inner.update(value);
        self
    }

    /// A 32-byte digest: its raw bytes.
    pub fn digest(&mut self, value: &Hash) -> &mut Self {
        self.fixed(value)
    }

    /// An integer: 8 bytes, little-endian.
    pub fn u64(&mut self, value: u64) -> &mut Self {
        self.inner.update(&value.to_le_bytes());
        self
    }

    /// The BLAKE3 hash of everything appended so far.
    pub fn finish(&self) -> Hash {
        *self.inner.finalize().as_bytes()
    }
}

/// Why an ordered commitment was refused. Nothing is hashed on refusal.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommitmentError {
    /// An ordered commitment needs at least one member.
    #[error("an ordered commitment needs at least one member")]
    Empty,
    /// A member repeats an earlier one (positions are 1-based). Repeats are
    /// refused, never removed, so a list cannot be padded into another hash.
    #[error("member {position} repeats member {first}")]
    Duplicate {
        /// The 1-based position of the repeat.
        position: u64,
        /// The 1-based position of its first occurrence.
        first: u64,
    },
}

/// Commit to an ordered list of digests, each bound to its 1-based position:
///
/// ```text
/// BLAKE3( tag || u64-LE(n) || for i in 1..=n: u64-LE(i) || member_i )
/// ```
///
/// Order is part of the commitment: reordering changes the hash. This is
/// **not** ICN's unordered receipt-set layout (`icn-governance`
/// `compute_receipt_set_hash`, which sorts members and hashes no positions),
/// and the two must never be substituted for each other. Use a tag distinct
/// from every record tag.
pub fn ordered_commitment(tag: DomainTag, members: &[Hash]) -> Result<Hash, CommitmentError> {
    if members.is_empty() {
        return Err(CommitmentError::Empty);
    }
    let mut first_seen: BTreeMap<&Hash, u64> = BTreeMap::new();
    let mut hasher = CanonicalHasher::new(tag);
    hasher.u64(members.len() as u64);
    for (position, member) in (1u64..).zip(members) {
        if let Some(&first) = first_seen.get(member) {
            return Err(CommitmentError::Duplicate { position, first });
        }
        first_seen.insert(member, position);
        hasher.u64(position).digest(member);
    }
    Ok(hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expected values in this module were derived independently of this crate:
    /// preimages written by hand from the documented layout, hashed by two
    /// separate BLAKE3 implementations (a pure-Python reference and the
    /// `blake3` Python package). Construction XI evidence:
    /// `independent_vectors.py` / `independent_vectors.json`.
    fn hex32(bytes: &[u8; 32]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    const TEST_TAG: DomainTag = DomainTag::new(b"icn:test:canonical-hasher:v1");
    const ORDERED_TAG: DomainTag = DomainTag::new(b"icn:test:ordered:v1");

    fn m(b: u8) -> Hash {
        [b; 32]
    }

    // --- exact byte-level encoding --------------------------------------------

    #[test]
    fn every_field_kind_encodes_exactly_as_documented() {
        let mut expected = b"icn:test:canonical-hasher:v1".to_vec();
        expected.extend_from_slice(&0u64.to_le_bytes()); // str ""
        expected.extend_from_slice(&2u64.to_le_bytes()); // str "é" is 2 UTF-8 bytes
        expected.extend_from_slice("é".as_bytes());
        expected.extend_from_slice(&2u64.to_le_bytes()); // bytes [0, 1]
        expected.extend_from_slice(&[0x00, 0x01]);
        expected.extend_from_slice(&[0x11; 4]); // fixed::<4>, raw
        expected.extend_from_slice(&[0x22; 32]); // digest, raw
        expected.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]); // u64 0
        expected.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0]); // u64 1, little-endian
        expected.extend_from_slice(&[0xff; 8]); // u64 MAX
        let got = CanonicalHasher::new(TEST_TAG)
            .str("")
            .str("é")
            .bytes(&[0x00, 0x01])
            .fixed(&[0x11; 4])
            .digest(&[0x22; 32])
            .u64(0)
            .u64(1)
            .u64(u64::MAX)
            .finish();
        assert_eq!(got, *blake3::hash(&expected).as_bytes());
        // ... and the independently computed value for those same bytes.
        assert_eq!(
            hex32(&got),
            "4d2945267c56295bc6104e500214adf554e64f66b03a53861174a3bf5e0487ac"
        );
    }

    #[test]
    fn the_tag_alone_is_hashed_with_no_separator() {
        assert_eq!(
            hex32(&CanonicalHasher::new(TEST_TAG).finish()),
            "df6138b325187debbd2a47251a0cb5c3d465e252dec21acc49c64f94079ed47d"
        );
        assert_eq!(
            CanonicalHasher::new(TEST_TAG).finish(),
            *blake3::hash(b"icn:test:canonical-hasher:v1").as_bytes()
        );
    }

    #[test]
    fn length_prefixes_are_u64_little_endian_byte_counts() {
        // 300 bytes: the prefix is 0x2c 0x01 0..., not a character count.
        let s = "é".repeat(150);
        let mut expected = b"icn:test:canonical-hasher:v1".to_vec();
        expected.extend_from_slice(&[0x2c, 0x01, 0, 0, 0, 0, 0, 0]);
        expected.extend_from_slice(s.as_bytes());
        assert_eq!(
            CanonicalHasher::new(TEST_TAG).str(&s).finish(),
            *blake3::hash(&expected).as_bytes()
        );
        // str and bytes encode identically for the same bytes (both length-prefixed).
        assert_eq!(
            CanonicalHasher::new(TEST_TAG).str(&s).finish(),
            CanonicalHasher::new(TEST_TAG).bytes(s.as_bytes()).finish()
        );
    }

    #[test]
    fn length_prefixing_separates_adjacent_variable_fields() {
        let a = CanonicalHasher::new(TEST_TAG).str("ab").str("c").finish();
        let b = CanonicalHasher::new(TEST_TAG).str("a").str("bc").finish();
        assert_ne!(a, b);
        let c = CanonicalHasher::new(TEST_TAG)
            .bytes(&[1, 2])
            .bytes(&[])
            .finish();
        let d = CanonicalHasher::new(TEST_TAG)
            .bytes(&[1])
            .bytes(&[2])
            .finish();
        assert_ne!(c, d);
    }

    #[test]
    fn integers_are_little_endian() {
        let le = CanonicalHasher::new(TEST_TAG)
            .u64(0x0102_0304_0506_0708)
            .finish();
        let mut expected = b"icn:test:canonical-hasher:v1".to_vec();
        expected.extend_from_slice(&[8, 7, 6, 5, 4, 3, 2, 1]);
        assert_eq!(le, *blake3::hash(&expected).as_bytes());
    }

    // --- determinism, order, domain separation --------------------------------

    #[test]
    fn hashing_is_deterministic() {
        let f = || {
            CanonicalHasher::new(TEST_TAG)
                .str("x")
                .u64(7)
                .digest(&m(3))
                .finish()
        };
        assert_eq!(f(), f());
    }

    #[test]
    fn field_order_matters() {
        let a = CanonicalHasher::new(TEST_TAG).str("x").u64(7).finish();
        let b = CanonicalHasher::new(TEST_TAG).u64(7).str("x").finish();
        assert_ne!(a, b);
        let c = CanonicalHasher::new(TEST_TAG)
            .digest(&m(1))
            .digest(&m(2))
            .finish();
        let d = CanonicalHasher::new(TEST_TAG)
            .digest(&m(2))
            .digest(&m(1))
            .finish();
        assert_ne!(c, d);
    }

    #[test]
    fn the_domain_tag_separates_otherwise_identical_inputs() {
        let a = CanonicalHasher::new(TEST_TAG).str("x").finish();
        let b = CanonicalHasher::new(ORDERED_TAG).str("x").finish();
        assert_ne!(a, b);
    }

    #[test]
    fn domain_tags_are_nonempty_printable_ascii_without_spaces() {
        assert!(DomainTag::try_new(b"icn:x:v1").is_some());
        for bad in [
            &b""[..],
            b"icn x",
            b"icn:\x00",
            b"icn:\n",
            b"icn:\xc3\xa9",
            b"icn:\x7f",
        ] {
            assert!(DomainTag::try_new(bad).is_none(), "{bad:?}");
        }
        assert_eq!(TEST_TAG.as_bytes(), b"icn:test:canonical-hasher:v1");
    }

    #[test]
    fn prefix_free_tag_sets_are_detected() {
        let a = DomainTag::new(b"icn:a:v1");
        let b = DomainTag::new(b"icn:a:v1:x");
        let c = DomainTag::new(b"icn:b:v1");
        assert!(tags_are_prefix_free(&[a, c]));
        assert!(!tags_are_prefix_free(&[a, b])); // a is a proper prefix of b
        assert!(!tags_are_prefix_free(&[a, c, a])); // a repeated tag is not separation
    }

    // --- ordered commitment ---------------------------------------------------

    #[test]
    fn ordered_commitment_encodes_count_then_positions_then_members() {
        let mut expected = b"icn:test:ordered:v1".to_vec();
        expected.extend_from_slice(&3u64.to_le_bytes());
        for (i, b) in [(1u64, 1u8), (2, 2), (3, 3)] {
            expected.extend_from_slice(&i.to_le_bytes());
            expected.extend_from_slice(&[b; 32]);
        }
        let got = ordered_commitment(ORDERED_TAG, &[m(1), m(2), m(3)]).unwrap();
        assert_eq!(got, *blake3::hash(&expected).as_bytes());
        assert_eq!(
            hex32(&got),
            "5f1e45df708304c89b65b6037a22fd5187fbd921f3f9ddb0b1cb9527a76c7a05"
        );
    }

    #[test]
    fn ordered_commitment_is_order_sensitive() {
        let swapped = ordered_commitment(ORDERED_TAG, &[m(2), m(1), m(3)]).unwrap();
        assert_eq!(
            hex32(&swapped),
            "d0c9b1dd77263db9f90dc5a09643f772a01ad1dcfa5d96ee62cacfcdeb467a4a"
        );
        assert_ne!(
            swapped,
            ordered_commitment(ORDERED_TAG, &[m(1), m(2), m(3)]).unwrap()
        );
    }

    #[test]
    fn ordered_commitment_of_one_member() {
        assert_eq!(
            hex32(&ordered_commitment(ORDERED_TAG, &[m(1)]).unwrap()),
            "9d275380d7c7852c22ee9b4a61ca415732473c76dd059aac28816813f6c199ec"
        );
    }

    #[test]
    fn ordered_commitment_refuses_empty_and_repeated_members() {
        assert_eq!(
            ordered_commitment(ORDERED_TAG, &[]),
            Err(CommitmentError::Empty)
        );
        assert_eq!(
            ordered_commitment(ORDERED_TAG, &[m(1), m(2), m(1)]),
            Err(CommitmentError::Duplicate {
                position: 3,
                first: 1
            })
        );
        assert_eq!(
            ordered_commitment(ORDERED_TAG, &[m(5), m(5)]),
            Err(CommitmentError::Duplicate {
                position: 2,
                first: 1
            })
        );
    }

    /// ICN's evidence-packet receipt set (icn-governance proof.rs,
    /// `compute_receipt_set_hash`) sorts its members and hashes no positions. It
    /// is a different layout: the same members under the same tag commit to
    /// different bytes, so one must never be substituted for the other.
    #[test]
    fn ordered_commitment_is_not_the_unordered_receipt_set_layout() {
        let unordered = CanonicalHasher::new(ORDERED_TAG)
            .u64(3)
            .digest(&m(1))
            .digest(&m(2))
            .digest(&m(3))
            .finish();
        assert_eq!(
            hex32(&unordered),
            "2383278e8fa0b3f8fa1e2836a83840c96f389da2b72cff1cae2134cb726f0a49"
        );
        assert_ne!(
            unordered,
            ordered_commitment(ORDERED_TAG, &[m(1), m(2), m(3)]).unwrap()
        );
    }

    // --- compatibility with ICN's existing receipt hashes ---------------------

    /// icn-governance/src/proof.rs `evidence_packet_produced_receipt_set_hash_golden_vector`
    /// and `evidence_packet_produced_golden_vector` (ICN 11a5889): the pinned
    /// goldens re-expressed through `CanonicalHasher`. The receipt set is
    /// written out field by field: it is the unordered layout above.
    #[test]
    fn icn_governance_receipt_goldens_reexpressed() {
        let set = CanonicalHasher::new(DomainTag::new(
            b"icn:gov:evidence_packet_produced:receipt_set:v1",
        ))
        .u64(2)
        .digest(&m(4))
        .digest(&m(7))
        .finish();
        assert_eq!(
            hex32(&set),
            "61f4124b565da0be8b090a20ffdccc896b343ad23c4548bb0777af62766f85e7"
        );
        let record = CanonicalHasher::new(DomainTag::new(b"icn:gov:evidence_packet_produced:v1"))
            .str("domain-food-coop")
            .str("session-alpha")
            .str("packet-001")
            .str("application-001")
            .str("did:icn:producer-1")
            .digest(&m(7))
            .digest(&set)
            .digest(&m(5))
            .digest(&m(9))
            .u64(1_750_000_000)
            .finish();
        assert_eq!(
            hex32(&record),
            "c6c74f06499a0a4d8c3a7ff1f3995ad2a88976e7ec1e0a9de9e95a51f53dfe57"
        );
    }

    /// Every receipt/proof tag this crate hashes under the recipe is prefix-free,
    /// so no two of them can share bytes through the tag (they are the real
    /// constants, so a new or renamed tag is checked here too).
    #[test]
    fn this_crates_receipt_tags_are_prefix_free() {
        use crate::proofs::*;
        let tags = [
            ArtifactReceipt::DOMAIN_TAG,
            AntiEntropyProbe::DOMAIN_TAG,
            PeerSyncReport::DOMAIN_TAG,
            SyncDegradedStatus::DOMAIN_TAG,
            FederationSyncWindow::DOMAIN_TAG,
            QuorumSyncCheck::DOMAIN_TAG,
            RoutingProof::DOMAIN_TAG,
            RedundancyProof::DOMAIN_TAG,
            DivergenceEvidence::DOMAIN_TAG,
            RepairPlan::DOMAIN_TAG,
            RepairReceipt::DOMAIN_TAG,
        ]
        .map(|t| DomainTag::try_new(t).expect("a valid tag"));
        assert!(tags_are_prefix_free(&tags));
    }

    /// The same-crate proof that the helper is the recipe: for varied inputs,
    /// `CanonicalHasher` reproduces `ArtifactReceipt::compute_receipt_hash`.
    #[test]
    fn artifact_receipt_hash_reexpressed() {
        use crate::proofs::ArtifactReceipt;
        const TAG: DomainTag = DomainTag::new(ArtifactReceipt::DOMAIN_TAG);
        for (req, blob, requester, provider, scope) in [
            (
                [0xBB; 32],
                [0xAA; 32],
                "did:icn:requester456",
                "did:icn:provider123",
                "coop:test-scope",
            ),
            ([0; 32], [0; 32], "", "", ""),
            ([1; 32], [2; 32], "did:icn:ABC", "did:icn:requester", "XYZ"),
            ([3; 32], [4; 32], "é", "did:icn:ß", "scope with spaces"),
        ] {
            let via_helper = CanonicalHasher::new(TAG)
                .fixed(&req)
                .digest(&blob)
                .str(requester)
                .str(provider)
                .str(scope)
                .finish();
            assert_eq!(
                via_helper,
                ArtifactReceipt::compute_receipt_hash(
                    &req,
                    &blob,
                    &requester.to_string(),
                    &provider.to_string(),
                    scope
                )
            );
        }
    }
}
