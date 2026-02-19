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
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
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
        Ok(Subscription {
            id: format!("naming:{}", name.as_str()),
        })
    }

    fn verify(&self, record: &NameRecord) -> Result<bool, NamingError> {
        let payload = Self::register_payload(&record.name, &record.target, record.ttl);
        match Self::verify_signature(&record.authority, &record.signature, &payload, "verify") {
            Ok(()) => Ok(true),
            Err(NamingError::InvalidSignature(_)) => Ok(false),
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
}
