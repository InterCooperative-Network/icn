//! ICN naming service implementation.
//!
//! Provides a sled-backed implementation of `NamingService` with:
//! - authority signature enforcement on register/update/delete
//! - deterministic signature payload construction
//! - hierarchical name validation (`/cell|org|fed|commons/...`)
//! - alias-aware resolution with bounded recursion

#![deny(clippy::unwrap_used, clippy::expect_used)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use anyhow::{Context, Result};
use ed25519_dalek::Verifier;
use icn_kernel_api::naming::{NameRecord, NamingError, NamingService, ResolveOptions, Target};
use icn_kernel_api::types::{Did, Duration, Endpoint, Name, Namespace, Signature, Subscription};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

const DOMAIN_REGISTER: &[u8] = b"icn:naming:register:v1";
const DOMAIN_UPDATE: &[u8] = b"icn:naming:update:v1";
const DOMAIN_DELETE: &[u8] = b"icn:naming:delete:v1";

const ROOT_CELL: &str = "cell";
const ROOT_ORG: &str = "org";
const ROOT_FED: &str = "fed";
const ROOT_COMMONS: &str = "commons";

/// Sled-backed naming store.
pub struct SledNamingService<S: icn_store::Store> {
    store: Arc<S>,
}

impl<S: icn_store::Store> SledNamingService<S> {
    const PREFIX: &'static [u8] = b"naming:record:";

    /// Create a naming service backed by the provided key/value store.
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    fn record_key(name: &Name) -> Vec<u8> {
        let mut key = Self::PREFIX.to_vec();
        key.extend_from_slice(name.as_str().as_bytes());
        key
    }

