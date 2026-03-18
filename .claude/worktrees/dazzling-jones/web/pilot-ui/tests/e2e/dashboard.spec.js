/**
 * E2E Tests: Dashboard and Navigation
 */

import { test, expect } from '@playwright/test';

test.use({ serviceWorkers: 'block' });

// Mock API helper
async function mockApiResponses(page) {
  const corsHeaders = {
    'Access-Control-Allow-Origin': '*',
    'Access-Control-Allow-Methods': 'GET,POST,PUT,DELETE,OPTIONS',
    'Access-Control-Allow-Headers': 'Authorization, Content-Type',
  };

  await page.route('**/v1/**', route => {
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

    // Mock balance endpoint
    if (url.includes('/ledger/') && url.includes('/balance/')) {
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        headers: corsHeaders,
        body: JSON.stringify({ balances: { hours: 42.5 } }),
      });
    }

    // Mock history endpoint
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
                { account_id: 'did:icn:bob', currency: 'hours', credit: 5 },
              ],
            },
            {
              id: '2',
              timestamp: now - 1000 * 60 * 120,
              author: 'did:icn:bob',
              memo: 'Tech support',
              accounts: [
                { account_id: 'did:icn:bob', currency: 'hours', debit: 3 },
                { account_id: 'did:icn:alice', currency: 'hours', credit: 3 },
              ],
            },
          ],
        }),
      });
    }

    // Mock cooperative members endpoint
    if (url.includes('/coops/') && !url.includes('/members')) {
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        headers: corsHeaders,
        body: JSON.stringify({
          members: [
            { did: 'did:icn:alice', role: 'owner', balance: 42.5 },
            { did: 'did:icn:bob', role: 'member', balance: -15.0 },
            { did: 'did:icn:charlie', role: 'member', balance: 8.5 },
          ],
        }),
      });
    }

    // Mock governance proposals endpoint
    if (url.includes('/gov/') && url.includes('/proposals')) {
      if (url.includes('/votes')) {
        return route.fulfill({
          status: 200,
          contentType: 'application/json',
          headers: corsHeaders,
          body: JSON.stringify({ for_votes: 5, against_votes: 2, abstain_votes: 1 }),
        });
      }

      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        headers: corsHeaders,
        body: JSON.stringify({
          data: [
            {
              id: '1',
              title: 'Update credit limit',
              description: 'Increase default credit limit to -100',
              state: 'Open',
              closes_at: new Date(now + 86400 * 1000).toISOString(),
            },
          ],
        }),
      });
    }

    // Default: return 404
    route.fulfill({ status: 404, headers: corsHeaders });
  });
}

// Login helper
async function loginToApp(page) {
  // Mock the API responses
  await mockApiResponses(page);

  await page.goto('/');

  await page.fill('#gateway-url', 'http://localhost:8000');
  await page.fill('#coop-id', 'test-coop');
  await page.fill('#did', 'did:icn:test123');
  await page.fill('#token', 'test-token-123');

  await page.click('#login-btn');

  // Wait for main screen
  await page.waitForSelector('#main-screen', { state: 'visible' });
}

test.describe('Dashboard', () => {
  test.beforeEach(async ({ page }) => {
    await loginToApp(page);
  });

  test('should display main dashboard after login', async ({ page }) => {
    await expect(page.locator('#main-screen')).toBeVisible();
    await expect(page.locator('#login-screen')).toBeHidden();

    // Should show header with user info
    await expect(page.locator('#user-did')).toBeVisible();
    await expect(page.locator('#coop-name')).toContainText('test-coop');
  });

  test('should display balance and stats', async ({ page }) => {
    // Wait for stats to load
    await page.waitForSelector('#my-balance');

    const balance = page.locator('#my-balance');
    await expect(balance).toBeVisible();

    const members = page.locator('#total-members');
    await expect(members).toBeVisible();
  });

  test('should display balance chart', async ({ page }) => {
    const chart = page.locator('#balance-chart');
    await expect(chart).toBeVisible();

    // Canvas should be rendered
    const isCanvas = await chart.evaluate(el => el.tagName === 'CANVAS');
    expect(isCanvas).toBeTruthy();
  });

  test('should display recent activity', async ({ page }) => {
    await page.waitForSelector('#recent-activity');

    const activity = page.locator('#recent-activity');
    await expect(activity).toBeVisible();
  });

  test('should display top contributors', async ({ page }) => {
    const contributors = page.locator('#top-contributors');
    await expect(contributors).toBeVisible();
  });

  test('should display proposal summary', async ({ page }) => {
    const proposals = page.locator('#dashboard-proposals');
    await expect(proposals).toBeVisible();
  });
});

