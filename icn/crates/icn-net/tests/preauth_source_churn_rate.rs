#![allow(clippy::unwrap_used, clippy::expect_used)]
//! How fast one source may keep taking fresh pre-authentication allowances (#2549).
//!
//! #2547 bounds how many unauthenticated connections one source may hold at once, and #2552
//! bounds how long each may be held. Neither bounded how fast a source may take them:
//!
//! ```text
//! connect  -> admitted, slot taken
//! close    -> slot released, map entry removed
//! connect  -> admitted again, from a table with no memory of the previous cycle
//! ```
//!
//! Every cycle stayed inside both existing bounds — concurrency never rose, no deadline was
//! ever reached — and every cycle was issued a fresh `PreAuthRateLimiter` with a full
//! 20-message anonymous burst (#2491).
//!
//! # The property under test
//!
//! > A source may take at most `PREAUTH_CHURN_BUDGET_PER_SOURCE` admissions that end without
//! > ever authenticating per `PREAUTH_CHURN_REFILL_WINDOW`, however quickly it releases them.
//!
//! # How these tests know
//!
//! The observable is the admission table's own accounting, never wall-clock throughput: a rate
//! defect proved by a benchmark is proved only on the machine that ran it. Refill is driven
//! through `try_admit_at`, so every assertion sits at an exact point on the curve instead of
//! being slept towards.
//!
//! [`churn_that_authenticates_is_not_charged`] is the positive control that matters most here.
//! The bound is only worth having if it leaves honest peers alone, and the case it must not
//! break — many peers behind one NAT reconnecting at once — is exactly the case that looks
//! like an attack from the outside.