    fn now_secs() -> u64 {
        match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(duration) => duration.as_secs(),
            Err(error) => {
                eprintln!("icn-naming: failed to read system clock for timestamp: {error}");
                0
            }
        }
    }

    fn validate_name(name: &Name) -> Result<(), NamingError> {
        let raw = name.as_str();
        if !raw.starts_with('/') {
            return Err(NamingError::InvalidName(
                "name must start with '/'".to_string(),
            ));
        }

        let parts: Vec<&str> = raw.split('/').filter(|p| !p.is_empty()).collect();
        if parts.len() < 2 {
            return Err(NamingError::InvalidName(
                "name must have at least two segments".to_string(),
            ));
        }

        match parts[0] {
            ROOT_CELL | ROOT_ORG | ROOT_FED | ROOT_COMMONS => {}
            _ => {
                return Err(NamingError::InvalidName(
                    "root segment must be one of: cell, org, fed, commons".to_string(),
                ));
            }
        }

        for seg in parts {
            if !seg
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
            {
                return Err(NamingError::InvalidName(format!(
                    "invalid segment '{seg}': allowed chars are [a-zA-Z0-9._-]"
                )));
            }
        }

        Ok(())
    }

    fn parse_authority(authority: &Did) -> Result<icn_identity::Did, NamingError> {
        icn_identity::Did::from_str(authority)
            .map_err(|e| NamingError::Unauthorized(format!("invalid authority DID: {e}")))
    }

    fn verify_signature(
        authority: &Did,
        signature: &Signature,
        payload: &[u8],
        label: &str,
    ) -> Result<(), NamingError> {
        let did = Self::parse_authority(authority)?;
        let verifying_key = did.to_verifying_key().map_err(|e| {
            NamingError::InvalidSignature(format!(
                "failed to decode authority key for {label}: {e}"
            ))
        })?;

        let sig_bytes: [u8; 64] = signature.as_bytes().try_into().map_err(|_| {
            NamingError::InvalidSignature(format!("invalid signature length for {label}"))
        })?;
        let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);

        verifying_key.verify(payload, &sig).map_err(|e| {
            NamingError::InvalidSignature(format!("signature verification failed for {label}: {e}"))
        })
    }

    fn append_len_prefixed(buf: &mut Vec<u8>, bytes: &[u8]) {
        buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(bytes);
    }

    fn append_opt_str(buf: &mut Vec<u8>, value: Option<&str>) {
        match value {
            Some(v) => {
                buf.push(1);
                Self::append_len_prefixed(buf, v.as_bytes());
            }
            None => buf.push(0),
        }
    }

    fn append_endpoint(buf: &mut Vec<u8>, ep: &Endpoint) {
        Self::append_len_prefixed(buf, ep.protocol.as_bytes());
        Self::append_len_prefixed(buf, ep.host.as_bytes());
        buf.extend_from_slice(&ep.port.to_le_bytes());
        Self::append_opt_str(buf, ep.path.as_deref());
    }

    fn target_payload(target: &Target) -> Vec<u8> {
        let mut out = Vec::new();
        match target {
            Target::Service { endpoint } => {
                out.push(0);
                Self::append_endpoint(&mut out, endpoint);
            }
            Target::Blob { hash } => {
                out.push(1);
                out.extend_from_slice(hash);
            }
            Target::Namespace { ns } => {
                out.push(2);
                Self::append_len_prefixed(&mut out, ns.org.as_bytes());
                Self::append_len_prefixed(&mut out, ns.app.as_bytes());
                Self::append_opt_str(&mut out, ns.sub.as_deref());
            }
            Target::Alias { name } => {
                out.push(3);
                Self::append_len_prefixed(&mut out, name.as_str().as_bytes());
            }
            Target::MultiService { endpoints } => {
                out.push(4);
                out.extend_from_slice(&(endpoints.len() as u32).to_le_bytes());
                for ep in endpoints {
                    Self::append_endpoint(&mut out, ep);
                }
            }
        }
        out
    }

    fn register_payload(name: &Name, target: &Target, ttl: Duration) -> Vec<u8> {
        let mut out = Vec::new();
        Self::append_len_prefixed(&mut out, DOMAIN_REGISTER);
        Self::append_len_prefixed(&mut out, name.as_str().as_bytes());
        out.extend_from_slice(&ttl.as_secs().to_le_bytes());
        Self::append_len_prefixed(&mut out, &Self::target_payload(target));
        out
    }

    fn update_payload(name: &Name, target: &Target) -> Vec<u8> {
        let mut out = Vec::new();
        Self::append_len_prefixed(&mut out, DOMAIN_UPDATE);
        Self::append_len_prefixed(&mut out, name.as_str().as_bytes());
        Self::append_len_prefixed(&mut out, &Self::target_payload(target));
        out
    }

    fn delete_payload(name: &Name, target: &Target) -> Vec<u8> {
        let mut out = Vec::new();
        Self::append_len_prefixed(&mut out, DOMAIN_DELETE);
        Self::append_len_prefixed(&mut out, name.as_str().as_bytes());
        Self::append_len_prefixed(&mut out, &Self::target_payload(target));
        out
    }

    fn load_record(&self, name: &Name) -> Result<Option<NameRecord>, NamingError> {
        let key = Self::record_key(name);
        let maybe_raw = self
            .store
            .get(&key)
            .context("failed to read naming record")
            .map_err(|e| NamingError::Internal(e.to_string()))?;
        match maybe_raw {
            Some(raw) => {
                let persisted: PersistedNameRecord = serde_json::from_slice(&raw)
                    .context("failed to deserialize naming record")
                    .map_err(|e| NamingError::Internal(e.to_string()))?;
                Ok(Some(persisted.into_name_record()))
            }
            None => Ok(None),
        }
    }

    fn save_record(&self, record: &NameRecord) -> Result<(), NamingError> {
        let key = Self::record_key(&record.name);
        let persisted = PersistedNameRecord::from_name_record(record);
        let raw = serde_json::to_vec(&persisted)
            .context("failed to serialize naming record")
            .map_err(|e| NamingError::Internal(e.to_string()))?;
        self.store
            .put(&key, &raw)
            .context("failed to store naming record")
            .map_err(|e| NamingError::Internal(e.to_string()))
    }

    fn resolve_record_recursive(
        &self,
        name: &Name,
        max_depth: u32,
    ) -> Result<(Target, NameRecord), NamingError> {
        let mut current = name.clone();
        for _ in 0..=max_depth {
            let record = self
                .load_record(&current)?
                .ok_or_else(|| NamingError::NotFound(current.as_str().to_string()))?;

            match &record.target {
                Target::Alias { name: alias } => {
                    current = alias.clone();
                }
                other => return Ok((other.clone(), record)),
            }
        }

        Err(NamingError::TooManyRedirects(max_depth))
    }
}

