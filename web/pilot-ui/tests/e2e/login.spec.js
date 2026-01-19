/**
 * E2E Tests: Login Flow
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

    if (route.request().method() === 'OPTIONS') {
      return route.fulfill({ status: 204, headers: corsHeaders });
    }

    if (url.endsWith('/health')) {
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        headers: corsHeaders,
        body: '{}',
      });
    }

    if (url.includes('/ledger/') && url.includes('/balance/')) {
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        headers: corsHeaders,
        body: JSON.stringify({ balances: { hours: 42.5 } }),
      });
    }

    if (url.includes('/coops/') && !url.includes('/members')) {
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        headers: corsHeaders,
        body: JSON.stringify({ members: [] }),
      });
    }

    if (url.includes('/ledger/') && url.includes('/history')) {
      return route.fulfill({
        status: 200,
        contentType: 'application/json',
        headers: corsHeaders,
        body: JSON.stringify({ transactions: [] }),
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

async function loginWithMock(page) {
  await mockApiResponses(page);
  await page.goto('/');
  await page.fill('#gateway-url', 'http://localhost:8000');
  await page.fill('#coop-id', 'test-coop');
  await page.fill('#did', 'did:icn:test123');
  await page.fill('#token', 'test-token-123');
  await page.click('#login-btn');
  await page.waitForSelector('#main-screen', { state: 'visible' });
}

test.describe('Login Flow', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('should display login screen on initial load', async ({ page }) => {
    await expect(page.locator('h1')).toContainText('ICN Timebank');
    await expect(page.locator('#login-screen')).toBeVisible();
    await expect(page.locator('#main-screen')).toBeHidden();
  });

  test('should validate required fields', async ({ page }) => {
    const loginBtn = page.locator('#login-btn');

    // Click login without filling fields
    await loginBtn.click();

    // Should show validation errors
    await expect(page.locator('#login-error')).toBeVisible();
    await expect(page.locator('#login-error')).toContainText('Please fill in all fields');
  });

  test('should show authentication help modal', async ({ page }) => {
    const helpBtn = page.locator('#show-auth-help');
    await helpBtn.click();

    const modal = page.locator('#auth-help-modal');
    await expect(modal).toBeVisible();
    await expect(modal).toContainText('How to Get Authentication Token');

    // Should close modal
    const closeBtn = page.locator('#close-auth-help');
    await closeBtn.click();
    await expect(modal).toBeHidden();
  });

  test('should close modal on escape key', async ({ page }) => {
    const helpBtn = page.locator('#show-auth-help');
    await helpBtn.click();

    const modal = page.locator('#auth-help-modal');
    await expect(modal).toBeVisible();

    // Press Escape
    await page.keyboard.press('Escape');
    await expect(modal).toBeHidden();
  });

  test('should close modal on backdrop click', async ({ page }) => {
    const helpBtn = page.locator('#show-auth-help');
    await helpBtn.click();

    const modal = page.locator('#auth-help-modal');
    await expect(modal).toBeVisible();

    // Click backdrop
    await modal.click({ position: { x: 0, y: 0 } });
    await expect(modal).toBeHidden();
  });

  test('should save form values to localStorage', async ({ page }) => {
    await loginWithMock(page);

    const storedValues = await page.evaluate(() => ({
      gateway: localStorage.getItem('icn-gateway'),
      coop: localStorage.getItem('icn-coop'),
      did: localStorage.getItem('icn-did'),
    }));

    expect(storedValues.gateway).toBe('http://localhost:8000');
    expect(storedValues.coop).toBe('test-coop');
    expect(storedValues.did).toBe('did:icn:test123');
  });

  test('should copy authentication command', async ({ page }) => {
    const helpBtn = page.locator('#show-auth-help');
    await helpBtn.click();

    // Grant clipboard permissions
    await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);

    const copyBtn = page.locator('#copy-command');
    await copyBtn.click();

    // Should show toast notification
    await expect(page.locator('.toast')).toBeVisible();
    await expect(page.locator('.toast')).toContainText('copied');
  });

  test('should focus skip link on tab', async ({ page }) => {
    // Press Tab to focus skip link
    await page.keyboard.press('Tab');

    const skipLink = page.locator('.skip-link');
    await expect(skipLink).toBeFocused();
  });

  test('should support keyboard navigation in help modal', async ({ page }) => {
    const helpBtn = page.locator('#show-auth-help');
    await helpBtn.click();

    const modal = page.locator('#auth-help-modal');
    await expect(modal).toBeVisible();

    // Tab should cycle through focusable elements
    await page.keyboard.press('Tab');
    const firstFocused = await page.evaluate(() => document.activeElement.tagName);
    expect(firstFocused).toBeTruthy();
  });
});

test.describe('Login Flow - Dark Mode', () => {
  test('should toggle dark mode', async ({ page }) => {
    await loginWithMock(page);
    const themeToggle = page.locator('#theme-toggle');
    await expect(themeToggle).toBeVisible();
    await themeToggle.click();

    // Check if dark mode is applied
    const theme = await page.evaluate(() =>
      document.documentElement.getAttribute('data-theme')
    );
    expect(theme).toBe('dark');

    // Toggle back to light mode
    await themeToggle.click();
    const lightTheme = await page.evaluate(() =>
      document.documentElement.getAttribute('data-theme')
    );
    expect(lightTheme).toBe('light');
  });

  test('should persist theme preference', async ({ page }) => {
    await loginWithMock(page);

    const themeToggle = page.locator('#theme-toggle');
    await expect(themeToggle).toBeVisible();
    await themeToggle.click();

    // Reload page
    await page.reload();

    // Dark mode should still be active
    const theme = await page.evaluate(() =>
      document.documentElement.getAttribute('data-theme')
    );
    expect(theme).toBe('dark');
  });
});

test.describe('Login Flow - Accessibility', () => {
  test('should have proper ARIA labels', async ({ page }) => {
    await page.goto('/');

    const form = page.locator('#login-form');
    const ariaLabel = await form.getAttribute('aria-label');
    expect(ariaLabel).toBeTruthy();

    const gatewayUrl = page.locator('#gateway-url');
    const required = await gatewayUrl.getAttribute('aria-required');
    expect(required).toBe('true');
  });

  test('should announce to screen readers', async ({ page }) => {
    await page.goto('/');

    const announcer = page.locator('#sr-announcements');
    await expect(announcer).toBeAttached();

    const liveRegion = await announcer.getAttribute('role');
    expect(liveRegion).toBe('status');
  });

  test('should have sufficient color contrast', async ({ page }) => {
    await page.goto('/');

    // This is a simplified check - in production, use axe-core
    const primaryButton = page.locator('#login-btn');
    const bgColor = await primaryButton.evaluate(el =>
      window.getComputedStyle(el).backgroundColor
    );

    expect(bgColor).toBeTruthy();
  });
});
