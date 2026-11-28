//! ADR-0091 S0c (D7): does the REAL `release_escrow` handler settle twice when the
//! persisted escrow row disagrees with the ledger?
//!
//! # What is real and what is not
//!
//! Real: the `release_escrow` handler (routed through Actix's test application),
//! `require_scope`, `require_coop_access`, `check_conditions`, `LedgerManager`
//! (`new_with_storage`, a file-backed sled ledger per coop), `EscrowStore` over a
//! file-backed sled database, real generated DIDs.
//!
//! Substituted / bypassed: JWT validation (the test inserts `TokenClaims` into the
//! request extensions, the same pattern the handler's own unit tests use), the
//! budget store and the event broadcaster (neither is configured, so their
//! non-idempotent side effects are NOT covered), the HTTP listener and middleware.
//!
//! # What this is not
//!
//! This is a *stale-state replay simulation*. It does NOT inject a failure into
//! `store.update` inside the handler and does NOT kill a process mid-handler: the
//! first release runs to completion (HTTP 200), and the test then writes the
//! pre-release escrow row back to reproduce the persisted state that a crash
//! between the ledger append and the escrow write (or a failed `store.update`)
//! would leave behind. No production fault-injection seam exists or is added.
//!
//! Every assertion is tied to an observed journal row, an observed balance or an
//! observed persisted escrow record; an HTTP status alone is never taken as proof
//! of economic execution. The `characterization_*` tests record CURRENT behaviour;
//! if a fix changes it they should flip to the `*_invariant` form, not be weakened.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use actix_web::http::StatusCode;
use actix_web::{test as actix_test, web, App, HttpMessage};
use icn_gateway::api::escrow::{release_escrow, Escrow, EscrowStatus, EscrowStore};
use icn_gateway::auth::TokenClaims;
use icn_gateway::ledger_mgr::LedgerManager;
use icn_identity::{Did, IdentityBundle};
use serde_json::Value;

const COOP: &str = "s0c-coop";
const CCY: &str = "hours";
const AMOUNT: i64 = 100;
const ESCROW_ID: &str = "s0c-escrow-1";

/// Distinct millisecond timestamps are needed for the ledger to see a different
/// entry (entry hash covers the ms timestamp). Real retries are always later.
const RETRY_GAP: Duration = Duration::from_millis(5);

struct Actors {
    payer: Did,
    beneficiary: Did,
    approver: Did,
}

fn actors() -> Actors {
    Actors {
        payer: IdentityBundle::generate().unwrap().did().clone(),
        beneficiary: IdentityBundle::generate().unwrap().did().clone(),
        approver: IdentityBundle::generate().unwrap().did().clone(),
    }
}

/// Everything that holds a sled handle, so dropping it releases the file locks.
struct World {
    db: sled::Db,
    escrows: EscrowStore,
    ledgers: Arc<LedgerManager>,
}

fn open_world(dir: &Path) -> World {
    let db = sled::open(dir.join("escrow-db")).unwrap();
    let escrows = EscrowStore::new(db.clone());
    let ledgers = Arc::new(LedgerManager::new_with_storage(dir.join("gateway-data")));
    World {
        db,
        escrows,
        ledgers,
    }
}

fn seed_pending_escrow(w: &World, a: &Actors) -> Escrow {
    let escrow = Escrow {
        id: ESCROW_ID.to_string(),
        coop_id: COOP.to_string(),
        creator: a.payer.to_string(),
        from_account: a.payer.to_string(),
        to_account: a.beneficiary.to_string(),
        amount: AMOUNT,
        currency: CCY.to_string(),
        status: EscrowStatus::Pending,
        conditions: vec![],
        approvals: vec![],
        expires_at: None,
        description: "S0c stale-state release".to_string(),
        created_at: 1,
        updated_at: 1,
    };
    w.escrows.insert(escrow.clone()).unwrap();
    escrow
}

fn claims(sub: &Did) -> TokenClaims {
    TokenClaims {
        entity_id: None,
        entity_type: None,
        sub: sub.to_string(),
        iat: 1_000_000_000,
        exp: 9_999_999_999,
        coop_id: COOP.to_string(),
        scopes: vec!["settlements:write".to_string()],
        jti: None,
    }
}

struct Released {
    status: StatusCode,
    body: Value,
}

