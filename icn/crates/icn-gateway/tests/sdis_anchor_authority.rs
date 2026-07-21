//! `/v1/sdis/anchor` identity-mutation authority enforcement (issue #2448).
//!
//! Anchor reads are intentionally public today. Public anchor values must not be
//! reusable as authority to mutate that anchor's current DID or device list.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use actix_web::{
    http::{header, StatusCode},
    test, web, App,
};
use icn_gateway::api::sdis::anchor::{self, AnchorRecord, AnchorStore};
use icn_gateway::api::sdis::enrollment::{AnchorDto, EnrollmentPathwayDto};

const ANCHOR_ID: &str = "anchor_test_authority";
const ROTATE_ROUTE: &str = "/v1/sdis/anchor/rotate-keys";
const ADD_DEVICE_ROUTE: &str = "/v1/sdis/anchor/devices/add";

#[derive(Clone, Copy)]
enum BodyCase {
    ValidJson,
    MalformedJson,
    EmptyJsonBody,
    MissingContentType,
    WrongContentType,
    ArbitraryBytes,
    PublicReplay,
}

impl BodyCase {
    const ALL: [Self; 7] = [
        Self::ValidJson,
        Self::MalformedJson,
        Self::EmptyJsonBody,
        Self::MissingContentType,
        Self::WrongContentType,
        Self::ArbitraryBytes,
        Self::PublicReplay,
    ];
}

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

fn assert_anchor_state_unchanged(store: &AnchorStore, expected_did: &str) {
    let record = store
        .get_anchor(ANCHOR_ID)
        .expect("read anchor")
        .expect("anchor exists");
    assert_eq!(record.current_did, expected_did);
    assert_eq!(record.rotation_count(), 0);
    assert!(record.rotation_history.is_empty());
    assert!(record.devices.is_empty());
}

fn valid_rotate_json(did: &str) -> serde_json::Value {
    serde_json::json!({
        "anchor_id": ANCHOR_ID,
        "current_did": did,
        "new_keybundle": {
            "ed25519_pub": "attacker-ed25519",
            "ml_dsa_pub": "attacker-ml-dsa",
            "x25519_pub": "attacker-x25519"
        },
        "reason": "replay public anchor data"
    })
}

fn valid_device_json() -> serde_json::Value {
    serde_json::json!({
        "anchor_id": ANCHOR_ID,
        "device_name": "attacker device",
        "device_pubkey": "attacker-device-pubkey"
    })
}

fn rotation_request(case: BodyCase, did: &str) -> actix_http::Request {
    let valid_body = valid_rotate_json(did).to_string();
    match case {
        BodyCase::ValidJson | BodyCase::PublicReplay => test::TestRequest::post()
            .uri(ROTATE_ROUTE)
            .set_json(valid_rotate_json(did))
            .to_request(),
        BodyCase::MalformedJson => test::TestRequest::post()
            .uri(ROTATE_ROUTE)
            .insert_header((header::CONTENT_TYPE, "application/json"))
            .set_payload(r#"{"anchor_id":"anchor_test_authority","#)
            .to_request(),
        BodyCase::EmptyJsonBody => test::TestRequest::post()
            .uri(ROTATE_ROUTE)
            .insert_header((header::CONTENT_TYPE, "application/json"))
            .to_request(),
        BodyCase::MissingContentType => test::TestRequest::post()
            .uri(ROTATE_ROUTE)
            .set_payload(valid_body)
            .to_request(),
        BodyCase::WrongContentType => test::TestRequest::post()
            .uri(ROTATE_ROUTE)
            .insert_header((header::CONTENT_TYPE, "text/plain"))
            .set_payload(valid_body)
            .to_request(),
        BodyCase::ArbitraryBytes => test::TestRequest::post()
            .uri(ROTATE_ROUTE)
            .insert_header((header::CONTENT_TYPE, "application/octet-stream"))
            .set_payload("not-json\0\x01\x02")
            .to_request(),
    }
}

fn add_device_request(case: BodyCase, did: &str) -> actix_http::Request {
    let valid_body = valid_device_json().to_string();
    match case {
        BodyCase::ValidJson => test::TestRequest::post()
            .uri(ADD_DEVICE_ROUTE)
            .set_json(valid_device_json())
            .to_request(),
        BodyCase::PublicReplay => test::TestRequest::post()
            .uri(ADD_DEVICE_ROUTE)
            .set_json(serde_json::json!({
                "anchor_id": ANCHOR_ID,
                "current_did": did,
                "device_name": "attacker device",
                "device_pubkey": "attacker-device-pubkey"
            }))
            .to_request(),
        BodyCase::MalformedJson => test::TestRequest::post()
            .uri(ADD_DEVICE_ROUTE)
            .insert_header((header::CONTENT_TYPE, "application/json"))
            .set_payload(r#"{"anchor_id":"anchor_test_authority","#)
            .to_request(),
        BodyCase::EmptyJsonBody => test::TestRequest::post()
            .uri(ADD_DEVICE_ROUTE)
            .insert_header((header::CONTENT_TYPE, "application/json"))
            .to_request(),
        BodyCase::MissingContentType => test::TestRequest::post()
            .uri(ADD_DEVICE_ROUTE)
            .set_payload(valid_body)
            .to_request(),
        BodyCase::WrongContentType => test::TestRequest::post()
            .uri(ADD_DEVICE_ROUTE)
            .insert_header((header::CONTENT_TYPE, "text/plain"))
            .set_payload(valid_body)
            .to_request(),
        BodyCase::ArbitraryBytes => test::TestRequest::post()
            .uri(ADD_DEVICE_ROUTE)
            .insert_header((header::CONTENT_TYPE, "application/octet-stream"))
            .set_payload("not-json\0\x01\x02")
            .to_request(),
    }
}

#[actix_web::test]
async fn disabled_anchor_writes_return_forbidden_without_body_parsing() {
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
    let response = test::call_service(&app, public_read).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = test::read_body_json(response).await;
    assert_eq!(body["current_did"], did);
    assert_eq!(body["rotation_count"], 0);

    let history_read = test::TestRequest::get()
        .uri(&format!("/v1/sdis/anchor/{ANCHOR_ID}/history"))
        .to_request();
    let response = test::call_service(&app, history_read).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = test::read_body_json(response).await;
    assert_eq!(body["total_count"], 0);

    let devices_read = test::TestRequest::get()
        .uri(&format!("/v1/sdis/anchor/{ANCHOR_ID}/devices"))
        .to_request();
    let response = test::call_service(&app, devices_read).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = test::read_body_json(response).await;
    assert_eq!(body["device_count"], 0);

    for case in BodyCase::ALL {
        let response = test::call_service(&app, rotation_request(case, &did)).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "disabled key rotation must not parse or accept any request body"
        );
        assert_anchor_state_unchanged(&store, &did);

        let response = test::call_service(&app, add_device_request(case, &did)).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "disabled device addition must not parse or accept any request body"
        );
        assert_anchor_state_unchanged(&store, &did);
    }
}

#[actix_web::test]
async fn disabled_anchor_writes_fail_closed_without_store_dependency() {
    let app =
        test::init_service(App::new().service(web::scope("/v1/sdis").configure(anchor::configure)))
            .await;

    let did = current_did();
    for case in BodyCase::ALL {
        assert_eq!(
            test::call_service(&app, rotation_request(case, &did))
                .await
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            test::call_service(&app, add_device_request(case, &did))
                .await
                .status(),
            StatusCode::FORBIDDEN
        );
    }
}