use icn_net::{
    AdmissionRefusal, PreAuthAdmission, MAX_PREAUTH_CONNECTIONS_PER_SOURCE,
    MAX_PREAUTH_CONNECTIONS_TOTAL, PREAUTH_AUTHENTICATION_DEADLINE,
    PREAUTH_CHURN_BUDGET_PER_SOURCE, PREAUTH_CHURN_REFILL_WINDOW,
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

fn source(addr: &str) -> SocketAddr {
    format!("{addr}:9000").parse().expect("test address")
}

/// The production policy, so these tests pin the shipped numbers rather than a fixture.
///
/// A `#[cfg(test)]` table configured with its own limits can pass while the constants the node
/// actually runs with say something else.
fn node_policy() -> Arc<PreAuthAdmission> {
    Arc::new(PreAuthAdmission::with_churn_policy(
        MAX_PREAUTH_CONNECTIONS_TOTAL,
        MAX_PREAUTH_CONNECTIONS_PER_SOURCE,
        PREAUTH_AUTHENTICATION_DEADLINE,
        PREAUTH_CHURN_BUDGET_PER_SOURCE,
        PREAUTH_CHURN_REFILL_WINDOW,
    ))
}

/// Take a slot and abandon it, exactly as a connection that never authenticates does.
fn churn_once(admission: &Arc<PreAuthAdmission>, addr: SocketAddr) -> Result<(), AdmissionRefusal> {
    admission.try_admit(addr).map(drop)
}

/// T1 — the same source cannot cycle indefinitely.
///
/// The pre-fix control was this loop running ten thousand times without a refusal. It now stops
/// at the budget, and the refusal names the bound that stopped it rather than being
/// indistinguishable from the concurrency limit.
#[test]
fn one_source_cannot_churn_beyond_its_budget() {
    let admission = node_policy();
    let addr = source("203.0.113.7");

    for cycle in 0..PREAUTH_CHURN_BUDGET_PER_SOURCE {
        churn_once(&admission, addr)
            .unwrap_or_else(|refusal| panic!("cycle {cycle} within budget refused: {refusal:?}"));
    }

    assert_eq!(
        churn_once(&admission, addr),
        Err(AdmissionRefusal::SourceChurn),
        "the cycle after the budget is refused, and for the rate bound"
    );
    // Not a one-off: the refusal persists rather than the budget re-minting behind it.
    for _ in 0..64 {
        assert_eq!(
            churn_once(&admission, addr),
            Err(AdmissionRefusal::SourceChurn)
        );
    }
}

/// T2 — releasing concurrency does not erase churn history.
///
/// This is the whole distinction from #2547, and the mutation most likely to be introduced by
/// someone "tidying up" the release path: the table returns to empty after every cycle, so
/// clearing the source's record there looks like the obvious cleanup. It is the defect.
#[test]
fn returning_to_zero_concurrency_does_not_refresh_the_budget() {
    let admission = node_policy();
    let addr = source("198.51.100.9");

    for _ in 0..PREAUTH_CHURN_BUDGET_PER_SOURCE {
        churn_once(&admission, addr).expect("within budget");
    }

    assert_eq!(
        admission.live_total(),
        0,
        "precondition: no concurrency is held at all"
    );
    assert_eq!(
        admission.tracked_sources(),
        0,
        "precondition: the concurrency table has forgotten this source, as it should"
    );

    assert_eq!(
        churn_once(&admission, addr),
        Err(AdmissionRefusal::SourceChurn),
        "an idle source is still refused: the rate record outlived every connection"
    );
    assert_eq!(
        admission.tracked_churn_sources(),
        1,
        "and it is the churn table, not the concurrency table, that remembers"
    );
}

/// T3 — one source's exhausted budget is not another's.
///
/// A rate bound that aggregated across sources would be a far more effective denial of service
/// than the churn it prevents: one address could lock out every other.
#[test]
fn exhausting_one_source_leaves_another_untouched() {
    let admission = node_policy();
    let noisy = source("203.0.113.7");
    let quiet = source("203.0.113.8");

    for _ in 0..PREAUTH_CHURN_BUDGET_PER_SOURCE {
        churn_once(&admission, noisy).expect("within budget");
    }
    assert_eq!(
        churn_once(&admission, noisy),
        Err(AdmissionRefusal::SourceChurn)
    );

    assert_eq!(
        admission.churn_budget_remaining(quiet),
        PREAUTH_CHURN_BUDGET_PER_SOURCE,
        "an unrelated source has spent nothing"
    );
    for _ in 0..PREAUTH_CHURN_BUDGET_PER_SOURCE {
        churn_once(&admission, quiet).expect("the quiet source has its own budget");
    }
}

/// T3b — a different source *port* is the same source.
///
/// Keying on the full socket address would make every reconnect a new source and the bound
/// would aggregate nothing, which is the failure mode the issue names explicitly.
#[test]
fn a_new_source_port_does_not_mint_a_new_budget() {
    let admission = node_policy();

    for port in 0..PREAUTH_CHURN_BUDGET_PER_SOURCE {
        let addr: SocketAddr = format!("203.0.113.7:{}", 40000 + port)
            .parse()
            .expect("test address");
        churn_once(&admission, addr).expect("within budget");
    }

    let fresh_port: SocketAddr = "203.0.113.7:59999".parse().expect("test address");
    assert_eq!(
        churn_once(&admission, fresh_port),
        Err(AdmissionRefusal::SourceChurn),
        "a new ephemeral port is the same source and inherits the spent budget"
    );
}

/// T3c — IPv6 is charged by /64, so rotating within a prefix does not mint a new budget.
///
/// SLAAC and privacy extensions rotate the host bits by design. Keying on the exact address
/// would make the bound free to evade for any v6 client, which is why `SourceKey` already
/// treats v6 structurally.
#[test]
fn rotating_within_an_ipv6_prefix_does_not_mint_a_new_budget() {
    let admission = node_policy();

    for host in 0..PREAUTH_CHURN_BUDGET_PER_SOURCE {
        let addr: SocketAddr = format!("[2001:db8:1:2::{}]:9000", host + 1)
            .parse()
            .expect("test address");
        churn_once(&admission, addr).expect("within budget");
    }

    let rotated: SocketAddr = "[2001:db8:1:2:dead:beef:cafe:1]:9000"
        .parse()
        .expect("test address");
    assert_eq!(
        churn_once(&admission, rotated),
        Err(AdmissionRefusal::SourceChurn),
        "a rotated host portion stays inside the charged /64"
    );

    let neighbouring_prefix: SocketAddr = "[2001:db8:1:3::1]:9000".parse().expect("test address");
    assert!(
        churn_once(&admission, neighbouring_prefix).is_ok(),
        "a genuinely different /64 keeps its own budget"
    );
}

/// T4 — the budget refills, and does so continuously rather than at a window edge.
///
/// Driven through `try_admit_at` so the assertion sits at an exact point on the curve. Nothing
/// here sleeps, so the test means the same thing on a loaded machine as on an idle one.
#[test]
fn the_budget_refills_over_the_window() {
    let admission = node_policy();
    let addr = source("192.0.2.30");

    for _ in 0..PREAUTH_CHURN_BUDGET_PER_SOURCE {
        churn_once(&admission, addr).expect("within budget");
    }
    let spent_at = Instant::now();
    assert_eq!(
        churn_once(&admission, addr),
        Err(AdmissionRefusal::SourceChurn),
        "precondition: exhausted"
    );

    // Still exhausted a hair later: the refusal is a rate, not a single-shot latch that any
    // elapsed time clears.
    assert_eq!(
        admission
            .try_admit_at(addr, spent_at + PREAUTH_CHURN_REFILL_WINDOW / 100)
            .map(drop),
        Err(AdmissionRefusal::SourceChurn),
        "one percent of the window is not one token"
    );

    // Half the window is half the budget: continuous refill, no edge to align with.
    let half = spent_at + PREAUTH_CHURN_REFILL_WINDOW / 2;
    let mut granted = 0usize;
    while admission.try_admit_at(addr, half).map(drop).is_ok() {
        granted += 1;
        assert!(
            granted <= PREAUTH_CHURN_BUDGET_PER_SOURCE,
            "refill overshot"
        );
    }
    assert_eq!(
        granted,
        PREAUTH_CHURN_BUDGET_PER_SOURCE / 2,
        "half a window refills half a budget"
    );

    // A full window *from the last charge* — which the loop above dated `half` — restores the
    // whole budget. Measuring from `spent_at` instead would ask for a window that has not
    // elapsed and is the easy way to write this assertion wrongly.
    let full = half + PREAUTH_CHURN_REFILL_WINDOW;
    let mut granted = 0usize;
    while admission.try_admit_at(addr, full).map(drop).is_ok() {
        granted += 1;
        assert!(
            granted <= PREAUTH_CHURN_BUDGET_PER_SOURCE,
            "refill overshot"
        );
    }
    assert_eq!(
        granted, PREAUTH_CHURN_BUDGET_PER_SOURCE,
        "a full window restores the full budget, and no more"
    );
}

/// T5 — an admission that authenticates is not churn.
///
/// The positive control, and the reason this bound is not NAT-hostile. Many peers behind one
/// address reconnecting is indistinguishable from churn by count alone; it is distinguishable
/// by whether the admissions ended in a peer. Ten times the budget, all released as
/// authenticated, costs nothing.
#[test]
fn churn_that_authenticates_is_not_charged() {
    let admission = node_policy();
    let nat = source("203.0.113.44");

    for cycle in 0..PREAUTH_CHURN_BUDGET_PER_SOURCE * 10 {
        let guard = admission
            .try_admit(nat)
            .unwrap_or_else(|refusal| panic!("authenticated cycle {cycle} refused: {refusal:?}"));
        guard.release_authenticated();
    }

    assert_eq!(
        admission.churn_budget_remaining(nat),
        PREAUTH_CHURN_BUDGET_PER_SOURCE,
        "authenticating peers never spend the churn budget"
    );
    assert_eq!(
        admission.tracked_churn_sources(),
        0,
        "and leave no entry behind at all"
    );
}

/// T5b — authenticating does not refund what abandoning already spent.
///
/// The two halves of T5 have to be pinned separately: "authentication is free" must not become
/// "authentication clears the record", which would let an attacker wipe its history with one
/// cheap Hello per burst.
#[test]
fn authenticating_does_not_refund_earlier_abandoned_admissions() {
    let admission = node_policy();
    let addr = source("203.0.113.55");

    let spend = PREAUTH_CHURN_BUDGET_PER_SOURCE / 2;
    for _ in 0..spend {
        churn_once(&admission, addr).expect("within budget");
    }
    let before = admission.churn_budget_remaining(addr);

    admission
        .try_admit(addr)
        .expect("still within budget")
        .release_authenticated();

    assert_eq!(
        admission.churn_budget_remaining(addr),
        before,
        "a successful authentication neither charges nor refunds"
    );
}

/// T6 — the churn table's own state stays bounded.
///
/// A defence that answers unbounded connection work with an unbounded map has not defended
/// anything. Two properties are asserted: a *refused* source does not create an entry (or being
/// refused would itself grow the map), and a source whose budget has refilled stops being
/// tracked (or the map would only ever grow).
#[test]
fn the_churn_table_does_not_grow_without_bound() {
    let admission = node_policy();
    let addr = source("192.0.2.77");

    for _ in 0..PREAUTH_CHURN_BUDGET_PER_SOURCE {
        churn_once(&admission, addr).expect("within budget");
    }
    assert_eq!(admission.tracked_churn_sources(), 1);

    for _ in 0..1_000 {
        assert_eq!(
            churn_once(&admission, addr),
            Err(AdmissionRefusal::SourceChurn)
        );
    }
    assert_eq!(
        admission.tracked_churn_sources(),
        1,
        "a thousand refusals created no entries: being refused cannot grow the map"
    );

    // A refilled entry says exactly what an absent one says, so it is dropped on touch.
    let recovered = Instant::now() + PREAUTH_CHURN_REFILL_WINDOW;
    admission
        .try_admit_at(addr, recovered)
        .expect("refilled")
        .release_authenticated();
    assert_eq!(
        admission.tracked_churn_sources(),
        0,
        "a source that has served out its budget stops being tracked"
    );
}

/// T6b — many distinct sources are tracked only while they owe something.
///
/// The cardinality argument in one assertion: entries exist for sources with an outstanding
/// charge, and evaporate as those charges refill.
#[test]
fn distinct_sources_are_tracked_only_while_they_owe() {
    let admission = node_policy();

    for octet in 0..200u8 {
        let addr: SocketAddr = format!("198.51.100.{octet}:9000")
            .parse()
            .expect("test address");
        churn_once(&admission, addr).expect("first cycle is always within budget");
    }
    assert_eq!(
        admission.tracked_churn_sources(),
        200,
        "each source that abandoned an admission is remembered"
    );

    let recovered = Instant::now() + PREAUTH_CHURN_REFILL_WINDOW;
    for octet in 0..200u8 {
        let addr: SocketAddr = format!("198.51.100.{octet}:9000")
            .parse()
            .expect("test address");
        admission
            .try_admit_at(addr, recovered)
            .expect("refilled")
            .release_authenticated();
    }
    assert_eq!(
        admission.tracked_churn_sources(),
        0,
        "and forgotten once their budgets refill"
    );
}

/// Charges land at release, so an unreleased burst is bounded by concurrency, not by the budget.
///
/// Stated rather than left to be rediscovered. A source may hold admissions whose charges have
/// not landed, so it can momentarily be ahead of its budget — but only by holding them, which is
/// what #2547 already bounds. The sustained rate is unchanged, which is the half that matters
/// and the half asserted second.
#[test]
fn an_unreleased_burst_is_bounded_by_concurrency_and_still_charged() {
    let admission = node_policy();
    let addr = source("192.0.2.101");

    // Spend the whole budget first, so nothing below is paid for out of it.
    for _ in 0..PREAUTH_CHURN_BUDGET_PER_SOURCE {
        churn_once(&admission, addr).expect("within budget");
    }
    assert_eq!(
        churn_once(&admission, addr),
        Err(AdmissionRefusal::SourceChurn),
        "precondition: budget exhausted"
    );

    // Refill exactly one token and take a burst against it. The check passes while the tokens
    // are still there, because nothing is debited until each admission ends.
    let refilled = Instant::now() + PREAUTH_CHURN_REFILL_WINDOW / 8;
    let mut burst = Vec::new();
    while let Ok(guard) = admission.try_admit_at(addr, refilled) {
        burst.push(guard);
        assert!(
            burst.len() <= MAX_PREAUTH_CONNECTIONS_PER_SOURCE,
            "an unreleased burst must still be bounded by the concurrency limit"
        );
    }
    assert!(
        burst.len() > 1,
        "one refilled token admitted a burst larger than itself: {} admissions",
        burst.len()
    );

    // ...and every one of them is charged as it ends, so the sustained rate is untouched.
    let held = burst.len();
    drop(burst);
    assert_eq!(
        admission.churn_budget_remaining(addr),
        0,
        "all {held} admissions were charged on release"
    );
    assert_eq!(
        churn_once(&admission, addr),
        Err(AdmissionRefusal::SourceChurn),
        "the source is refused again immediately: the burst bought no sustained rate"
    );
}

/// The rate bound does not shadow the concurrency bound it completes.
///
/// A source holding its full concurrent allowance is doing nothing the older bounds forbid, so
/// it must not be refused by the newer one — the budget is derived from exactly that ceiling.
#[test]
fn a_source_at_its_concurrency_limit_is_not_refused_for_churn() {
    let admission = node_policy();
    let addr = source("203.0.113.99");

    let held: Vec<_> = (0..MAX_PREAUTH_CONNECTIONS_PER_SOURCE)
        .map(|slot| {
            admission
                .try_admit(addr)
                .unwrap_or_else(|refusal| panic!("slot {slot} refused: {refusal:?}"))
        })
        .collect();

    assert_eq!(
        admission.try_admit(addr).map(drop),
        Err(AdmissionRefusal::SourceLimit),
        "the next one is refused by the concurrency bound, not the rate bound"
    );
    drop(held);
}