impl<S: icn_store::Store> NamingService for SledNamingService<S> {
    fn register(
        &self,
        name: &Name,
        target: Target,
        authority: &Did,
        signature: &Signature,
        ttl: Duration,
    ) -> Result<NameRecord, NamingError> {
        Self::validate_name(name)?;

        if self.load_record(name)?.is_some() {
            return Err(NamingError::AlreadyExists(name.as_str().to_string()));
        }

        let payload = Self::register_payload(name, &target, ttl);
        Self::verify_signature(authority, signature, &payload, "register")?;

        let now = Self::now_secs();
        let record = NameRecord {
            name: name.clone(),
            target,
            authority: authority.clone(),
            signature: signature.clone(),
            ttl,
            created_at: now,
            updated_at: now,
            metadata: HashMap::new(),
        };

        self.save_record(&record)?;
        Ok(record)
    }

    fn resolve(&self, name: &Name) -> Result<Target, NamingError> {
        let (target, _) = self.resolve_with_options(name, ResolveOptions::default())?;
        Ok(target)
    }

    fn resolve_with_options(
        &self,
        name: &Name,
        options: ResolveOptions,
    ) -> Result<(Target, NameRecord), NamingError> {
        Self::validate_name(name)?;
        // `scope` and `allow_cached` are reserved for distributed resolvers;
        // this store-backed implementation resolves directly from local storage.
        let _ = (&options.scope, options.allow_cached);
        let max_depth = options.max_depth.unwrap_or(10);
        let (target, record) = self.resolve_record_recursive(name, max_depth)?;

        if options.verify_signatures && !self.verify(&record)? {
            return Err(NamingError::InvalidSignature(name.as_str().to_string()));
        }

        Ok((target, record))
    }

    fn update(
        &self,
        name: &Name,
        new_target: Target,
        signature: &Signature,
    ) -> Result<NameRecord, NamingError> {
        Self::validate_name(name)?;

        let mut existing = self
            .load_record(name)?
            .ok_or_else(|| NamingError::NotFound(name.as_str().to_string()))?;

        let payload = Self::update_payload(name, &new_target);
        Self::verify_signature(&existing.authority, signature, &payload, "update")?;

        existing.target = new_target;
        existing.signature = signature.clone();
        existing.updated_at = Self::now_secs();

        self.save_record(&existing)?;
        Ok(existing)
    }

    fn delete(&self, name: &Name, signature: &Signature) -> Result<(), NamingError> {
        Self::validate_name(name)?;

        let existing = self
            .load_record(name)?
            .ok_or_else(|| NamingError::NotFound(name.as_str().to_string()))?;

        let payload = Self::delete_payload(name, &existing.target);
        Self::verify_signature(&existing.authority, signature, &payload, "delete")?;

        let key = Self::record_key(name);
        self.store
            .delete(&key)
            .context("failed to delete naming record")
            .map_err(|e| NamingError::Internal(e.to_string()))
    }

    fn get_record(&self, name: &Name) -> Result<NameRecord, NamingError> {
        Self::validate_name(name)?;
        self.load_record(name)?
            .ok_or_else(|| NamingError::NotFound(name.as_str().to_string()))
    }

    fn list(&self, prefix: &Name) -> Result<Vec<Name>, NamingError> {
        if !prefix.as_str().starts_with('/') {
            return Err(NamingError::InvalidName(
                "prefix must start with '/'".to_string(),
            ));
        }

        let entries = self
            .store
            .scan(Self::PREFIX)
            .context("failed to list naming records")
            .map_err(|e| NamingError::Internal(e.to_string()))?;

        let mut names = Vec::new();
        for (key, _) in entries {
            if let Some(rest) = key.strip_prefix(Self::PREFIX) {
                if let Ok(name_str) = std::str::from_utf8(rest) {
                    if name_str.starts_with(prefix.as_str()) {
                        names.push(Name::new(name_str));
                    }
                }
            }
        }

        names.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        Ok(names)
    }

    fn watch(&self, name: &Name) -> Result<Subscription, NamingError> {
        Self::validate_name(name)?;
        Err(NamingError::Internal(
            "watch is not yet implemented for SledNamingService".to_string(),
        ))
    }

