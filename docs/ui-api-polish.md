# ICN Wallet UI API Polish Guide

This guide outlines recommended improvements and polish for the Wallet UI API to ensure it provides a rich frontend experience with comprehensive data and reliable notifications.

## Current API Endpoints

The current API structure includes:

- Identity management endpoints (`/api/did/*`)
- Proposal handling endpoints (`/api/proposal/*`, `/api/actions/*`)
- Credential management endpoints (`/api/vc/*`)
- AgoraNet integration endpoints (`/api/agoranet/*`)
- Synchronization endpoints (`/api/sync/*`)

## API Enhancements

### 1. Response Metadata

Add consistent metadata to all API responses:

```json
{
  "meta": {
    "timestamp": "2023-05-01T12:00:00Z",
    "version": "1.0.0",
    "request_id": "req-123456"
  },
  "data": {
    // Original response data
  }
}
```

Implementation:
- Create a wrapper middleware in `wallet-ui-api/src/middleware.rs`
- Apply to all API routes

### 2. Pagination Support

Add pagination for collection endpoints:

```
GET /api/vc/list?page=2&limit=20
```

Response format:

```json
{
  "data": [...],
  "pagination": {
    "total_items": 45,
    "total_pages": 3,
    "current_page": 2,
    "limit": 20,
    "next_page": 3,
    "prev_page": 1
  }
}
```

Implementation:
- Add pagination parameters to handler functions
- Update database queries to support LIMIT and OFFSET
- Include pagination metadata in responses

### 3. Enhanced Error Responses

Improve error responses with detailed information:

```json
{
  "error": {
    "code": "CREDENTIAL_VERIFICATION_FAILED",
    "message": "Unable to verify credential signature",
    "details": "The issuer's public key could not be retrieved",
    "request_id": "req-123456",
    "timestamp": "2023-05-01T12:00:00Z"
  }
}
```

Implementation:
- Enhance `ApiError` enum in `wallet-ui-api/src/error.rs`
- Add error codes and detailed messages
- Implement detailed error conversion from internal errors

### 4. Identity Management Improvements

#### Add Identity Status Information

```json
{
  "id": "identity123",
  "did": "did:icn:abc123",
  "scope": "personal",
  "status": {
    "is_active": true,
    "is_guardian": false,
    "last_used": "2023-05-01T12:00:00Z",
    "credential_count": 3,
    "proposal_count": 2
  },
  "document": {...}
}
```

Implementation:
- Update `IdentityResponse` struct in `handlers.rs`
- Add status calculation to identity handlers

#### Support Identity Metadata Updates

Add endpoint:
```
PATCH /api/did/:id/metadata
```

Implementation:
- Add new handler for updating identity metadata
- Validate and merge updates with existing metadata

### 5. Credential Management Enhancements

#### Add Credential Filtering and Search

```
GET /api/vc/list?type=MembershipCredential&issuer=did:icn:issuer123&status=valid
```

Implementation:
- Update credential list handler to support query filters
- Add search capability for credential content

#### Add Credential Timeline View

New endpoint:
```
GET /api/vc/:id/timeline
```

Response:
```json
{
  "id": "cred123",
  "events": [
    {
      "type": "issuance",
      "timestamp": "2023-05-01T10:00:00Z",
      "actor": "did:icn:issuer123"
    },
    {
      "type": "verification",
      "timestamp": "2023-05-01T11:00:00Z",
      "actor": "did:icn:verifier456"
    },
    {
      "type": "linked",
      "timestamp": "2023-05-01T12:00:00Z",
      "references": {
        "thread_id": "thread789"
      }
    }
  ]
}
```

Implementation:
- Add credential events tracking in a new module
- Create new timeline endpoint and response format

### 6. Governance and Proposal Improvements

#### Comprehensive Proposal Status

New endpoint:
```
GET /api/proposals/:id/status
```

Response:
```json
{
  "id": "proposal123",
  "type": "ConfigChange",
  "title": "Increase Voting Period",
  "status": "voting",
  "timeline": [
    {
      "status": "created",
      "timestamp": "2023-05-01T10:00:00Z",
      "actor": "did:icn:creator123"
    },
    {
      "status": "discussion",
      "timestamp": "2023-05-01T10:30:00Z",
      "thread_id": "thread456"
    },
    {
      "status": "voting",
      "timestamp": "2023-05-01T12:00:00Z"
    }
  ],
  "voting": {
    "start_time": "2023-05-01T12:00:00Z",
    "end_time": "2023-05-14T12:00:00Z",
    "threshold": 3,
    "votes": {
      "approve": 2,
      "reject": 1,
      "abstain": 0
    }
  },
  "agoranet": {
    "thread_id": "thread456",
    "post_count": 5,
    "last_activity": "2023-05-01T14:00:00Z"
  }
}
```

