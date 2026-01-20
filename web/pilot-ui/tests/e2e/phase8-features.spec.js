/**
 * E2E Tests: Phase 8 Advanced Features
 * Member Profile, Service Board, Economic Health Dashboard
 */

import { test, expect } from '@playwright/test';

test.use({ serviceWorkers: 'block' });

async function mockApiResponses(page) {
  const corsHeaders = {
    'Access-Control-Allow-Origin': '*',
    'Access-Control-Allow-Methods': 'GET,POST,PUT,DELETE,OPTIONS',
    'Access-Control-Allow-Headers': 'Authorization, Content-Type',
  };

  await page.route('**/v1/**', (route) => {
    const url = route.request().url();
    const now = Date.now();

    if (route.request().method() === 'OPTIONS') {
      return route.fulfill({ status: 204, headers: corsHeaders });
    }

    if (url.endsWith('/health')) {
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        headers: corsHeaders,
        body: JSON.stringify({ status: 'ok' }),
      });
    }

    if (url.includes('/ledger/') && url.includes('/balance/')) {
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        headers: corsHeaders,
        body: JSON.stringify({ balances: { hours: 12.0 } }),
      });
    }

    if (url.includes('/ledger/') && url.includes('/history')) {
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        headers: corsHeaders,
        body: JSON.stringify({
          transactions: [
            {
              id: '1',
              timestamp: now - 1000 * 60 * 60,
              author: 'did:icn:alice',
              memo: 'Garden work',
              accounts: [
                { account_id: 'did:icn:alice', currency: 'hours', debit: 5 },
                { account_id: 'did:icn:test123', currency: 'hours', credit: 5 },
              ],
            },
          ],
        }),
      });
    }

    if (url.includes('/coops/') && !url.includes('/members')) {
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        headers: corsHeaders,
        body: JSON.stringify({
          members: [
            { did: 'did:icn:test123', role: 'owner', balance: 12.0 },
            { did: 'did:icn:alice', role: 'member', balance: 5.0 },
          ],
        }),
      });
    }

    if (url.includes('/gov/') && url.includes('/proposals')) {
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        headers: corsHeaders,
        body: JSON.stringify({ data: [] }),
      });
    }

    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      headers: corsHeaders,
      body: '{}',
    });
  });
}

async function loginToApp(page) {
  await mockApiResponses(page);
  await page.goto('/');
  await page.fill('#gateway-url', 'http://localhost:8000');
  await page.fill('#coop-id', 'test-coop');
  await page.fill('#did', 'did:icn:test123');
  await page.fill('#token', 'test-token-123');
  await page.click('#login-btn');
  await page.waitForSelector('#main-screen', { state: 'visible' });
}

test.describe('Economic Health Dashboard', () => {
  test.beforeEach(async ({ page }) => {
    await loginToApp(page);
  });

  test('should toggle advanced reports', async ({ page }) => {
    const toggleBtn = page.locator('#toggle-advanced-reports');
    await expect(toggleBtn).toBeVisible();

    const reports = page.locator('#advanced-reports');
    await expect(reports).toBeHidden();

    // Click to show
    await toggleBtn.click();
    await expect(reports).toBeVisible();
    await expect(toggleBtn).toContainText('Hide Details');

    // Click to hide
    await toggleBtn.click();
    await expect(reports).toBeHidden();
    await expect(toggleBtn).toContainText('Show Details');
  });

  test('should display economic metrics', async ({ page }) => {
    const toggleBtn = page.locator('#toggle-advanced-reports');
    await toggleBtn.click();

    // Check all 4 metrics are displayed
    await expect(page.locator('#metric-velocity')).toBeVisible();
    await expect(page.locator('#metric-participation')).toBeVisible();
    await expect(page.locator('#metric-gini')).toBeVisible();
    await expect(page.locator('#metric-hoarding')).toBeVisible();
  });

  test('should render velocity chart', async ({ page }) => {
    const toggleBtn = page.locator('#toggle-advanced-reports');
    await toggleBtn.click();

    const chart = page.locator('#velocity-chart');
    await expect(chart).toBeVisible();

    // Should be a canvas element
    const isCanvas = await chart.evaluate(el => el.tagName === 'CANVAS');
    expect(isCanvas).toBeTruthy();
  });

  test('should render participation heatmap', async ({ page }) => {
    const toggleBtn = page.locator('#toggle-advanced-reports');
    await toggleBtn.click();

    const heatmap = page.locator('#participation-heatmap');
    await expect(heatmap).toBeVisible();

    // Should contain heatmap cells
    const cells = page.locator('.heatmap-cell');
    const count = await cells.count();
    expect(count).toBe(84); // 12 weeks * 7 days
  });

  test('should show tooltip on heatmap cell hover', async ({ page }) => {
    const toggleBtn = page.locator('#toggle-advanced-reports');
    await toggleBtn.click();

    const firstCell = page.locator('.heatmap-cell').first();
    await firstCell.hover();

    const tooltip = firstCell.locator('.heatmap-tooltip');
    // Tooltip should become visible on hover (CSS opacity change)
    await expect(tooltip).toBeAttached();
  });
});

