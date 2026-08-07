//! How many unauthenticated connections one source may hold at once (#2547).
//!
//! # What #2491 left open
//!
//! #2491 gave every connection its own anonymous budget, keyed on the `ConnectionContext`
//! so that nothing the sender says can be issued a different one. That closed the
//! identity-ordering defect. It deliberately did not bound *how many* of those budgets one
//! source may hold:
//!
//! ```text
//! attacker opens N connections -> attacker holds N anonymous budgets
//! ```
//!
//! Three facts on the inbound path make that worse than it looks. The handshake semaphore
//! releases its permit as soon as the handshake completes, so it caps concurrent
//! *handshakes* and never the number of established connections. This node installs
//! `keep_alive_interval` on its **server** config, so it PINGs its own inbound connections
//! and an idle one never reaches the idle timeout. And an unauthenticated connection is in
//! no map at all — the session map is populated only by an authenticated Hello (#2530) — so
//! nothing tracks or evicts it.
//!
//! Established-but-unauthenticated connections were therefore unbounded in count and
//! effectively immortal. This module bounds them.
//!
//! # The invariant
//!
//! > At any instant, the number of established-but-unauthenticated inbound connections is
//! > bounded, both server-wide and per source.
//!
//! **Concurrency, not rate.** This says nothing about how fast connections may be opened and
//! closed; see the module's "what this does not bound" note below.
//!
//! **Pre-authentication only.** The reservation is released the moment a connection
//! authenticates, so this never constrains established authenticated peers — a peer that
//! says who it is stops consuming source admission immediately, and is thereafter governed
//! by the per-DID and per-anchor limits (#2490, #2491).
//!
//! # Why the post-handshake remote IP can be trusted as a key
//!
//! Admission is taken **after** the QUIC handshake completes, and that is what makes the
//! address meaningful. Completing a QUIC handshake proves return-routability (RFC 9000 §8):
//! the server sends its handshake packets to the claimed address and the peer must answer
//! from it. A spoofed source address cannot reach this hook at all.
//!
//! So the key is attacker-*chosen* but not attacker-*forgeable*: to present a different one,
//! an attacker must actually receive traffic there. That is precisely the property
//! `NetworkMessage.from` lacked in #2491, and it is why keying on it here is sound while
//! keying on a claimed DID was not.
//!
//! The alternatives aggregate nothing or cost too much:
//!
//! - **remote `SocketAddr`** — a new source port is a new key, so reconnecting mints a fresh
//!   allowance and the bound aggregates nothing. QUIC connection migration can also change
//!   it mid-connection.
//! - **a global cap alone** — bounds total work, but one attacker can consume the entire
//!   allowance and lock every honest peer out. Kept here as a second bound, not the only one.
//!
//! # What this does not bound
//!
//! - Attacks distributed across many source addresses or many /64s. A single node cannot
//!   solve that locally, and nothing here pretends to.
//! - QUIC/TLS handshake work *below* this hook — that is the existing concurrent-handshake
//!   semaphore's job, and it bounds concurrency rather than rate.
//! - Connection *rate*. A source that opens, spends its anonymous burst, and closes stays
//!   within every bound here. Churn is throttled only indirectly, by handshake concurrency
//!   divided by handshake latency.
//! - Authenticated application traffic, which is #2490's and #2491's subject.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};

/// Maximum established-but-unauthenticated inbound connections, server-wide.
///
/// Sized above `MAX_CONCURRENT_INBOUND_HANDSHAKES` (64, in `actor::connection`)
/// so that this bound never shadows it: every handshake the node is willing to run
/// concurrently must have somewhere to land, or the two limits would fight and the symptom
/// would be refused connections under ordinary load rather than under attack.
pub const MAX_PREAUTH_CONNECTIONS_TOTAL: usize = 256;