Implementation:
- Create new proposal status tracking system
- Update proposal handlers to collect status information
- Add new status endpoint with comprehensive view

#### Guardian Actions API

New endpoint:
```
GET /api/guardian/actions
```

Response:
```json
{
  "pending_proposals": 2,
  "pending_votes": 3,
  "executed_proposals": 5,
  "recent_actions": [
    {
      "type": "vote",
      "proposal_id": "proposal123",
      "timestamp": "2023-05-01T12:00:00Z"
    },
    {
      "type": "execute",
      "proposal_id": "proposal456",
      "timestamp": "2023-05-01T10:00:00Z"
    }
  ]
}
```

Implementation:
- Create guardian actions dashboard endpoint
- Collect metrics from proposal queue
- Track guardian action history

### 7. AgoraNet Integration Improvements

#### Thread Subscription System

New endpoints:
```
POST /api/agoranet/threads/:id/subscribe
DELETE /api/agoranet/threads/:id/unsubscribe
GET /api/agoranet/subscriptions
```

Implementation:
- Add subscription tracking to AgoraNet integration
- Set up notification triggers for subscribed threads
- Create subscription management endpoints

#### Enhanced Thread Details

Improve thread detail response:
```json
{
  "id": "thread123",
  "title": "Discussion on Treasury Proposal",
  "proposal_id": "proposal123",
  "topic": "governance",
  "author": "did:icn:alice",
  "created_at": "2023-05-01T12:00:00Z",
  "post_count": 3,
  "summary": "Discussion about allocating treasury funds for community projects",
  "tags": ["treasury", "funding", "community"],
  "last_activity": "2023-05-01T15:00:00Z",
  "is_subscribed": true,
  "posts": [...],
  "credential_links": [...]
}
```

Implementation:
- Enhance thread detail response with additional metadata
- Add subscription status to response
- Include summary and tag extraction

## WebSocket Notifications

### 1. WebSocket Connection Enhancements

Improve the existing WebSocket system:
- Add client identification and authentication
- Support targeted notifications for specific clients
- Add heartbeat mechanism to detect disconnections

Implementation:
- Update WebSocket handler to support authentication
- Add client tracking in WebSocket manager
- Implement heartbeat protocol

### 2. Notification Types

Implement comprehensive notification types:

#### Identity Notifications
```json
{
  "type": "identity_update",
  "timestamp": "2023-05-01T12:00:00Z",
  "data": {
    "identity_id": "identity123",
    "action": "activated"
  }
}
```

#### Credential Notifications
```json
{
  "type": "credential_update",
  "timestamp": "2023-05-01T12:00:00Z",
  "data": {
    "credential_id": "cred123",
    "action": "verified"
  }
}
```

#### Proposal Notifications
```json
{
  "type": "proposal_update",
  "timestamp": "2023-05-01T12:00:00Z",
  "data": {
    "proposal_id": "proposal123",
    "status": "voting",
    "previous_status": "discussion"
  }
}
```

#### AgoraNet Notifications
```json
{
  "type": "thread_update",
  "timestamp": "2023-05-01T12:00:00Z",
  "data": {
    "thread_id": "thread123",
    "action": "new_post",
    "post_id": "post456",
    "post_author": "did:icn:bob"
  }
}
```

#### Sync Notifications
```json
{
  "type": "sync_complete",
  "timestamp": "2023-05-01T12:00:00Z",
  "data": {
    "entity_type": "trust_bundles",
    "count": 5,
    "success": true
  }
}
```

Implementation:
- Create notification type enum and serialization
- Add notification trigger points throughout the API
- Implement notification filtering and routing

### 3. Notification Preferences

Allow users to configure notification preferences:

```
POST /api/notifications/preferences
```

Request:
```json
{
  "thread_updates": true,
  "credential_events": true,
  "proposal_status": true,
  "guardian_actions": false
}
```

Implementation:
- Add notification preferences storage
- Create preference management endpoints
- Apply preferences to notification routing

## Implementation Plan

### Phase 1: Response Enhancement
1. Implement consistent metadata
2. Enhance error responses
3. Add pagination support

### Phase 2: Feature Enhancements
1. Upgrade identity management
2. Improve credential management
3. Enhance proposal and governance APIs

### Phase 3: Notification System
1. Enhance WebSocket connections
2. Implement notification types
3. Add notification preferences

### Phase 4: Testing and Validation
1. Test API with frontend integration
2. Validate notification delivery
3. Stress test with concurrent connections 