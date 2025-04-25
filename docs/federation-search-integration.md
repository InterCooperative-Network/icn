# Federation-Aware Search Integration Guide

This document provides instructions for integrating the new federation-aware search UI into the ICN Wallet.

## Overview

The federation-aware search UI consists of three main components:

1. `CredentialSearchBar` - A component for searching and filtering credentials by federation, type, and role
2. `ThreadSearchView` - A component for searching and viewing AgoraNet discussion threads
3. `FederationSearchPage` - A unified search page that integrates both components with tabbed navigation

These components are supported by a new `SearchService` that provides federation-aware search capabilities for both credentials and threads.

## Integration Steps

### 1. Add the Search Service

First, ensure that `search-service.ts` is available in the `src/services` directory. This service provides the core functionality for federation-aware search.

### 2. Update an Existing Page

You can either:

1. Add the `FederationSearchPage` component to the main navigation menu, or
2. Replace the existing credential list view with the enhanced search functionality

#### Option 1: Add as a New Page

For web applications using React Router or similar navigation:

```tsx
import { FederationSearchPage } from './components/FederationSearchPage';
import { SearchService } from './services/search-service';

// Inside your routes definition
<Route 
  path="/search" 
  element={
    <FederationSearchPage 
      credentials={credentials}
      agoraNetEndpoint="https://agoranet.icn.zone"
      userDid={currentUser.did}
    />
  } 
/>

// Add a navigation link
<NavLink to="/search">Search Credentials & Threads</NavLink>
```

#### Option 2: Replace Existing Credential View

```tsx
// Instead of rendering the basic CredentialListView
<CredentialListView credentialService={credentialService} userDid={userDid} />

// Render the enhanced FederationSearchPage
<FederationSearchPage 
  credentials={credentials}
  agoraNetEndpoint="https://agoranet.icn.zone"
  userDid={userDid}
/>
```

### 3. Initialize the Search Service

In your app initialization code:

```tsx
import { CredentialService } from './services/credential-service';
import { SearchService } from './services/search-service';

// Initialize services
const credentialService = new CredentialService();
const searchService = new SearchService(
  credentialService, 
  "https://agoranet.icn.zone"
);

// Make services available to components
// (via context, props, or state management system)
```

## API Configuration

The search functionality requires access to the AgoraNet API for thread search. Ensure that your configuration includes:

```typescript
// Example configuration
const config = {
  agoraNetEndpoint: "https://agoranet.icn.zone",
  // Other configuration options...
};
```

## Usage Examples

### Search for Credentials within a Specific Federation

```typescript
const searchService = new SearchService(credentialService, agoraNetEndpoint);

// Search for "proposal" credentials in a specific federation
const results = await searchService.searchCredentials({
  query: "proposal",
  federationId: "federation-123",
  type: "proposal"
});
```

### Search for Threads Related to a Proposal

```typescript
// Search for threads related to a specific proposal
const threads = await searchService.searchThreads({
  proposalId: "proposal-456",
  federationId: "federation-123"
});
```

## Component Props

### FederationSearchPage

```typescript
interface FederationSearchPageProps {
  credentials: WalletCredential[];
  agoraNetEndpoint: string;
  userDid: string;
}
```

### CredentialSearchBar

```typescript
interface CredentialSearchBarProps {
  credentials: WalletCredential[];
  federations: { id: string; name: string }[];
  onSearchResults: (results: WalletCredential[], searchQuery: string, federationFilter: string) => void;
}
```

### ThreadSearchView

```typescript
interface ThreadSearchViewProps {
  agoraNetEndpoint: string;
  federations: { id: string; name: string }[];
  onThreadSelect?: (threadId: string) => void;
  userCredentialIds?: string[]; // Optional array of credential IDs to highlight related threads
}
```

## Styling

The components use CSS-in-JS with the `styled-jsx` library. You can customize the styling by:

1. Modifying the existing style blocks in each component
2. Adding global CSS classes to override component styles
3. Wrapping components in custom styled wrappers

## Extending the Search Functionality

To add additional search capabilities:

1. Update the `SearchOptions` interface in `search-service.ts`
2. Add new filter parameters to the search methods
3. Update the UI components to include the new filter options 