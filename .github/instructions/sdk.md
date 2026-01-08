---
applyTo: "sdk/**/*.{ts,tsx,js,jsx}"
---

# SDK Instructions

These instructions apply to the client SDKs in the `sdk/` directory.

## SDK Structure

- `sdk/typescript/` - TypeScript SDK for Node.js and browsers
- `sdk/react-native/` - React Native SDK for mobile apps

## Technology Stack

### TypeScript SDK
- TypeScript 5.x
- Node.js 18+ runtime
- ES modules
- Comprehensive type definitions

### React Native SDK
- React Native with TypeScript
- Expo for easier development
- Native module wrappers for ICN functionality

## Code Style

### TypeScript

- Use strict TypeScript (`strict: true`)
- Prefer interfaces over types for object shapes
- Use descriptive, explicit type annotations
- Avoid `any` - use `unknown` if type is truly unknown
- Use generics for reusable components
- Export types alongside implementations

Example:
```typescript
export interface CreateTransactionParams {
    recipient: string;
    amount: number;
    description: string;
}

export interface Transaction {
    id: string;
    sender: string;
    recipient: string;
    amount: number;
    timestamp: number;
}

export async function createTransaction(
    params: CreateTransactionParams
): Promise<Transaction> {
    // Implementation
}
```

### React Native

- Use functional components with hooks
- Prefer TypeScript over PropTypes
- Use `memo` for expensive components
- Follow React Native best practices
- Handle platform-specific code appropriately

## SDK Design Principles

### 1. Developer Experience

- Simple, intuitive API
- Comprehensive TypeScript types
- Clear error messages
- Helpful JSDoc comments
- Minimal dependencies

### 2. Consistency

- Follow naming conventions from main codebase
- Mirror ICN daemon API structure
- Consistent error handling patterns
- Standard async/await patterns

### 3. Error Handling

```typescript
export class ICNError extends Error {
    constructor(
        message: string,
        public code: string,
        public statusCode?: number
    ) {
        super(message);
        this.name = 'ICNError';
    }
}

// Usage
throw new ICNError('Transaction failed', 'TRANSACTION_FAILED', 400);
```

### 4. Type Safety

- Export all public types
- Use branded types for IDs
- Validate input parameters
- Provide type guards

Example:
```typescript
export type DID = string & { readonly __brand: 'DID' };

export function isDID(value: string): value is DID {
    return value.startsWith('did:icn:');
}
```

## Testing

### Unit Tests

- Test all exported functions
- Mock external dependencies
- Test error conditions
- Use Jest or Vitest

### Integration Tests

- Test against real ICN daemon (test mode)
- Test complete workflows
- Test error recovery
- Test edge cases

### Type Tests

- Ensure types are exported correctly
- Test type inference
- Verify no `any` leakage

## Documentation

### Required for Public APIs

- JSDoc comments with examples
- Parameter descriptions
- Return value descriptions
- Error conditions
- Usage examples

Example:
```typescript
/**
 * Creates a new transaction in the mutual credit ledger.
 *
 * @param params - Transaction parameters
 * @param params.recipient - DID of the recipient
 * @param params.amount - Amount in time credits
 * @param params.description - Human-readable description
 * @returns The created transaction with ID and timestamp
 * @throws {ICNError} If transaction fails validation or cannot be processed
 *
 * @example
 * ```typescript
 * const tx = await client.createTransaction({
 *   recipient: 'did:icn:abc123',
 *   amount: 2.5,
 *   description: 'Website design'
 * });
 * console.log(`Transaction created: ${tx.id}`);
 * ```
 */
export async function createTransaction(
    params: CreateTransactionParams
): Promise<Transaction> {
    // Implementation
}
```

## Build Configuration

### TypeScript

- Compile to ES modules and CommonJS (dual package)
- Generate declaration files (.d.ts)
- Enable source maps
- Target ES2020 or later

### React Native

- Use Expo for development builds
- Support both iOS and Android
- Handle platform-specific features gracefully
- Bundle size optimization

## Common Tasks

### Adding a New API Method

1. Define TypeScript types/interfaces
2. Implement method with error handling
3. Add JSDoc documentation with examples
4. Export from main entry point
5. Add unit tests
6. Add integration tests
7. Update README with usage example

### Versioning

- Follow semantic versioning (semver)
- Document breaking changes clearly
- Maintain CHANGELOG.md
- Tag releases appropriately

### Publishing

- Build before publishing
- Test built package locally
- Update version in package.json
- Update CHANGELOG.md
- Publish to npm registry

## Important Notes

- SDKs should be simple and focused
- Don't replicate daemon logic in SDK
- SDK is a thin wrapper around Gateway API
- Prioritize developer experience
- Keep dependencies minimal
- Support both ESM and CommonJS where applicable