    fn verify(&self, record: &NameRecord) -> Result<bool, NamingError> {
        let register_payload = Self::register_payload(&record.name, &record.target, record.ttl);
        match Self::verify_signature(
            &record.authority,
            &record.signature,
            &register_payload,
            "verify(register)",
        ) {
            Ok(()) => Ok(true),
            Err(NamingError::InvalidSignature(_)) => {
                // Updated records are signed with update domain separation.
                let update_payload = Self::update_payload(&record.name, &record.target);
                match Self::verify_signature(
                    &record.authority,
                    &record.signature,
                    &update_payload,
                    "verify(update)",
                ) {
                    Ok(()) => Ok(true),
                    Err(NamingError::InvalidSignature(_)) => Ok(false),
                    Err(other) => Err(other),
                }
            }
            Err(other) => Err(other),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PersistedNameRecord {
    name: String,
    target: TargetRepr,
    authority: String,
    signature: Vec<u8>,
    ttl_secs: u64,
    created_at: u64,
    updated_at: u64,
    metadata: HashMap<String, String>,
}

impl PersistedNameRecord {
    fn from_name_record(record: &NameRecord) -> Self {
        Self {
            name: record.name.as_str().to_string(),
            target: TargetRepr::from_target(&record.target),
            authority: record.authority.clone(),
            signature: record.signature.as_bytes().to_vec(),
            ttl_secs: record.ttl.as_secs(),
            created_at: record.created_at,
            updated_at: record.updated_at,
            metadata: record.metadata.clone(),
        }
    }

    fn into_name_record(self) -> NameRecord {
        NameRecord {
            name: Name::new(self.name),
            target: self.target.into_target(),
            authority: self.authority,
            signature: Signature::new(self.signature),
            ttl: Duration::from_secs(self.ttl_secs),
            created_at: self.created_at,
            updated_at: self.updated_at,
            metadata: self.metadata,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum TargetRepr {
    Service { endpoint: Endpoint },
    Blob { hash: [u8; 32] },
    Namespace { ns: Namespace },
    Alias { name: String },
    MultiService { endpoints: Vec<Endpoint> },
}

impl TargetRepr {
    fn from_target(target: &Target) -> Self {
        match target {
            Target::Service { endpoint } => Self::Service {
                endpoint: endpoint.clone(),
            },
            Target::Blob { hash } => Self::Blob { hash: *hash },
            Target::Namespace { ns } => Self::Namespace { ns: ns.clone() },
            Target::Alias { name } => Self::Alias {
                name: name.as_str().to_string(),
            },
            Target::MultiService { endpoints } => Self::MultiService {
                endpoints: endpoints.clone(),
            },
        }
    }

    fn into_target(self) -> Target {
        match self {
            Self::Service { endpoint } => Target::Service { endpoint },
            Self::Blob { hash } => Target::Blob { hash },
            Self::Namespace { ns } => Target::Namespace { ns },
            Self::Alias { name } => Target::Alias {
                name: Name::new(name),
            },
            Self::MultiService { endpoints } => Target::MultiService { endpoints },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Signer;
    use icn_store::SledStore;

    fn test_service() -> SledNamingService<SledStore> {
        let store = Arc::new(SledStore::temporary().unwrap());
        SledNamingService::new(store)
    }

    fn sign_register(
        signing_key: &ed25519_dalek::SigningKey,
        name: &Name,
        target: &Target,
        ttl: Duration,
    ) -> Signature {
        let payload = SledNamingService::<SledStore>::register_payload(name, target, ttl);
        Signature::new(signing_key.sign(&payload).to_bytes().to_vec())
    }

    fn sign_update(
        signing_key: &ed25519_dalek::SigningKey,
        name: &Name,
        target: &Target,
    ) -> Signature {
        let payload = SledNamingService::<SledStore>::update_payload(name, target);
        Signature::new(signing_key.sign(&payload).to_bytes().to_vec())
    }

    fn sign_delete(
        signing_key: &ed25519_dalek::SigningKey,
        name: &Name,
        target: &Target,
    ) -> Signature {
        let payload = SledNamingService::<SledStore>::delete_payload(name, target);
        Signature::new(signing_key.sign(&payload).to_bytes().to_vec())
    }

    #[test]
    fn naming_register_and_resolve_roundtrip() {
        let service = test_service();
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let name = Name::new("/org/demo/ledger");
        let target = Target::Service {
            endpoint: Endpoint::new("https", "ledger.demo", 443),
        };
        let ttl = Duration::from_secs(300);

        let signing_key = ed25519_dalek::SigningKey::from_bytes(&keypair.to_signing_key_bytes());
        let signature = sign_register(&signing_key, &name, &target, ttl);

        let record = service
            .register(
                &name,
                target.clone(),
                &keypair.did().to_string(),
                &signature,
                ttl,
            )
            .unwrap();

        assert_eq!(record.name.as_str(), "/org/demo/ledger");

        let resolved = service.resolve(&name).unwrap();
        match resolved {
            Target::Service { endpoint } => {
                assert_eq!(endpoint.host, "ledger.demo");
                assert_eq!(endpoint.port, 443);
            }
            _ => panic!("expected service endpoint"),
        }
    }

    #[test]
    fn naming_requires_authority_signature() {
        let service = test_service();
        let authority = icn_identity::KeyPair::generate().unwrap();
        let attacker = icn_identity::KeyPair::generate().unwrap();

        let name = Name::new("/org/demo/treasury");
        let target = Target::Service {
            endpoint: Endpoint::new("https", "treasury.demo", 443),
        };
        let ttl = Duration::from_secs(300);

        let attacker_key = ed25519_dalek::SigningKey::from_bytes(&attacker.to_signing_key_bytes());
        let forged_sig = sign_register(&attacker_key, &name, &target, ttl);

        let err = service
            .register(
                &name,
                target,
                &authority.did().to_string(),
                &forged_sig,
                ttl,
            )
            .unwrap_err();

        match err {
            NamingError::InvalidSignature(_) => {}
            other => panic!("expected InvalidSignature, got {other:?}"),
        }
    }

    #[test]
    fn validate_name_rules() {
        let valid = Name::new("/org/demo.service/api");
        assert!(SledNamingService::<SledStore>::validate_name(&valid).is_ok());

        let err =
            SledNamingService::<SledStore>::validate_name(&Name::new("org/demo")).unwrap_err();
        assert!(matches!(err, NamingError::InvalidName(msg) if msg.contains("start with '/'")));

        let err =
            SledNamingService::<SledStore>::validate_name(&Name::new("/org-only")).unwrap_err();
        assert!(
            matches!(err, NamingError::InvalidName(msg) if msg.contains("at least two segments"))
        );

        let err =
            SledNamingService::<SledStore>::validate_name(&Name::new("/team/demo")).unwrap_err();
        assert!(matches!(err, NamingError::InvalidName(msg) if msg.contains("root segment")));

        let err = SledNamingService::<SledStore>::validate_name(&Name::new("/org/demo$/svc"))
            .unwrap_err();
        assert!(matches!(err, NamingError::InvalidName(msg) if msg.contains("invalid segment")));
    }

    #[test]
    fn update_requires_valid_authority_signature_and_verify_accepts_updated_record() {
        let service = test_service();
        let authority = icn_identity::KeyPair::generate().unwrap();
        let attacker = icn_identity::KeyPair::generate().unwrap();
        let name = Name::new("/org/demo/router");
        let ttl = Duration::from_secs(300);

        let initial_target = Target::Service {
            endpoint: Endpoint::new("https", "router-v1.demo", 443),
        };
        let updated_target = Target::Service {
            endpoint: Endpoint::new("https", "router-v2.demo", 443),
        };

        let authority_key =
            ed25519_dalek::SigningKey::from_bytes(&authority.to_signing_key_bytes());
        let attacker_key = ed25519_dalek::SigningKey::from_bytes(&attacker.to_signing_key_bytes());

        let register_sig = sign_register(&authority_key, &name, &initial_target, ttl);
        service
            .register(
                &name,
                initial_target,
                &authority.did().to_string(),
                &register_sig,
                ttl,
            )
            .unwrap();

        let update_sig = sign_update(&authority_key, &name, &updated_target);
        let updated = service
            .update(&name, updated_target.clone(), &update_sig)
            .unwrap();

        match updated.target {
            Target::Service { ref endpoint } => assert_eq!(endpoint.host, "router-v2.demo"),
            other => panic!("expected service target, got {other:?}"),
        }

        assert!(service.verify(&updated).unwrap());

        let forged_update = sign_update(&attacker_key, &name, &updated_target);
        let err = service
            .update(&name, updated_target, &forged_update)
            .unwrap_err();
        assert!(matches!(err, NamingError::InvalidSignature(_)));
    }

    #[test]
    fn delete_requires_valid_signature() {
        let service = test_service();
        let authority = icn_identity::KeyPair::generate().unwrap();
        let attacker = icn_identity::KeyPair::generate().unwrap();
        let name = Name::new("/org/demo/treasury");
        let target = Target::Service {
            endpoint: Endpoint::new("https", "treasury.demo", 443),
        };
        let ttl = Duration::from_secs(300);

        let authority_key =
            ed25519_dalek::SigningKey::from_bytes(&authority.to_signing_key_bytes());
        let attacker_key = ed25519_dalek::SigningKey::from_bytes(&attacker.to_signing_key_bytes());

        let register_sig = sign_register(&authority_key, &name, &target, ttl);
        service
            .register(
                &name,
                target.clone(),
                &authority.did().to_string(),
                &register_sig,
                ttl,
            )
            .unwrap();

        let forged_delete = sign_delete(&attacker_key, &name, &target);
        let err = service.delete(&name, &forged_delete).unwrap_err();
        assert!(matches!(err, NamingError::InvalidSignature(_)));

        let delete_sig = sign_delete(&authority_key, &name, &target);
        service.delete(&name, &delete_sig).unwrap();

        let err = service.get_record(&name).unwrap_err();
        assert!(matches!(err, NamingError::NotFound(_)));
    }

    #[test]
    fn resolve_alias_chain_and_enforce_max_depth() {
        let service = test_service();
        let authority = icn_identity::KeyPair::generate().unwrap();
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&authority.to_signing_key_bytes());
        let ttl = Duration::from_secs(300);

        let a = Name::new("/org/demo/a");
        let b = Name::new("/org/demo/b");
        let c = Name::new("/org/demo/c");

        let a_target = Target::Alias { name: b.clone() };
        let b_target = Target::Alias { name: c.clone() };
        let c_target = Target::Service {
            endpoint: Endpoint::new("https", "final.demo", 443),
        };

        service
            .register(
                &a,
                a_target.clone(),
                &authority.did().to_string(),
                &sign_register(&signing_key, &a, &a_target, ttl),
                ttl,
            )
            .unwrap();
        service
            .register(
                &b,
                b_target.clone(),
                &authority.did().to_string(),
                &sign_register(&signing_key, &b, &b_target, ttl),
                ttl,
            )
            .unwrap();
        service
            .register(
                &c,
                c_target.clone(),
                &authority.did().to_string(),
                &sign_register(&signing_key, &c, &c_target, ttl),
                ttl,
            )
            .unwrap();

        let (resolved, record) = service
            .resolve_with_options(&a, ResolveOptions::new().with_max_depth(5))
            .unwrap();

        assert_eq!(record.name.as_str(), "/org/demo/c");
        assert!(matches!(resolved, Target::Service { .. }));

        let err = service
            .resolve_with_options(&a, ResolveOptions::new().with_max_depth(1))
            .unwrap_err();
        assert!(matches!(err, NamingError::TooManyRedirects(1)));
    }

    #[test]
    fn circular_alias_hits_redirect_limit() {
        let service = test_service();
        let authority = icn_identity::KeyPair::generate().unwrap();
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&authority.to_signing_key_bytes());
        let ttl = Duration::from_secs(300);

        let a = Name::new("/org/demo/x");
        let b = Name::new("/org/demo/y");

        let a_target = Target::Alias { name: b.clone() };
        let b_target = Target::Alias { name: a.clone() };

        service
            .register(
                &a,
                a_target.clone(),
                &authority.did().to_string(),
                &sign_register(&signing_key, &a, &a_target, ttl),
                ttl,
            )
            .unwrap();
        service
            .register(
                &b,
                b_target.clone(),
                &authority.did().to_string(),
                &sign_register(&signing_key, &b, &b_target, ttl),
                ttl,
            )
            .unwrap();

        let err = service
            .resolve_with_options(&a, ResolveOptions::new().with_max_depth(3))
            .unwrap_err();
        assert!(matches!(err, NamingError::TooManyRedirects(3)));
    }

    #[test]
    fn list_filters_by_prefix_and_is_sorted() {
        let service = test_service();
        let authority = icn_identity::KeyPair::generate().unwrap();
        let signing_key = ed25519_dalek::SigningKey::from_bytes(&authority.to_signing_key_bytes());
        let ttl = Duration::from_secs(300);

        let names = vec![
            Name::new("/org/demo/zeta"),
            Name::new("/org/demo/alpha"),
            Name::new("/org/other/beta"),
            Name::new("/org/demo/gamma"),
        ];

        for name in &names {
            let target = Target::Service {
                endpoint: Endpoint::new("https", "svc.demo", 443),
            };
            let sig = sign_register(&signing_key, name, &target, ttl);
            service
                .register(name, target, &authority.did().to_string(), &sig, ttl)
                .unwrap();
        }

        let listed = service.list(&Name::new("/org/demo")).unwrap();
        let listed: Vec<String> = listed.into_iter().map(|n| n.as_str().to_string()).collect();
        assert_eq!(
            listed,
            vec![
                "/org/demo/alpha".to_string(),
                "/org/demo/gamma".to_string(),
                "/org/demo/zeta".to_string(),
            ]
        );
    }
}