impl Released {
    fn released(&self) -> bool {
        self.body["released"].as_bool().unwrap_or(false)
    }
    fn tx_hash(&self) -> Option<String> {
        self.body["transaction_hash"].as_str().map(str::to_owned)
    }
}

/// POST /escrow/{id}/release through the real handler.
async fn release(w: &World, caller: &Did) -> Released {
    let app = actix_test::init_service(
        App::new()
            .app_data(web::Data::new(w.escrows.clone()))
            .app_data(web::Data::new(w.ledgers.clone()))
            .service(release_escrow),
    )
    .await;
    let req = actix_test::TestRequest::post()
        .uri(&format!("/escrow/{ESCROW_ID}/release"))
        .set_json(serde_json::json!({ "proof": null }))
        .to_request();
    req.extensions_mut().insert(claims(caller));
    let resp = actix_test::call_service(&app, req).await;
    let status = resp.status();
    let bytes = actix_test::read_body(resp).await;
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    Released { status, body }
}

/// Independent observers: journal rows and balances come from the ledger, the
/// escrow record from the escrow store, never from the handler's response.
async fn journal_rows(w: &World) -> usize {
    w.ledgers
        .get_history(&COOP.to_string(), None, 0, 1000)
        .await
        .unwrap()
        .len()
}

async fn balance(w: &World, who: &Did) -> i64 {
    w.ledgers
        .get_position(&COOP.to_string(), who, CCY)
        .await
        .unwrap()
}

fn stored(w: &World) -> Escrow {
    w.escrows.get(ESCROW_ID).unwrap().expect("escrow row")
}

/// A. Control: the status guard stops a plain second release, and the observers
/// can see exactly one settlement. This is the positive control for every
/// "no second row" claim below.
#[actix_web::test]
async fn control_normal_release_settles_once_and_second_release_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let w = open_world(dir.path());
    let a = actors();
    seed_pending_escrow(&w, &a);

    let first = release(&w, &a.approver).await;
    assert_eq!(first.status, StatusCode::OK, "body: {}", first.body);
    assert!(first.released());
    assert!(first.tx_hash().is_some());

    assert_eq!(
        journal_rows(&w).await,
        1,
        "exactly one settlement journaled"
    );
    assert_eq!(balance(&w, &a.beneficiary).await, AMOUNT);
    assert_eq!(stored(&w).status, EscrowStatus::Released);

    tokio::time::sleep(RETRY_GAP).await;
    let second = release(&w, &a.approver).await;
    assert_eq!(
        second.status,
        StatusCode::BAD_REQUEST,
        "body: {}",
        second.body
    );
    assert_eq!(journal_rows(&w).await, 1, "refused release adds no row");
    assert_eq!(balance(&w, &a.beneficiary).await, AMOUNT);
    assert_eq!(stored(&w).status, EscrowStatus::Released);
}

/// What the stale-state scenario observed, printed for the evidence log.
struct StaleReplay {
    h1: String,
    second: Released,
    rows_after: usize,
    beneficiary_after: i64,
    escrow_after: Escrow,
}