/// Maximum established-but-unauthenticated inbound connections from one source.
///
/// A peer needs exactly one: connect, Hello, authenticated. This is not a limit on how many
/// connections a source may hold — an authenticated connection has already released its
/// reservation — it is a limit on how many it may hold *while still refusing to say who it
/// is*. Eight leaves room for genuine concurrency (several peers behind one NAT starting at
/// once, a retry racing an original) while keeping the per-source share of
/// [`MAX_PREAUTH_CONNECTIONS_TOTAL`] small enough that one source cannot crowd the table.
///
/// Like the concurrent-handshake cap this sits next to, it is a protocol constant rather
/// than operator configuration. That is a deliberately conservative starting point, not a
/// claim that no deployment will want to tune it.
pub const MAX_PREAUTH_CONNECTIONS_PER_SOURCE: usize = 8;

/// The unit a pre-authentication allowance is charged to.
///
/// IPv4 is keyed on the exact address. IPv6 is keyed on the **/64**, because a single host is
/// routinely assigned an entire /64 and exact-address keying would be defeated by rotating
/// inside it at no cost — the v6 equivalent of picking a new source port. Treating v6
/// structurally rather than as an opaque address matches what this crate already does when
/// it classifies addresses (`handlers::peer_exchange`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceKey {
    /// An exact IPv4 address.
    V4(std::net::Ipv4Addr),
    /// The leading 64 bits of an IPv6 address.
    V6Prefix([u8; 8]),
}

impl SourceKey {
    /// The key a connection from `addr` is charged to.
    pub fn from_addr(addr: SocketAddr) -> Self {
        match addr.ip() {
            IpAddr::V4(ip) => SourceKey::V4(ip),
            IpAddr::V6(ip) => {
                let octets = ip.octets();
                let mut prefix = [0u8; 8];
                prefix.copy_from_slice(&octets[..8]);
                SourceKey::V6Prefix(prefix)
            }
        }
    }
}

/// Why a connection was refused admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionRefusal {
    /// The server-wide pre-authentication allowance is exhausted.
    GlobalLimit,
    /// This source already holds its maximum pre-authentication allowance.
    SourceLimit,
}

impl AdmissionRefusal {
    /// A short, bounded label for logs and metrics.
    ///
    /// Deliberately a fixed set of two strings: it is safe as a metric label precisely
    /// because nothing an attacker sends can add a new value.
    pub fn as_str(self) -> &'static str {
        match self {
            AdmissionRefusal::GlobalLimit => "global_limit",
            AdmissionRefusal::SourceLimit => "source_limit",
        }
    }
}

#[derive(Debug, Default)]
struct AdmissionState {
    /// Live pre-authentication connections per source.
    ///
    /// An entry exists only while its count is non-zero — see [`PreAuthAdmission`] on why
    /// that is what keeps this map from becoming the leak it is meant to prevent.
    per_source: HashMap<SourceKey, usize>,
    /// Live pre-authentication connections, all sources.
    total: usize,
}

/// The pre-authentication connection allowance, shared by the whole node.
///
/// # Why this map cannot be turned into a leak
///
/// A DoS defence that introduces an attacker-controlled unbounded map is not a defence. This
/// one is bounded by construction rather than by policy:
///
/// - an entry is created only by a successful admission, and admission first checks `total`
///   against [`MAX_PREAUTH_CONNECTIONS_TOTAL`];
/// - an entry is removed the instant its count reaches zero;
/// - therefore every entry has at least one live connection behind it, and
///   `per_source.len() <= total <= MAX_PREAUTH_CONNECTIONS_TOTAL` holds at all times.
///
/// There is no expiry timer, no eviction policy and no cleanup task, because there is
/// nothing to expire: the map's size is pinned to the number of connections currently being
/// counted, and those are bounded. Releasing is not a background job that could fall behind —
/// it is [`AdmissionGuard`]'s destructor, so it runs on every exit path including panics.
#[derive(Debug)]
pub struct PreAuthAdmission {
    state: Mutex<AdmissionState>,
    max_total: usize,
    max_per_source: usize,
}

impl PreAuthAdmission {
    /// The node-wide admission table with the default bounds.
    pub fn new() -> Self {
        Self::with_limits(
            MAX_PREAUTH_CONNECTIONS_TOTAL,
            MAX_PREAUTH_CONNECTIONS_PER_SOURCE,
        )
    }