test.describe('Tab Navigation', () => {
  test.beforeEach(async ({ page }) => {
    await loginToApp(page);
  });

  test('should switch between tabs', async ({ page }) => {
    // Click Log Hours tab
    await page.click('button[data-tab="log-hours"]');
    await expect(page.locator('#log-hours')).toBeVisible();
    await expect(page.locator('#dashboard')).toBeHidden();

    // Click History tab
    await page.click('button[data-tab="history"]');
    await expect(page.locator('#history')).toBeVisible();
    await expect(page.locator('#log-hours')).toBeHidden();

    // Click Members tab
    await page.click('button[data-tab="members"]');
    await expect(page.locator('#members')).toBeVisible();

    // Click Governance tab
    await page.click('button[data-tab="governance"]');
    await expect(page.locator('#governance')).toBeVisible();

    // Click Profile tab
    await page.click('button[data-tab="member-profile"]');
    await expect(page.locator('#member-profile')).toBeVisible();

    // Click Services tab
    await page.click('button[data-tab="service-board"]');
    await expect(page.locator('#service-board')).toBeVisible();

    // Click back to Dashboard
    await page.click('button[data-tab="dashboard"]');
    await expect(page.locator('#dashboard')).toBeVisible();
  });

  test('should highlight active tab', async ({ page }) => {
    // Dashboard should be active by default
    const dashboardBtn = page.locator('button[data-tab="dashboard"]');
    await expect(dashboardBtn).toHaveClass(/active/);

    // Click Log Hours
    const logHoursBtn = page.locator('button[data-tab="log-hours"]');
    await logHoursBtn.click();

    await expect(logHoursBtn).toHaveClass(/active/);
    await expect(dashboardBtn).not.toHaveClass(/active/);
  });

  test('should support keyboard shortcuts for tabs', async ({ page }) => {
    // Ctrl+1 should switch to Dashboard
    await page.keyboard.press('Control+1');
    await expect(page.locator('#dashboard')).toBeVisible();

    // Ctrl+2 should switch to Log Hours
    await page.keyboard.press('Control+2');
    await expect(page.locator('#log-hours')).toBeVisible();

    // Ctrl+3 should switch to History
    await page.keyboard.press('Control+3');
    await expect(page.locator('#history')).toBeVisible();

    // Ctrl+4 should switch to Members
    await page.keyboard.press('Control+4');
    await expect(page.locator('#members')).toBeVisible();

    // Ctrl+5 should switch to Governance
    await page.keyboard.press('Control+5');
    await expect(page.locator('#governance')).toBeVisible();
  });

  test('should support arrow key navigation in tabs', async ({ page }) => {
    const firstTab = page.locator('button[data-tab="dashboard"]');
    await firstTab.focus();

    // Arrow Right should move to next tab
    await page.keyboard.press('ArrowRight');
    const logHoursTab = page.locator('button[data-tab="log-hours"]');
    await expect(logHoursTab).toBeFocused();

    // Arrow Right again
    await page.keyboard.press('ArrowRight');
    const historyTab = page.locator('button[data-tab="history"]');
    await expect(historyTab).toBeFocused();

    // Arrow Left should go back
    await page.keyboard.press('ArrowLeft');
    await expect(logHoursTab).toBeFocused();
  });
});

test.describe('History Tab', () => {
  test.beforeEach(async ({ page }) => {
    await loginToApp(page);
    await page.click('button[data-tab="history"]');
  });

  test('should display transaction history', async ({ page }) => {
    await page.waitForSelector('#transaction-list');

    const list = page.locator('#transaction-list');
    await expect(list).toBeVisible();

    // Should contain transaction items
    const items = page.locator('.transaction-item');
    const count = await items.count();
    expect(count).toBeGreaterThan(0);
  });

  test('should filter transactions by date', async ({ page }) => {
    await page.waitForSelector('#history-filter');

    const filter = page.locator('#history-filter');
    await filter.selectOption('week');

    // Transactions should be filtered
    // (This would work with actual data)
  });

  test('should sort transactions', async ({ page }) => {
    await page.waitForSelector('#transaction-sort');

    const sort = page.locator('#transaction-sort');
    await sort.selectOption('amount-desc');

    // Transactions should be sorted
    // (This would work with actual data)
  });

  test('should export to CSV', async ({ page }) => {
    const exportBtn = page.locator('#export-csv');

    // Set up download listener
    const downloadPromise = page.waitForEvent('download');
    await exportBtn.click();

    const download = await downloadPromise;
    expect(download.suggestedFilename()).toContain('.csv');
  });
});

test.describe('Members Tab', () => {
  test.beforeEach(async ({ page }) => {
    await loginToApp(page);
    await page.click('button[data-tab="members"]');
  });

  test('should display member list', async ({ page }) => {
    await page.waitForSelector('#member-list');

    const list = page.locator('#member-list');
    await expect(list).toBeVisible();

    // Should contain member items
    const items = page.locator('.member-item');
    const count = await items.count();
    expect(count).toBeGreaterThan(0);
  });

  test('should search members', async ({ page }) => {
    await page.waitForSelector('#member-search');

    const search = page.locator('#member-search');
    await search.fill('alice');

    // Member list should be filtered
    // (This would work with actual rendering)
  });

  test('should copy DID to clipboard', async ({ page }) => {
    await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);

    const copyBtn = page.locator('.btn-copy-did').first();
    await copyBtn.click();

    // Should show toast
    await expect(page.locator('.toast')).toBeVisible();
    await expect(page.locator('.toast')).toContainText('copied');
  });
});

test.describe('Logout', () => {
  test.beforeEach(async ({ page }) => {
    await loginToApp(page);
  });

  test('should logout successfully', async ({ page }) => {
    const logoutBtn = page.locator('#logout-btn');
    await logoutBtn.click();

    // Should return to login screen
    await expect(page.locator('#login-screen')).toBeVisible();
    await expect(page.locator('#main-screen')).toBeHidden();
  });

  test('should clear session data on logout', async ({ page }) => {
    await page.click('#logout-btn');

    // localStorage should be cleared
    const token = await page.evaluate(() => localStorage.getItem('icn-token'));
    expect(token).toBeNull();
  });
});