/// Runs: first release -> verify the settlement is recorded -> restore the pre-release
/// escrow row -> verify the row is Pending again while the settlement remains -> retry.
/// `between` runs after the restore and may reopen the stores.
async fn stale_state_replay<F, Fut>(w: World, a: &Actors, between: F) -> StaleReplay
where
    F: FnOnce(World) -> Fut,
    Fut: std::future::Future<Output = World>,
{
    let original = seed_pending_escrow(&w, a);

    let first = release(&w, &a.approver).await;
    assert_eq!(first.status, StatusCode::OK, "body: {}", first.body);
    assert!(first.released());
    let h1 = first
        .tx_hash()
        .expect("first release returns a ledger hash");
    assert_eq!(journal_rows(&w).await, 1, "first settlement journaled");
    assert_eq!(balance(&w, &a.beneficiary).await, AMOUNT);
    assert_eq!(stored(&w).status, EscrowStatus::Released);

    // Stale-state injection: put the pre-release record back.
    w.escrows.update(ESCROW_ID, original.clone()).unwrap();
    w.db.flush().unwrap();
    let restored = stored(&w);
    assert_eq!(
        restored.status,
        EscrowStatus::Pending,
        "injection took effect"
    );
    assert!(restored.approvals.is_empty(), "approvals restored to empty");
    assert_eq!(
        journal_rows(&w).await,
        1,
        "original settlement still recorded"
    );

    let w = between(w).await;

    // Preconditions must hold on the (possibly reopened) stores.
    assert_eq!(
        stored(&w).status,
        EscrowStatus::Pending,
        "stale row precondition"
    );
    assert_eq!(journal_rows(&w).await, 1, "settlement precondition");

    tokio::time::sleep(RETRY_GAP).await;
    let second = release(&w, &a.approver).await;

    let replay = StaleReplay {
        h1,
        rows_after: journal_rows(&w).await,
        beneficiary_after: balance(&w, &a.beneficiary).await,
        escrow_after: stored(&w),
        second,
    };
    eprintln!(
        "S0C-OBSERVED second_status={} second_released={} h1={} h2={:?} rows_after={} \
         beneficiary_after={} escrow_after={:?} approvals_after={}",
        replay.second.status,
        replay.second.released(),
        replay.h1,
        replay.second.tx_hash(),
        replay.rows_after,
        replay.beneficiary_after,
        replay.escrow_after.status,
        replay.escrow_after.approvals.len(),
    );
    replay
}

/// B. Stale escrow-state retry in one process lifetime.
///
/// CURRENT BEHAVIOUR (recorded, not desired): the retry is accepted and a second,
/// distinct settlement is journaled.
#[actix_web::test]
async fn characterization_stale_escrow_state_retry_settles_again() {
    let dir = tempfile::tempdir().unwrap();
    let a = actors();
    let r = stale_state_replay(open_world(dir.path()), &a, |w| async move { w }).await;

    assert_eq!(r.second.status, StatusCode::OK, "body: {}", r.second.body);
    assert!(r.second.released());
    let h2 = r.second.tx_hash().expect("retry returns a ledger hash");
    assert_ne!(h2, r.h1, "second settlement has a distinct journal hash");
    assert_eq!(r.rows_after, 2, "two settlements journaled for one escrow");
    assert_eq!(
        r.beneficiary_after,
        2 * AMOUNT,
        "beneficiary credited twice for one escrow"
    );
    assert_eq!(r.escrow_after.status, EscrowStatus::Released);
    assert_eq!(r.escrow_after.approvals.len(), 1);
}

/// C. The same replay across a clean close and reopen of both file-backed stores.
/// Persistence here is the clean-drop kind (escrow side explicitly flushed); this
/// makes no claim about surviving SIGKILL or power loss.
#[actix_web::test]
async fn characterization_stale_escrow_state_retry_settles_again_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    let a = actors();
    let reopen_path = path.clone();
    let r = stale_state_replay(open_world(&path), &a, move |w| async move {
        drop(w); // releases the sled file locks for both stores
        open_world(&reopen_path)
    })
    .await;

    assert_eq!(r.second.status, StatusCode::OK, "body: {}", r.second.body);
    assert!(r.second.released());
    let h2 = r.second.tx_hash().expect("retry returns a ledger hash");
    assert_ne!(h2, r.h1, "second settlement has a distinct journal hash");
    assert_eq!(r.rows_after, 2, "duplicate settlement survives reopen");
    assert_eq!(r.beneficiary_after, 2 * AMOUNT);
    assert_eq!(r.escrow_after.status, EscrowStatus::Released);
}

/// FUTURE INVARIANT (RED today): one escrow release must never cause more than one
/// economic settlement, even when persisted caller state is stale and the request is
/// retried. Expected to be satisfied by a durable operation id (ADR-0091 PR-5).
#[actix_web::test]
#[ignore = "ADR-0091 PR-5: escrow release needs a durable operation id; RED on main today"]
async fn stale_escrow_state_retry_must_not_settle_twice_invariant() {
    let dir = tempfile::tempdir().unwrap();
    let a = actors();
    let r = stale_state_replay(open_world(dir.path()), &a, |w| async move { w }).await;

    assert_eq!(
        r.rows_after, 1,
        "a stale-state retry added a second settlement for one escrow"
    );
    assert_eq!(
        r.beneficiary_after, AMOUNT,
        "beneficiary was credited more than once for one escrow"
    );
}