    /// An admission table with explicit bounds.
    ///
    /// Exists so tests can drive the boundary without opening hundreds of real connections.
    pub fn with_limits(max_total: usize, max_per_source: usize) -> Self {
        Self {
            state: Mutex::new(AdmissionState::default()),
            max_total,
            max_per_source,
        }
    }

    /// Reserve a pre-authentication slot for a connection from `addr`.
    ///
    /// Returns the reservation on success. Dropping it releases the slot, so the caller
    /// cannot forget: there is deliberately no `release` free function to leave uncalled.
    pub fn try_admit(
        self: &Arc<Self>,
        addr: SocketAddr,
    ) -> Result<AdmissionGuard, AdmissionRefusal> {
        let key = SourceKey::from_addr(addr);
        let mut state = self.lock_state();

        if state.total >= self.max_total {
            return Err(AdmissionRefusal::GlobalLimit);
        }
        let source_count = state.per_source.entry(key).or_insert(0);
        if *source_count >= self.max_per_source {
            // Leave no zero-valued entry behind: a refused admission must not be able to
            // grow the map, or refusing would itself become the amplification.
            if *source_count == 0 {
                state.per_source.remove(&key);
            }
            return Err(AdmissionRefusal::SourceLimit);
        }
        *source_count += 1;
        state.total += 1;

        Ok(AdmissionGuard {
            admission: Arc::clone(self),
            key,
        })
    }

    /// Live pre-authentication connections, all sources.
    pub fn live_total(&self) -> usize {
        self.lock_state().total
    }

    /// Live pre-authentication connections charged to `addr`'s source.
    pub fn live_for(&self, addr: SocketAddr) -> usize {
        let key = SourceKey::from_addr(addr);
        self.lock_state().per_source.get(&key).copied().unwrap_or(0)
    }

    /// Number of sources currently holding at least one pre-authentication slot.
    ///
    /// This is the map's cardinality, exposed so the bound above can be asserted rather than
    /// asserted-about.
    pub fn tracked_sources(&self) -> usize {
        self.lock_state().per_source.len()
    }

    /// Take the state lock, recovering from a poisoned mutex rather than panicking.
    ///
    /// A panic elsewhere must not turn connection admission into a permanent outage: the
    /// counters are plain integers, so the worst a poisoned lock can mean is that one
    /// release was interrupted, and refusing every future connection would be a far worse
    /// failure than proceeding with a slightly stale count.
    fn lock_state(&self) -> std::sync::MutexGuard<'_, AdmissionState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn release(&self, key: SourceKey) {
        let mut state = self.lock_state();
        state.total = state.total.saturating_sub(1);
        if let Some(count) = state.per_source.get_mut(&key) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                // The entry's whole purpose was to carry a non-zero count. Removing it here
                // is what bounds the map's cardinality by the global limit.
                state.per_source.remove(&key);
            }
        }
    }
}

impl Default for PreAuthAdmission {
    fn default() -> Self {
        Self::new()
    }
}

/// A reserved pre-authentication slot, released when dropped.
///
/// Held by the connection's [`ConnectionContext`](crate::handlers::ConnectionContext) until
/// either the connection authenticates — at which point the context drops it, because an
/// identified peer no longer spends anonymous admission — or the handler task ends and the
/// context is dropped with it. Both paths are the same destructor, so there is no ordering
/// to get wrong and no failure path that leaks a slot.
#[derive(Debug)]
pub struct AdmissionGuard {
    admission: Arc<PreAuthAdmission>,
    key: SourceKey,
}

