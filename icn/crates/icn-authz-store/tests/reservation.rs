#![allow(clippy::unwrap_used)]

use icn_authz::effect_v0::ContextRef;
use icn_authz_store::{
    pool_key, reservation_key, LocalReservationPolicyV0, LocalResourceReservations,
    ReservationError, ReservationRecordV0, ReservationRequestV0, ResourcePoolV0,
    RESERVATION_ACTION,
};
use icn_store::{SledStore, Store};

const CTX: ContextRef = ContextRef([0x11; 32]);
const OP: [u8; 32] = [0x22; 32];
const POOL: [u8; 32] = [0x33; 32];
const TARGET: [u8; 32] = [0x44; 32];

fn request() -> ReservationRequestV0 {
    ReservationRequestV0 {
        context: CTX,
        operation_id: OP,
        pool_id: POOL,
        expected_pool_revision: 0,
        quantity: 3,
        unit: "hours".into(),
        request_params_hash: [0x55; 32],
        policy_hash: [0x66; 32],
    }
}

fn pool() -> ResourcePoolV0 {
    ResourcePoolV0 {
        context: CTX,
        pool_id: POOL,
        custodian: TARGET,
        unit: "hours".into(),
        total_capacity: 10,
        available_capacity: 10,
        revision: 0,
        policy: LocalReservationPolicyV0 {
            enabled: true,
            max_quantity_per_reservation: 6,
            policy_hash: [0x66; 32],
        },
    }
}

fn record() -> ReservationRecordV0 {
    ReservationRecordV0 {
        context: CTX,
        operation_id: OP,
        pool_id: POOL,
        custodian: TARGET,
        quantity: 3,
        unit: "hours".into(),
        request_params_hash: [0x55; 32],
        policy_hash: [0x66; 32],
        authorization_hash: [0x77; 32],
        reserved_at: 900,
        pool_revision_before: 0,
        pool_revision_after: 1,
        available_before: 10,
        available_after: 7,
    }
}

#[test]
fn request_commitment_binds_every_reservation_parameter() {
    let base = request();
    let want = base.commitment().unwrap();
    assert_eq!(base.effect(TARGET).unwrap().action, RESERVATION_ACTION);

    let variants = [
        ReservationRequestV0 {
            context: ContextRef([0x12; 32]),
            ..base.clone()
        },
        ReservationRequestV0 {
            operation_id: [0x23; 32],
            ..base.clone()
        },
        ReservationRequestV0 {
            pool_id: [0x34; 32],
            ..base.clone()
        },
        ReservationRequestV0 {
            expected_pool_revision: 1,
            ..base.clone()
        },
        ReservationRequestV0 {
            quantity: 4,
            ..base.clone()
        },
        ReservationRequestV0 {
            unit: "days".into(),
            ..base.clone()
        },
        ReservationRequestV0 {
            request_params_hash: [0x56; 32],
            ..base.clone()
        },
        ReservationRequestV0 {
            policy_hash: [0x67; 32],
            ..base.clone()
        },
    ];
    for variant in variants {
        assert_ne!(variant.commitment().unwrap(), want);
    }
}

#[test]
fn invalid_requests_and_pool_policy_fail_closed() {
    for invalid in [
        ReservationRequestV0 {
            quantity: 0,
            ..request()
        },
        ReservationRequestV0 {
            unit: String::new(),
            ..request()
        },
        ReservationRequestV0 {
            unit: "x".repeat(65),
            ..request()
        },
    ] {
        assert!(invalid.validate().is_err());
    }

    for invalid in [
        ResourcePoolV0 {
            total_capacity: 0,
            available_capacity: 0,
            ..pool()
        },
        ResourcePoolV0 {
            available_capacity: 11,
            ..pool()
        },
        ResourcePoolV0 {
            policy: LocalReservationPolicyV0 {
                max_quantity_per_reservation: 0,
                ..pool().policy
            },
            ..pool()
        },
        ResourcePoolV0 {
            policy: LocalReservationPolicyV0 {
                max_quantity_per_reservation: 11,
                ..pool().policy
            },
            ..pool()
        },
    ] {
        assert!(invalid.validate().is_err());
    }
}

#[test]
fn records_roundtrip_strictly_and_reject_trailing_or_malformed_bytes() {
    let p = pool();
    assert_eq!(ResourcePoolV0::decode(&p.encode().unwrap()).unwrap(), p);
    let r = record();
    assert_eq!(
        ReservationRecordV0::decode(&r.encode().unwrap()).unwrap(),
        r
    );

    for mut bytes in [p.encode().unwrap(), r.encode().unwrap()] {
        bytes.push(0);
        assert!(ResourcePoolV0::decode(&bytes).is_err());
        assert!(ReservationRecordV0::decode(&bytes).is_err());
    }
    assert!(ResourcePoolV0::decode(&[]).is_err());
    assert!(ReservationRecordV0::decode(&[0xff; 8]).is_err());
}

#[test]
fn keys_isolate_context_custodian_pool_and_operation() {
    let p = pool_key(CTX, TARGET, POOL);
    assert_ne!(p, pool_key(ContextRef([0x12; 32]), TARGET, POOL));
    assert_ne!(p, pool_key(CTX, [0x45; 32], POOL));
    assert_ne!(p, pool_key(CTX, TARGET, [0x34; 32]));

    let r = reservation_key(CTX, OP);
    assert_ne!(r, reservation_key(ContextRef([0x12; 32]), OP));
    assert_ne!(r, reservation_key(CTX, [0x23; 32]));
}

#[test]
fn pool_initialization_is_durable_and_never_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    {
        let reservations = LocalResourceReservations::open(dir.path(), TARGET).unwrap();
        reservations.initialize_pool(pool()).unwrap();
        assert_eq!(reservations.inspect_pool(CTX, POOL).unwrap(), Some(pool()));
        assert_eq!(
            reservations.initialize_pool(pool()),
            Err(ReservationError::PoolAlreadyExists)
        );
    }
    let reopened = LocalResourceReservations::open(dir.path(), TARGET).unwrap();
    assert_eq!(reopened.inspect_pool(CTX, POOL).unwrap(), Some(pool()));
}

#[test]
fn malformed_persistent_pool_and_reservation_records_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    {
        let reservations = LocalResourceReservations::open(dir.path(), TARGET).unwrap();
        reservations.initialize_pool(pool()).unwrap();
    }
    {
        let store = SledStore::open(dir.path()).unwrap();
        Store::put(&store, &pool_key(CTX, TARGET, POOL), b"malformed-pool").unwrap();
        Store::put(&store, &reservation_key(CTX, OP), b"malformed-reservation").unwrap();
        Store::flush(&store).unwrap();
    }

    let reservations = LocalResourceReservations::open(dir.path(), TARGET).unwrap();
    assert!(matches!(
        reservations.inspect_pool(CTX, POOL),
        Err(ReservationError::Corrupt(_))
    ));
    assert!(matches!(
        reservations.inspect_reservation(CTX, OP),
        Err(ReservationError::Corrupt(_))
    ));
}
