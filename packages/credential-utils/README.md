# @icn/credential-utils

A shared library for credential-related utilities in the Intercoin Network ecosystem.

## Overview

This package provides common functionality for working with credentials, DIDs, and federation trust scores across ICN applications. It is designed to be used in both browser and Node.js environments.

## Installation

```bash
# If using npm
npm install --save @icn/credential-utils

# If using yarn
yarn add @icn/credential-utils

# If using pnpm
pnpm add @icn/credential-utils
```

## Features

- DID formatting and manipulation (`formatDid`, `createDid`)
- Trust score calculation (`getTrustScore`, `computeFederationTrustScore`)
- Credential export utilities (`exportCredentialAsVC`)
- UI helper functions (`getTrustLevelColor`, `getTrustLevelMuiColor`)
- Common type definitions for credentials, federations, and wallets

## Usage Examples

### DID Formatting

```typescript
import { formatDid } from '@icn/credential-utils';

const did = 'did:icn:federation123:user456';
const formattedDid = formatDid(did); // 'did:icn:...user456'
```

### Trust Score Calculation

```typescript
import { computeFederationTrustScore } from '@icn/credential-utils';

const trustScore = computeFederationTrustScore(credential, federationManifest);
console.log(`Trust Score: ${trustScore.score}/100 (${trustScore.status})`);
```

### Credential Export

```typescript
import { exportCredentialAsVC } from '@icn/credential-utils';

// In a UI component
<Button onClick={() => exportCredentialAsVC(credential)}>
  Export Credential
</Button>
```

## License

Apache-2.0 