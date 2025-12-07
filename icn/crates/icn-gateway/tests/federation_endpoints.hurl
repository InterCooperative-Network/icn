# Federation API Endpoint Tests
# Run with: hurl --test federation_endpoints.hurl --variable host=http://localhost:8080
#
# Prerequisites:
# 1. Gateway running: icnd --gateway-enable --gateway-bind 127.0.0.1:8080 --gateway-jwt-secret "test-secret"
# 2. Valid JWT token (replace {{token}} with actual token from /v1/auth/verify)

# ===========================================================================
# Health Check (no auth required)
# ===========================================================================

GET {{host}}/v1/health
HTTP 200
[Asserts]
jsonpath "$.status" == "ok"

# ===========================================================================
# Authentication Flow
# ===========================================================================

# Step 1: Get challenge
POST {{host}}/v1/auth/challenge
Content-Type: application/json
{
  "did": "{{did}}"
}
HTTP 200
[Captures]
challenge: jsonpath "$.challenge"

# Step 2: Verify (requires signature - manual step)
# POST {{host}}/v1/auth/verify
# Content-Type: application/json
# {
#   "did": "{{did}}",
#   "signature": "<sign challenge with DID key>",
#   "coop_id": "test-coop",
#   "scopes": ["federation:read", "federation:write", "federation:admin"]
# }

# ===========================================================================
# Federation Status (requires federation:read scope)
# ===========================================================================

GET {{host}}/v1/federation/status
Authorization: Bearer {{token}}
HTTP 200
[Asserts]
jsonpath "$.initialized" exists

# ===========================================================================
# Initialize Federation (requires federation:admin scope)
# ===========================================================================

POST {{host}}/v1/federation/init
Authorization: Bearer {{token}}
Content-Type: application/json
{
  "coop_id": "test-coop",
  "name": "Test Cooperative",
  "gateway_endpoint": "http://localhost:8080"
}
HTTP 200
[Asserts]
jsonpath "$.status" == "initialized"

# ===========================================================================
# Cooperative Registry
# ===========================================================================

# List cooperatives
GET {{host}}/v1/federation/coops
Authorization: Bearer {{token}}
HTTP 200

# Register a new cooperative
POST {{host}}/v1/federation/coops
Authorization: Bearer {{token}}
Content-Type: application/json
{
  "coop_id": "partner-coop",
  "name": "Partner Cooperative",
  "public_did": "did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK",
  "gateway_endpoints": ["http://partner.example.com:8080"],
  "capabilities": ["compute", "storage"]
}
HTTP 201
[Asserts]
jsonpath "$.status" == "registered"

# Get specific cooperative
GET {{host}}/v1/federation/coops/partner-coop
Authorization: Bearer {{token}}
HTTP 200
[Asserts]
jsonpath "$.coop_id" == "partner-coop"

# ===========================================================================
# Vouching
# ===========================================================================

# Get vouches for a cooperative
GET {{host}}/v1/federation/coops/partner-coop/vouches
Authorization: Bearer {{token}}
HTTP 200
[Asserts]
jsonpath "$.coop_id" == "partner-coop"

# Vouch for a cooperative (with trust score)
POST {{host}}/v1/federation/coops/partner-coop/vouch
Authorization: Bearer {{token}}
Content-Type: application/json
{
  "target_coop_id": "partner-coop",
  "trust_score": 0.75,
  "expires_in_days": 365
}
HTTP 201
[Asserts]
jsonpath "$.status" == "vouched"
jsonpath "$.trust_score" == 0.75

# ===========================================================================
# Attestations
# ===========================================================================

# Issue an attestation
POST {{host}}/v1/federation/attestations
Authorization: Bearer {{token}}
Content-Type: application/json
{
  "member_did": "did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK",
  "trust_score": 0.85,
  "context": "economic",
  "validity_days": 90
}
HTTP 201
[Asserts]
jsonpath "$.status" == "issued"

# Get attestations for a member
GET {{host}}/v1/federation/attestations/did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK
Authorization: Bearer {{token}}
HTTP 200

# ===========================================================================
# Clearing Agreements
# ===========================================================================

# List clearing agreements
GET {{host}}/v1/federation/clearing
Authorization: Bearer {{token}}
HTTP 200

# Create a clearing agreement
POST {{host}}/v1/federation/clearing
Authorization: Bearer {{token}}
Content-Type: application/json
{
  "agreement_id": "test-coop-partner-coop-001",
  "partner_coop_id": "partner-coop",
  "partner_did": "did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK",
  "max_imbalance": 10000,
  "settlement": "weekly"
}
HTTP 201
[Asserts]
jsonpath "$.status" == "created"

# Get specific agreement
GET {{host}}/v1/federation/clearing/test-coop-partner-coop-001
Authorization: Bearer {{token}}
HTTP 200
[Asserts]
jsonpath "$.id" exists

# Get clearing position
GET {{host}}/v1/federation/clearing/test-coop-partner-coop-001/position
Authorization: Bearer {{token}}
HTTP 200

# Trigger settlement
POST {{host}}/v1/federation/clearing/test-coop-partner-coop-001/settle
Authorization: Bearer {{token}}
HTTP 200

# ===========================================================================
# Error Cases
# ===========================================================================

# Missing auth token
GET {{host}}/v1/federation/status
HTTP 401

# Invalid trust score (out of range)
POST {{host}}/v1/federation/coops/partner-coop/vouch
Authorization: Bearer {{token}}
Content-Type: application/json
{
  "target_coop_id": "partner-coop",
  "trust_score": 1.5,
  "expires_in_days": 365
}
HTTP 400

# Invalid attestation context
POST {{host}}/v1/federation/attestations
Authorization: Bearer {{token}}
Content-Type: application/json
{
  "member_did": "did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK",
  "trust_score": 0.5,
  "context": "invalid_context",
  "validity_days": 90
}
HTTP 400

# Non-existent cooperative
GET {{host}}/v1/federation/coops/nonexistent-coop
Authorization: Bearer {{token}}
HTTP 404
