//! `/v1/sdis/anchor` identity-mutation authority enforcement (issue #2448).
//!
//! Anchor reads are intentionally public today. Public anchor values must not be
//! reusable as authority to mutate that anchor's current DID or device list.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use actix_web::{test, web, App};
use icn_gateway::api::sdis::anchor::{self, AnchorRecord, AnchorStore};
use icn_gateway::api::sdis::enrollment::{AnchorDto, EnrollmentPathwayDto};

const ANCHOR_ID: &str = "anchor_test_authority";

fn current_did() -> String {
    icn_identity::IdentityBundle::generate()
        .expect("generate identity")
        .did()
        .to_string()
}

fn seeded_store(did: &str) -> Arc<AnchorStore> {
    let store = Arc::new(AnchorStore::new());
    let anchor = AnchorDto {
        anchor_id: ANCHOR_ID.to_string(),
        created_at: 1_702_425_600,
        pathway: EnrollmentPathwayDto::Genesis {
            reason: "test fixture".to_string(),
        },
    };
    store
        .store_anchor(
            ANCHOR_ID.to_string(),
            AnchorRecord::new(anchor, did.to_string()),
        )
        .expect("seed anchor");
    store
}

#[actix_web::test]
async fn public_anchor_data_cannot_be_replayed_to_rotate_keys() {
    let did = current_did();
    let store = seeded_store(&did);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(store.clone()))
            .service(web::scope("/v1/sdis").configure(anchor::configure)),
    )
    .await;

    let public_read = test::TestRequest::get()
        .uri(&format!("/v1/sdis/anchor/{ANCHOR_ID}"))
        .to_request();
    let body: serde_json::Value = test::call_and_read_body_json(&app, public_read).await;
    assert_eq!(body["current_did"], did);

    let rotate = test::TestRequest::post()
        .uri("/v1/sdis/anchor/rotate-keys")
        .set_json(serde_json::json!({
            "anchor_id": ANCHOR_ID,
            "current_did": body["current_did"].as_str().unwrap(),
            "new_keybundle": {
                "ed25519_pub": "attacker-ed25519",
                "ml_dsa_pub": "attacker-ml-dsa",
                "x25519_pub": "attacker-x25519"
            },
            "reason": "replay public anchor data"
        }))
        .to_request();
    let response = test::call_service(&app, rotate).await;

    assert_eq!(
        response.status().as_u16(),
        403,
        "publicly readable anchor fields must not authorize key rotation"
    );
    let record = store
        .get_anchor(ANCHOR_ID)
        .expect("read anchor")
        .expect("anchor exists");
    assert_eq!(record.current_did, did);
    assert_eq!(record.rotation_count(), 0);
}

#[actix_web::test]
async fn anonymous_device_addition_is_refused_and_state_is_unchanged() {
    let did = current_did();
    let store = seeded_store(&did);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(store.clone()))
            .service(web::scope("/v1/sdis").configure(anchor::configure)),
    )
    .await;

    let add_device = test::TestRequest::post()
        .uri("/v1/sdis/anchor/devices/add")
        .set_json(serde_json::json!({
            "anchor_id": ANCHOR_ID,
            "device_name": "attacker device",
            "device_pubkey": "attacker-device-pubkey"
        }))
        .to_request();
    let response = test::call_service(&app, add_device).await;

    assert_eq!(
        response.status().as_u16(),
        403,
        "anonymous callers must not attach devices to an anchor"
    );

    let devices_read = test::TestRequest::get()
        .uri(&format!("/v1/sdis/anchor/{ANCHOR_ID}/devices"))
        .to_request();
    let body: serde_json::Value = test::call_and_read_body_json(&app, devices_read).await;
    assert_eq!(body["device_count"], 0);

    let record = store
        .get_anchor(ANCHOR_ID)
        .expect("read anchor")
        .expect("anchor exists");
    assert!(record.devices.is_empty());
}

#[actix_web::test]
async fn disabled_anchor_writes_fail_closed_without_store_dependency() {
    let app =
        test::init_service(App::new().service(web::scope("/v1/sdis").configure(anchor::configure)))
            .await;

    let rotate = test::TestRequest::post()
        .uri("/v1/sdis/anchor/rotate-keys")
        .set_json(serde_json::json!({
            "anchor_id": ANCHOR_ID,
            "current_did": current_did(),
            "new_keybundle": {
                "ed25519_pub": "new-ed25519",
                "ml_dsa_pub": "new-ml-dsa",
                "x25519_pub": "new-x25519"
            },
            "reason": null
        }))
        .to_request();
    assert_eq!(
        test::call_service(&app, rotate).await.status().as_u16(),
        403
    );

    let add_device = test::TestRequest::post()
        .uri("/v1/sdis/anchor/devices/add")
        .set_json(serde_json::json!({
            "anchor_id": ANCHOR_ID,
            "device_name": "phone",
            "device_pubkey": "device-pubkey"
        }))
        .to_request();
    assert_eq!(
        test::call_service(&app, add_device).await.status().as_u16(),
        403
    );
}