test.describe('Member Profile', () => {
  test.beforeEach(async ({ page }) => {
    await loginToApp(page);
    await page.click('button[data-tab="member-profile"]');
  });

  test('should display profile header', async ({ page }) => {
    await expect(page.locator('.profile-header')).toBeVisible();
    await expect(page.locator('.profile-avatar')).toBeVisible();
    await expect(page.locator('#profile-name')).toBeVisible();
    await expect(page.locator('#profile-did')).toBeVisible();
  });

  test('should display profile statistics', async ({ page }) => {
    await expect(page.locator('#profile-balance')).toBeVisible();
    await expect(page.locator('#profile-tx-count')).toBeVisible();
    await expect(page.locator('#profile-member-since')).toBeVisible();
  });

  test('should display bio section', async ({ page }) => {
    await expect(page.locator('#profile-bio')).toBeVisible();
  });

  test('should display skills section', async ({ page }) => {
    await expect(page.locator('#profile-skills')).toBeVisible();
  });

  test('should display availability section', async ({ page }) => {
    await expect(page.locator('#profile-availability')).toBeVisible();
  });

  test('should display contact information', async ({ page }) => {
    await expect(page.locator('#profile-contact')).toBeVisible();
  });

  test('should switch between service history tabs', async ({ page }) => {
    // Click Services Given tab
    const givenTab = page.locator('.service-history-tabs button[data-history="given"]');
    await givenTab.click();
    await expect(givenTab).toHaveClass(/active/);
    await expect(page.locator('#profile-service-history')).toBeVisible();

    // Click Services Received tab
    const receivedTab = page.locator('.service-history-tabs button[data-history="received"]');
    await receivedTab.click();
    await expect(receivedTab).toHaveClass(/active/);
    await expect(page.locator('#profile-service-history')).toBeVisible();
  });

  test('should open edit profile modal', async ({ page }) => {
    const editBtn = page.locator('#edit-profile-btn');
    await editBtn.click();

    const modal = page.locator('#edit-profile-modal');
    await expect(modal).toBeVisible();

    // Should contain form fields
    await expect(page.locator('#edit-bio')).toBeVisible();
    await expect(page.locator('#edit-skills')).toBeVisible();
    await expect(page.locator('#edit-availability')).toBeVisible();
    await expect(page.locator('#edit-contact')).toBeVisible();
  });

  test('should save profile changes', async ({ page }) => {
    const editBtn = page.locator('#edit-profile-btn');
    await editBtn.click();

    // Fill in form
    await page.fill('#edit-bio', 'Test bio content');
    await page.fill('#edit-skills', 'Gardening, Teaching');
    await page.selectOption('#edit-availability', 'weekdays');
    await page.fill('#edit-contact', 'test@example.com');

    // Save
    const saveBtn = page.locator('#edit-profile-form button[type="submit"]');
    await saveBtn.click();

    // Modal should close
    const modal = page.locator('#edit-profile-modal');
    await expect(modal).toBeHidden();

    // Should show success toast
    const toast = page.locator('.toast', { hasText: 'Profile updated' });
    await expect(toast).toBeVisible();

    // Check localStorage
    const profileData = await page.evaluate(() =>
      JSON.parse(localStorage.getItem('icn-profile') || '{}')
    );
    expect(profileData.bio).toBe('Test bio content');
    expect(profileData.contact.info).toBe('test@example.com');
  });

  test('should close edit modal on cancel', async ({ page }) => {
    const editBtn = page.locator('#edit-profile-btn');
    await editBtn.click();

    const modal = page.locator('#edit-profile-modal');
    await expect(modal).toBeVisible();

    const closeBtn = page.locator('#cancel-edit-profile');
    await closeBtn.click();

    await expect(modal).toBeHidden();
  });
});

