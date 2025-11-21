# ICN Timebank Pilot UI - Testing Guide

Comprehensive testing suite for the ICN Timebank Progressive Web App.

## Overview

This testing suite includes:
- **Unit Tests** (Jest) - Testing utility functions and business logic
- **E2E Tests** (Playwright) - Testing complete user flows across browsers
- **Coverage Reports** - Code coverage analysis

## Quick Start

### Install Dependencies

```bash
npm install
```

### Run All Tests

```bash
# Run unit tests
npm test

# Run unit tests in watch mode
npm run test:watch

# Run unit tests with coverage
npm run test:coverage

# Run E2E tests (headless)
npm run test:e2e

# Run E2E tests (headed - see browser)
npm run test:e2e:headed

# Run E2E tests with debugger
npm run test:e2e:debug

# Run all tests (unit + E2E)
npm run test:all
```

## Unit Tests (Jest)

### What We Test

**Utility Functions:**
- `truncateDid()` - DID truncation for display
- `formatDate()` / `formatDateTime()` - Timestamp formatting
- `calculateGini()` - Economic inequality measurement
- `getUserFriendlyError()` - Error message transformation

**Economic Metrics:**
- Transaction velocity calculation
- Participation rate calculation
- Hoarding index calculation
- Gini coefficient calculation

**Data Operations:**
- Balance calculations (credits - debits)
- Transaction filtering (by date, type)
- Transaction sorting (by date, amount)
- Service listing search and filter

**Coverage Thresholds:**
- Branches: 70%
- Functions: 70%
- Lines: 70%
- Statements: 70%

### Running Unit Tests

```bash
# Run once
npm test

# Watch mode (re-runs on file changes)
npm run test:watch

# With coverage report
npm run test:coverage
```

Coverage reports are generated in `coverage/` directory. Open `coverage/lcov-report/index.html` in a browser to view detailed coverage.

### Writing Unit Tests

Place unit tests in `tests/*.test.js` files:

```javascript
describe('Feature Name', () => {
  test('should do something specific', () => {
    const result = myFunction(input);
    expect(result).toBe(expectedOutput);
  });
});
```

## E2E Tests (Playwright)

### What We Test

**Login Flow:**
- Form validation
- Authentication help modal
- Dark mode toggle
- Accessibility (ARIA labels, keyboard navigation)
- LocalStorage persistence

**Dashboard:**
- Balance and stats display
- Chart rendering (Canvas API)
- Recent activity
- Top contributors
- Proposal summary

**Navigation:**
- Tab switching
- Active tab highlighting
- Keyboard shortcuts (Ctrl+1-5)
- Arrow key navigation

**History Tab:**
- Transaction list display
- Date filtering
- Sorting (by date, amount)
- CSV export

**Members Tab:**
- Member list display
- Search functionality
- DID copying to clipboard

**Member Profile (Phase 8):**
- Profile header and stats
- Bio, skills, availability display
- Contact information
- Service history tabs
- Edit profile modal
- Profile data persistence

**Service Board (Phase 8):**
- Service listings display
- Type and category filtering
- Search functionality
- Post service modal
- Form validation
- Service persistence

**Economic Dashboard (Phase 8):**
- Advanced reports toggle
- Metrics display (velocity, participation, Gini, hoarding)
- Velocity chart rendering
- Participation heatmap rendering

### Browser Coverage

Tests run on:
- **Desktop:** Chrome, Firefox, Safari (WebKit)
- **Mobile:** Chrome (Pixel 5), Safari (iPhone 12)

### Running E2E Tests

```bash
# Run all E2E tests (headless)
npm run test:e2e

# Run with visible browser (headed mode)
npm run test:e2e:headed

# Run with Playwright Inspector (debug)
npm run test:e2e:debug

# Run specific test file
npx playwright test tests/e2e/login.spec.js

# Run specific test by name
npx playwright test -g "should display login screen"

# Run on specific browser
npx playwright test --project=chromium
```

### Viewing Test Reports

