//! Integration tests for federation layer

use icn_federation::*;
use icn_identity::{Did, KeyPair};

fn test_keypair() -> (KeyPair, Did) {
    let keypair = KeyPair::generate().unwrap();
    let did = Did::from_public_key(keypair.verifying_key());
    (keypair, did)
}

#[test]
fn test_cooperative_info_creation() {
    let (_keypair, did) = test_keypair();
    
    let info = CooperativeInfo::new(
        "food-coop".to_string(),
        "Food Co-op".to_string(),
        did.clone(),
        FederationPolicy::Open,
    )
    .with_gateway("https://food-coop.icn".to_string())
    .with_capability("clearing");
    
    assert_eq!(info.coop_id, "food-coop");
    assert_eq!(info.name, "Food Co-op");
    assert_eq!(info.gateway_endpoints.len(), 1);
    assert!(info.has_capability("clearing"));
}

#[test]
fn test_cooperative_info_signature() {
    let (keypair, did) = test_keypair();
    
    let info = CooperativeInfo::new(
        "tech-coop".to_string(),
        "Tech Cooperative".to_string(),
        did,
        FederationPolicy::Open,
    )
    .sign(&keypair);
    
    assert!(!info.signature.is_empty());
    assert!(info.verify_signature().is_ok());
}

#[test]
fn test_federation_policy_open() {
    let policy = FederationPolicy::Open;
    assert_eq!(policy, FederationPolicy::Open);
    assert!(policy.allows_federation());
}

#[test]
fn test_federation_policy_vouched() {
    let policy = FederationPolicy::vouched(3);
    
    match policy {
        FederationPolicy::Vouched { min_vouches } => {
            assert_eq!(min_vouches, 3);
        }
        _ => panic!("Wrong policy type"),
    }
    assert!(policy.allows_federation());
}

#[test]
fn test_federation_policy_closed() {
    let policy = FederationPolicy::Closed;
    assert_eq!(policy, FederationPolicy::Closed);
    assert!(!policy.allows_federation());
}

#[test]
fn test_currency_info() {
    let currency = CurrencyInfo::new("FCC", "Food Coop Credits", 2);
    
    assert_eq!(currency.symbol, "FCC");
    assert_eq!(currency.decimals, 2);
}

#[test]
fn test_currency_info_hours() {
    let hours = CurrencyInfo::hours();
    assert_eq!(hours.symbol, "hours");
    assert_eq!(hours.decimals, 2);
}

#[test]
fn test_currency_info_usd() {
    let usd = CurrencyInfo::usd();
    assert_eq!(usd.symbol, "USD");
    assert_eq!(usd.decimals, 2);
}

#[test]
fn test_vouch_creation() {
    let (_keypair, did) = test_keypair();
    
    let vouch = Vouch::new(
        "voucher-coop".to_string(),
        did.clone(),
        "new-coop".to_string(),
        0.8,
    );
    
    assert_eq!(vouch.target_coop_id, "new-coop");
    assert_eq!(vouch.trust_score, 0.8);
}

#[test]
fn test_vouch_signature() {
    let (keypair, did) = test_keypair();
    
    let vouch = Vouch::new(
        "voucher-coop".to_string(),
        did,
        "target-coop".to_string(),
        0.9,
    )
    .sign(&keypair);
    
    assert!(!vouch.signature.is_empty());
    assert!(vouch.verify_signature().is_ok());
}

#[test]
fn test_vouch_expiry() {
    let (_keypair, did) = test_keypair();
    
    let vouch = Vouch::new(
        "voucher-coop".to_string(),
        did,
        "target-coop".to_string(),
        0.9,
    )
    .with_expiry(0); // Expired
    
    assert!(vouch.is_expired());
}

#[test]
fn test_bilateral_clearing_agreement() {
    let (_keypair1, did1) = test_keypair();
    let (_keypair2, did2) = test_keypair();
    
    let agreement = BilateralClearingAgreement::new(
        "agreement-1".to_string(),
        "coop-1".to_string(),
        did1,
        "coop-2".to_string(),
        did2,
    )
    .with_rate("hours", "USD", 25.0)
    .with_interval(SettlementInterval::Weekly)
    .with_max_imbalance(10000);
    
    assert_eq!(agreement.coop_a, "coop-1");
    assert_eq!(agreement.max_imbalance, 10000);
    assert_eq!(agreement.get_rate("hours", "USD"), Some(25.0));
}

#[test]
fn test_settlement_interval() {
    assert_eq!(SettlementInterval::Weekly.seconds(), Some(604800));
}

#[test]
fn test_gossip_scope() {
    let local = GossipScope::Local;
    assert_eq!(local, GossipScope::Local);
}

#[test]
fn test_trust_attestation() {
    let (_keypair, did1) = test_keypair();
    let (_keypair2, did2) = test_keypair();
    
    let evidence = vec![
        EvidenceSummary::new("transactions", "Transaction history").with_count(10),
        EvidenceSummary::new("disputes", "Dispute record").with_count(0),
    ];
    
    let attestation = FederatedTrustAttestation {
        source_coop_id: "coop-1".to_string(),
        source_coop_did: did1,
        member_did: did2,
        trust_score: 0.8,
        trust_context: TrustContext::Economic,
        evidence_summary: evidence.clone(),
        issued_at: 0,
        expires_at: 2592000,
        signature: vec![],
    };
    
    assert_eq!(attestation.trust_score, 0.8);
    assert_eq!(attestation.evidence_summary.len(), 2);
}

#[test]
fn test_evidence_summary() {
    let evidence = EvidenceSummary::new("transactions", "Transaction count").with_count(50);
    
    assert_eq!(evidence.kind, "transactions");
    assert_eq!(evidence.count, Some(50));
}

#[test]
fn test_trust_context() {
    assert_eq!(TrustContext::Economic.as_str(), "economic");
    assert_eq!(TrustContext::Social.as_str(), "social");
    assert_eq!(TrustContext::Governance.as_str(), "governance");
    assert_eq!(TrustContext::General.as_str(), "general");
}

#[test]
fn test_topic_constants() {
    assert_eq!(TOPIC_FEDERATION_REGISTRY, "federation:registry");
    assert_eq!(TOPIC_FEDERATION_TRUST, "federation:trust");
    assert_eq!(TOPIC_FEDERATION_CLEARING, "federation:clearing");
}