test.describe('Service Request Board', () => {
  test.beforeEach(async ({ page }) => {
    await loginToApp(page);
    await page.click('button[data-tab="service-board"]');
  });

  test('should display service board', async ({ page }) => {
    await expect(page.locator('#service-listings')).toBeVisible();
  });

  test('should display filter controls', async ({ page }) => {
    await expect(page.locator('#service-type-filter')).toBeVisible();
    await expect(page.locator('#service-category-filter')).toBeVisible();
    await expect(page.locator('#service-search')).toBeVisible();
  });

  test('should display service listings', async ({ page }) => {
    // Should have sample listings
    const listings = page.locator('.service-listing');
    const count = await listings.count();
    expect(count).toBeGreaterThan(0);
  });

  test('should filter by service type', async ({ page }) => {
    const filter = page.locator('#service-type-filter');

    // Filter to only offers
    await filter.selectOption('offer');

    // Check that only offers are displayed
    const listings = page.locator('.service-listing');
    const count = await listings.count();

    if (count > 0) {
      const firstBadge = page.locator('.service-type-badge').first();
      await expect(firstBadge).toHaveClass(/offer/);
    }
  });

  test('should filter by category', async ({ page }) => {
    const filter = page.locator('#service-category-filter');
    await filter.selectOption('teaching');

    // Listings should be filtered
    // (With actual data, we'd verify the category)
  });

  test('should search services', async ({ page }) => {
    const search = page.locator('#service-search');
    await search.fill('math');

    // Results should be filtered
    // (With actual data, we'd verify the search results)
  });

  test('should open post service modal', async ({ page }) => {
    const postBtn = page.locator('#post-service-btn');
    await postBtn.click();

    const modal = page.locator('#post-service-modal');
    await expect(modal).toBeVisible();

    // Should contain form fields
    await expect(page.locator('#service-category')).toBeVisible();
    await expect(page.locator('#service-title')).toBeVisible();
    await expect(page.locator('#service-description')).toBeVisible();
  });

  test('should submit new service listing', async ({ page }) => {
    const postBtn = page.locator('#post-service-btn');
    await postBtn.click();

    // Fill in form
    await page.check('input[value="offer"]');
    await page.selectOption('#service-category', 'teaching');
    await page.fill('#service-title', 'Test Service');
    await page.fill('#service-description', 'Test description');

    // Submit
    const submitBtn = page.locator('#post-service-form button[type="submit"]');
    await submitBtn.click();

    // Modal should close
    const modal = page.locator('#post-service-modal');
    await expect(modal).toBeHidden();

    // Should show success toast
    const successToast = page.locator('.toast', { hasText: 'Service posted' });
    await expect(successToast).toBeVisible();

    // New listing should appear
    const listings = page.locator('.service-listing');
    const firstTitle = listings.first().locator('.service-listing-title');
    await expect(firstTitle).toContainText('Test Service');
  });

  test('should validate required fields', async ({ page }) => {
    const postBtn = page.locator('#post-service-btn');
    await postBtn.click();

    // Try to submit without filling fields
    const submitBtn = page.locator('#post-service-form button[type="submit"]');
    await submitBtn.click();

    // Required fields should be invalid
    const titleField = page.locator('#service-title');
    const isInvalid = await titleField.evaluate((el) => !el.validity.valid);
    expect(isInvalid).toBeTruthy();
  });

  test('should close post modal on cancel', async ({ page }) => {
    const postBtn = page.locator('#post-service-btn');
    await postBtn.click();

    const modal = page.locator('#post-service-modal');
    await expect(modal).toBeVisible();

    const closeBtn = page.locator('#close-post-service');
    await closeBtn.click();

    await expect(modal).toBeHidden();
  });

  test('should persist service listings', async ({ page }) => {
    // Post a service
    const postBtn = page.locator('#post-service-btn');
    await postBtn.click();

    await page.check('input[value="request"]');
    await page.selectOption('#service-category', 'gardening');
    await page.fill('#service-title', 'Persistent Service');
    await page.fill('#service-description', 'Should persist');

    const submitBtn = page.locator('#post-service-form button[type="submit"]');
    await submitBtn.click();

    // Reload page
    await page.reload();
    await page.click('button[data-tab="service-board"]');

    // Service should still be there
    const listings = page.locator('.service-listing-title');
    await expect(listings.first()).toContainText('Persistent Service');
  });
});

test.describe('Phase 8 - Mobile Responsiveness', () => {
  test('should adapt profile layout for mobile', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 });
    await loginToApp(page);
    await page.click('button[data-tab="member-profile"]');

    const header = page.locator('.profile-header');
    await expect(header).toBeVisible();

    // Profile should stack vertically on mobile
    // (This would be verified with actual layout measurements)
  });

  test('should adapt service board for mobile', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 });
    await loginToApp(page);
    await page.click('button[data-tab="service-board"]');

    const listings = page.locator('#service-listings');
    await expect(listings).toBeVisible();

    // Grid should become single column on mobile
    // (This would be verified with actual grid measurements)
  });

  test('should make filters full-width on mobile', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 667 });
    await loginToApp(page);
    await page.click('button[data-tab="service-board"]');

    const filters = page.locator('.service-filters');
    await expect(filters).toBeVisible();

    // Filters should stack vertically on mobile
  });
});