```bash
# Generate HTML report
npx playwright show-report

# View last run results
npx playwright show-report test-results
```

### Writing E2E Tests

Place E2E tests in `tests/e2e/*.spec.js` files:

```javascript
import { test, expect } from '@playwright/test';

test.describe('Feature Name', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('should do something', async ({ page }) => {
    await page.click('#my-button');
    await expect(page.locator('#result')).toBeVisible();
  });
});
```

## Test Structure

```
tests/
├── README.md                      # This file
├── setup.js                       # Jest configuration
├── utils.test.js                  # Unit tests for utilities
└── e2e/
    ├── login.spec.js              # Login flow E2E tests
    ├── dashboard.spec.js          # Dashboard & navigation E2E tests
    └── phase8-features.spec.js    # Phase 8 features E2E tests
```

## Continuous Integration

### GitHub Actions Example

```yaml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions/setup-node@v3
        with:
          node-version: '18'
      - run: npm ci
      - run: npm test
      - run: npx playwright install --with-deps
      - run: npm run test:e2e
      - uses: actions/upload-artifact@v3
        if: always()
        with:
          name: playwright-report
          path: playwright-report/
```

## Debugging Tests

### Jest

```bash
# Run with Node debugger
node --inspect-brk node_modules/.bin/jest --runInBand

# Run specific test file
npm test -- utils.test.js

# Run tests matching pattern
npm test -- -t "calculateGini"
```

### Playwright

```bash
# Debug mode (opens Playwright Inspector)
npm run test:e2e:debug

# Debug specific test
npx playwright test --debug -g "should login"

# Slow motion (useful for demos)
npx playwright test --headed --slow-mo=1000
```

## Best Practices

### Unit Tests
1. **Test pure functions** - Easier to test, no side effects
2. **Mock external dependencies** - Don't rely on API/localStorage
3. **Use descriptive test names** - "should calculate balance correctly"
4. **Test edge cases** - Empty arrays, null values, extreme inputs
5. **Keep tests focused** - One assertion per test when possible

### E2E Tests
1. **Test user flows, not implementation** - Click buttons, fill forms, verify UI
2. **Use data-testid attributes** - Stable selectors that won't change with styling
3. **Mock API responses** - Don't depend on live backend
4. **Clean up after tests** - Reset localStorage, clear cookies
5. **Make tests independent** - Each test should work in isolation

### General
1. **Write tests first (TDD)** - Design your API through tests
2. **Keep tests simple** - Easy to understand and maintain
3. **Run tests frequently** - Catch bugs early
4. **Maintain test coverage** - Aim for 70%+ coverage
5. **Review test failures** - Fix or update tests, don't ignore

## Common Issues

### Jest

**Issue:** "Cannot find module"
```bash
# Solution: Ensure module path is correct
# Use absolute path from project root
```

**Issue:** "TypeError: Cannot read property..."
```bash
# Solution: Mock the dependency properly
global.navigator = { clipboard: { writeText: jest.fn() } };
```

### Playwright

**Issue:** "Target closed" or "Browser context closed"
```bash
# Solution: Ensure page.goto() succeeds before interactions
await page.goto('/', { waitUntil: 'networkidle' });
```

**Issue:** "Element is not attached to the DOM"
```bash
# Solution: Wait for element before interacting
await page.waitForSelector('#my-element');
```

**Issue:** "Timeout 30000ms exceeded"
```bash
# Solution: Increase timeout or improve selectors
await page.waitForSelector('#slow-element', { timeout: 60000 });
```

## Resources

- [Jest Documentation](https://jestjs.io/docs/getting-started)
- [Playwright Documentation](https://playwright.dev/docs/intro)
- [Testing Library](https://testing-library.com/docs/dom-testing-library/intro/)
- [Web Vitals Testing](https://web.dev/vitals/)

## Contributing

When adding new features:
1. Write unit tests for business logic
2. Write E2E tests for user-facing features
3. Ensure all tests pass before submitting PR
4. Maintain or improve code coverage
5. Update this README if adding new test types

## License

MIT