impl Drop for AdmissionGuard {
    fn drop(&mut self) {
        self.admission.release(self.key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v4(addr: &str) -> SocketAddr {
        format!("{addr}:9000").parse().expect("test address")
    }

    fn v6(addr: &str) -> SocketAddr {
        format!("[{addr}]:9000").parse().expect("test address")
    }

    #[test]
    fn a_source_cannot_exceed_its_share() {
        let admission = Arc::new(PreAuthAdmission::with_limits(100, 3));
        let addr = v4("203.0.113.7");

        let guards: Vec<_> = (0..3)
            .map(|_| admission.try_admit(addr).expect("within the source limit"))
            .collect();
        assert_eq!(admission.live_for(addr), 3);

        assert_eq!(
            admission.try_admit(addr).unwrap_err(),
            AdmissionRefusal::SourceLimit,
            "a fourth slot was issued to a source limited to three"
        );

        drop(guards);
        assert_eq!(admission.live_for(addr), 0);
        assert!(
            admission.try_admit(addr).is_ok(),
            "releasing every slot must make the source admissible again"
        );
    }

    #[test]
    fn one_source_cannot_lock_out_another() {
        let admission = Arc::new(PreAuthAdmission::with_limits(100, 2));
        let noisy = v4("203.0.113.7");
        let honest = v4("198.51.100.9");

        let _held: Vec<_> = (0..2)
            .map(|_| admission.try_admit(noisy).expect("within the source limit"))
            .collect();
        assert_eq!(
            admission.try_admit(noisy).unwrap_err(),
            AdmissionRefusal::SourceLimit
        );

        assert!(
            admission.try_admit(honest).is_ok(),
            "an exhausted source must not deny an unrelated one — that would make the \
             defence itself the outage"
        );
    }

    #[test]
    fn the_global_limit_binds_across_sources() {
        let admission = Arc::new(PreAuthAdmission::with_limits(3, 100));

        let _a = admission.try_admit(v4("203.0.113.1")).expect("first");
        let _b = admission.try_admit(v4("203.0.113.2")).expect("second");
        let _c = admission.try_admit(v4("203.0.113.3")).expect("third");

        assert_eq!(
            admission.try_admit(v4("203.0.113.4")).unwrap_err(),
            AdmissionRefusal::GlobalLimit,
            "the server-wide bound must hold even though no single source exceeded its share"
        );
    }

    #[test]
    fn ipv6_is_aggregated_by_prefix_not_by_address() {
        let admission = Arc::new(PreAuthAdmission::with_limits(100, 2));

        // Three different addresses, one /64. Rotating inside a prefix is free, so it must
        // not buy a fresh allowance.
        let _a = admission.try_admit(v6("2001:db8:0:1::1")).expect("first");
        let _b = admission.try_admit(v6("2001:db8:0:1::2")).expect("second");
        assert_eq!(
            admission.try_admit(v6("2001:db8:0:1::3")).unwrap_err(),
            AdmissionRefusal::SourceLimit,
            "rotating the low 64 bits bought a fresh allowance"
        );

        // A genuinely different /64 is a different source.
        assert!(
            admission.try_admit(v6("2001:db8:0:2::1")).is_ok(),
            "a different /64 must be charged separately"
        );
    }

    /// The map's cardinality is the thing that could turn this defence into the leak it
    /// prevents, so it is asserted directly rather than reasoned about.
    #[test]
    fn the_table_never_outgrows_the_global_limit() {
        let admission = Arc::new(PreAuthAdmission::with_limits(4, 4));

        // Far more distinct sources than the global limit allows.
        let mut guards = Vec::new();
        for i in 0..200u32 {
            let addr = v4(&format!("203.0.113.{}", i % 256));
            if let Ok(guard) = admission.try_admit(addr) {
                guards.push(guard);
            }
        }
        assert_eq!(admission.live_total(), 4);
        assert!(
            admission.tracked_sources() <= 4,
            "the admission table grew past the global limit: {} entries",
            admission.tracked_sources()
        );

        // And a refused admission must not leave an entry behind either.
        drop(guards);
        assert_eq!(admission.live_total(), 0);
        assert_eq!(
            admission.tracked_sources(),
            0,
            "releasing every slot must leave no residue in the table"
        );
    }

    /// A refusal must not itself grow the map — otherwise attacking the limiter would be
    /// cheaper than attacking what it protects.
    #[test]
    fn refusals_leave_no_residue() {
        let admission = Arc::new(PreAuthAdmission::with_limits(1, 1));
        let _held = admission.try_admit(v4("203.0.113.1")).expect("first");

        for i in 0..100u32 {
            let addr = v4(&format!("198.51.100.{}", i % 256));
            assert_eq!(
                admission.try_admit(addr).unwrap_err(),
                AdmissionRefusal::GlobalLimit
            );
        }
        assert_eq!(
            admission.tracked_sources(),
            1,
            "refused admissions accumulated entries in the table"
        );
    }
}
