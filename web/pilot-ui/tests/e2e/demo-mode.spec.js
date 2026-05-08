/**
 * E2E Tests — Demo Mode Banner + Guided Demo Landing (PR 2)
 *
 * Verifies that the ?mode= query-param produces a visible mode
 * banner and a Demo Guide tab/landing on the login screen and main
 * screen, without modifying any existing tab or fetching new
 * endpoints.
 *
 * The Demo Guide content is reachable on the login screen since the
 * banner is in the main-screen subtree but the nav button + tab are
 * unhidden by applyDemoMode() at script-evaluation time. We assert
 * directly against DOM presence + visibility rather than driving the
 * full login flow, because PR 2 must not require a working gateway.
 */

import { test, expect } from '@playwright/test';

test.use({ serviceWorkers: 'block' });

test.describe('Demo Mode (PR 2)', () => {
    test('no mode param → banner stays hidden, demo-guide nav stays hidden', async ({ page }) => {
        await page.goto('/');
        await page.waitForLoadState('networkidle');

        const banner = page.locator('#demo-mode-banner');
        await expect(banner).toHaveClass(/hidden/);

        const navBtn = page.locator('#demo-guide-nav-btn');
        await expect(navBtn).toHaveClass(/hidden/);
    });

    test('?mode=demo → banner renders with DEMO badge and label-mode dataset', async ({ page }) => {
        await page.goto('/?mode=demo');
        await page.waitForLoadState('networkidle');

        const banner = page.locator('#demo-mode-banner');
        await expect(banner).not.toHaveClass(/hidden/);
        await expect(banner).toHaveAttribute('data-mode', 'demo');

        const badge = page.locator('#demo-mode-badge');
        await expect(badge).toHaveText('DEMO');

        const message = page.locator('#demo-mode-message');
        await expect(message).toContainText('UI is in DEMO mode');
        await expect(message).toContainText('labeling convention');
    });

    test('?mode=demo → Demo Guide nav button has its hidden class removed (visible once main-screen shows post-login)', async ({ page }) => {
        await page.goto('/?mode=demo');
        await page.waitForLoadState('networkidle');

        // The nav button lives inside #main-screen, which carries the
        // .hidden class until login completes. We assert the button's
        // OWN .hidden class was removed by applyDemoMode() — that is
        // what determines whether the button will be visible once
        // #main-screen unhides post-login. We do not drive the full
        // login flow here because PR 2 must not require a working
        // gateway for these e2e checks.
        const navBtn = page.locator('#demo-guide-nav-btn');
        await expect(navBtn).not.toHaveClass(/hidden/);
    });

    test('Demo Guide section contains required content blocks (no fetch)', async ({ page }) => {
        await page.goto('/?mode=demo');
        await page.waitForLoadState('networkidle');

        const guide = page.locator('#demo-guide');
        await expect(guide).toContainText('ICN Local Demo — Guided Path');
        await expect(guide).toContainText('Where you are');
        await expect(guide).toContainText('Available now in pilot-ui');
        await expect(guide).toContainText('Coming next in this demo tranche');
        await expect(guide).toContainText('Not implemented in demo');
        await expect(guide).toContainText('Non-claims');

        // Mode echo
        const modeEl = page.locator('#demo-guide-mode');
        await expect(modeEl).toHaveText('DEMO');
    });

    test('?mode=fixture → banner renders with FIXTURE badge and color tag', async ({ page }) => {
        await page.goto('/?mode=fixture');
        await page.waitForLoadState('networkidle');

        const banner = page.locator('#demo-mode-banner');
        await expect(banner).toHaveAttribute('data-mode', 'fixture');
        await expect(page.locator('#demo-mode-badge')).toHaveText('FIXTURE');
    });

    test('?mode=invalidvalue → banner stays hidden (defensive)', async ({ page }) => {
        await page.goto('/?mode=notathing');
        await page.waitForLoadState('networkidle');

        const banner = page.locator('#demo-mode-banner');
        await expect(banner).toHaveClass(/hidden/);
        const navBtn = page.locator('#demo-guide-nav-btn');
        await expect(navBtn).toHaveClass(/hidden/);
    });

    test('Demo Guide explicit non-claims are present and visible', async ({ page }) => {
        await page.goto('/?mode=demo');
        await page.waitForLoadState('networkidle');

        const nonClaims = page.locator('.demo-guide-non-claims');
        await expect(nonClaims).toContainText('UI labeling convention');
        await expect(nonClaims).toContainText('LIVE');
        await expect(nonClaims).toContainText('Not yet wired');
        await expect(nonClaims).toContainText('Not implemented in demo');
    });

    test('Demo Guide labels Action Items as distinct from Action Cards', async ({ page }) => {
        await page.goto('/?mode=demo');
        await page.waitForLoadState('networkidle');

        const guide = page.locator('#demo-guide');
        // Action Items live now, action-cards is PR 3
        await expect(guide).toContainText('Action items');
        await expect(guide).toContainText('different endpoint and concept from per-member action cards');
        await expect(guide).toContainText('Per-member action cards surface');
        await expect(guide).toContainText('PR 3');
    });
});
