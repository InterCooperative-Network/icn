#!/usr/bin/env python3
"""
Sign and emit a service announcement JSON body for the ICN gateway.

Usage:
    python3 sign_service_announce.py <service_id>
    SVC_ID=my-service python3 sign_service_announce.py

Outputs: JSON suitable for POST /v1/services/announce (to stdout).

Generates a fresh ephemeral Ed25519 keypair per invocation. The provider DID
encodes the public key so the gateway can verify the signature without a
separate key registry.

Requires: python3 + cryptography library (pip install cryptography)
"""
import json
import os
import struct
import sys
import time

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat

# ── Base58btc encoding (multibase 'z' prefix) ─────────────────────────────────
_B58_ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"


def _b58enc(data: bytes) -> str:
    n = int.from_bytes(data, "big")
    result = []
    while n > 0:
        n, r = divmod(n, 58)
        result.append(_B58_ALPHABET[r])
    leading = 0
    for b in data:
        if b == 0:
            leading += 1
        else:
            break
    return _B58_ALPHABET[0] * leading + "".join(reversed(result))


def _make_did(pub_bytes: bytes) -> str:
    """Encode 32-byte Ed25519 public key as did:icn:<multibase-base58btc>."""
    return "did:icn:z" + _b58enc(pub_bytes)


# ── Length-prefixed field helper ──────────────────────────────────────────────
def _lp(buf: bytes, s: str) -> bytes:
    enc = s.encode("utf-8")
    return buf + struct.pack("<I", len(enc)) + enc


# ── Canonical byte mappings matching icn-kernel-api ──────────────────────────
# EndpointType::to_canonical_byte(): Quic=0, Http=1, Grpc=2, WebSocket=3
_ENDPOINT_TYPE_BYTE = {"quic": 0, "http": 1, "grpc": 2, "websocket": 3}

# ScopeLevel as u8: Local=0, Cell=1, Org=2, Federation=3, Commons=4
_SCOPE_BYTE = {"local": 0, "cell": 1, "org": 2, "federation": 3, "commons": 4}


def _build_signing_payload(
    service_id: str,
    provider: str,
    endpoint_type: str,
    service_type: str,
    service_version: str,
    endpoints: list,
    addresses: list,
    capabilities: list,
    trust_threshold: float,
    scope_visibility: str,
    ttl_secs: int,
    created_at: int,
    updated_at: int,
) -> bytes:
    """
    Build the canonical binary signing payload matching
    ServiceEndpoint::signing_payload() in icn-kernel-api/src/naming.rs.

    Fields are length-prefixed (u32 LE) to prevent ambiguity attacks.
    """
    buf = b""
    buf = _lp(buf, service_id)
    buf = _lp(buf, provider)
    buf += bytes([_ENDPOINT_TYPE_BYTE.get(endpoint_type, 1)])
    buf = _lp(buf, service_type)
    buf = _lp(buf, service_version)

    # Endpoints: count then each endpoint
    buf += struct.pack("<I", len(endpoints))
    for ep in endpoints:
        buf = _lp(buf, ep["protocol"])
        buf = _lp(buf, ep["host"])
        buf += struct.pack("<H", ep["port"])
        path = ep.get("path")
        if path is not None:
            buf += b"\x01"
            buf = _lp(buf, path)
        else:
            buf += b"\x00"

    # Addresses and capabilities: sorted for determinism
    for lst in [sorted(addresses), sorted(capabilities)]:
        buf += struct.pack("<I", len(lst))
        for item in lst:
            buf = _lp(buf, item)

    buf += struct.pack("<d", trust_threshold)
    buf += bytes([_SCOPE_BYTE.get(scope_visibility.lower(), 2)])
    buf += b"\x00"  # no cell_id
    buf += struct.pack("<Q", ttl_secs)
    buf += struct.pack("<Q", created_at)
    buf += struct.pack("<Q", updated_at)
    return buf


def build_announce_request(service_id: str) -> dict:
    """
    Generate an ephemeral keypair, build and sign an AnnounceRequest.
    Returns the request dict ready for JSON serialization.
    """
    now = int(time.time())
    endpoint_type = "http"
    service_type = "ledger"
    service_version = "1.0"
    endpoints = [{"protocol": "http", "host": "node-a.devnet", "port": 8080}]
    addresses: list = []
    capabilities = ["read", "write"]
    trust_threshold = 0.0
    scope_visibility = "org"
    ttl_secs = 3600

    # Generate ephemeral Ed25519 keypair; derive provider DID from public key
    priv = Ed25519PrivateKey.generate()
    pub_bytes = priv.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)
    provider = _make_did(pub_bytes)

    payload = _build_signing_payload(
        service_id,
        provider,
        endpoint_type,
        service_type,
        service_version,
        endpoints,
        addresses,
        capabilities,
        trust_threshold,
        scope_visibility,
        ttl_secs,
        now,
        now,
    )
    sig = priv.sign(payload)

    return {
        "service_id": service_id,
        "provider": provider,
        "endpoint_type": endpoint_type,
        "service_type": service_type,
        "service_version": service_version,
        "endpoints": endpoints,
        "addresses": addresses,
        "capabilities": capabilities,
        "trust_threshold": trust_threshold,
        "scope_visibility": scope_visibility,
        "ttl_secs": ttl_secs,
        "created_at": now,
        "updated_at": now,
        "signature": sig.hex(),
    }


if __name__ == "__main__":
    service_id = (
        sys.argv[1]
        if len(sys.argv) > 1
        else os.environ.get("SVC_ID", f"demo-ledger-{int(time.time())}")
    )
    req = build_announce_request(service_id)
    print(json.dumps(req))
