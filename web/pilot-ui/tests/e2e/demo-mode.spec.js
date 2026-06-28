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
        await expect(guide).toContainText('ICN Organizing Committee Demo');
        await expect(guide).toContainText('Where you are');
        await expect(guide).toContainText('Available now in this local demo');
        await expect(guide).toContainText('Fixture-backed today');
        await expect(guide).toContainText('Visible but still gateway-backed');
        await expect(guide).toContainText('Not implemented in this demo');
        await expect(guide).toContainText('Future narrow slices');
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
        await expect(nonClaims).toContainText('fictional fixtures');
        await expect(nonClaims).toContainText('Not implemented in this demo');
    });

    test('Demo Guide says review preview, standing, and action cards are fixture-backed (not coming-next)', async ({ page }) => {
        await page.goto('/?mode=demo');
        await page.waitForLoadState('networkidle');

        const guide = page.locator('#demo-guide');
        // Review preview joins the existing standing + action-card fixture slice.
        // Old "PR 3 / Not yet wired" framing must be gone.
        await expect(guide).toContainText('Review Preview surface');
        await expect(guide).toContainText('My Standing surface');
        await expect(guide).toContainText('Action Cards surface');
        await expect(guide).toContainText('Fixture-backed today');

        // Stale tranche labels must NOT appear.
        await expect(guide).not.toContainText('Coming next in this demo tranche');
        await expect(guide).not.toContainText('Not yet wired');
        await expect(guide).not.toContainText('arrives in PR');
    });

    test('Demo Guide says governance and receipt-chain data are not fixture-backed yet', async ({ page }) => {
        await page.goto('/?mode=demo');
        await page.waitForLoadState('networkidle');

        const gatewayBacked = page.locator('.demo-guide-gateway-backed');
        await expect(gatewayBacked).toContainText('Governance proposals / votes');
        await expect(gatewayBacked).toContainText('Receipt-chain data');
        await expect(gatewayBacked).toContainText('Ledger / history');
        await expect(gatewayBacked).toContainText('Members');
        await expect(gatewayBacked).toContainText('Trust');
        await expect(gatewayBacked).toContainText('Federation');
    });

    test('?mode=demo auto-bootstraps the main screen without a running gateway', async ({ page }) => {
        // Verification fix on top of #1773: ?mode=demo previously required
        // a real login (and therefore a running gateway) before the demo
        // surfaces became visible. The bootstrap shim transitions straight
        // into the main screen with fictional identity so the demo path
        // works under a static server.
        await page.goto('/?mode=demo');
        await page.waitForLoadState('networkidle');

        // Login screen hidden, main screen visible.
        const loginScreen = page.locator('#login-screen');
        const mainScreen = page.locator('#main-screen');
        await expect(loginScreen).toHaveClass(/hidden/);
        await expect(mainScreen).not.toHaveClass(/hidden/);

        // Demo Guide tab is active by default.
        const demoGuideTab = page.locator('#demo-guide');
        await expect(demoGuideTab).toHaveClass(/active/);

        // Banner reads DEMO and shows the fictional identity.
        const badge = page.locator('#demo-mode-badge');
        await expect(badge).toHaveText('DEMO');
        const userDid = page.locator('#user-did');
        await expect(userDid).toContainText('Demo organizer (fictional)');
    });

    test('without ?mode=demo, login screen still gates the main screen', async ({ page }) => {
        // The bootstrap is gated on DEMO_MODE === 'demo'. Verify the
        // non-demo path is unchanged: login-screen visible, main-screen
        // hidden until real login.
        await page.goto('/');
        await page.waitForLoadState('networkidle');

        const loginScreen = page.locator('#login-screen');
        const mainScreen = page.locator('#main-screen');
        await expect(loginScreen).not.toHaveClass(/hidden/);
        await expect(mainScreen).toHaveClass(/hidden/);
    });

    test('?mode=demo retitles the app away from "ICN Timebank"', async ({ page }) => {
        // Demo shell must read as an organizing-committee demo on first
        // glance, not as a timebank app. Document title (browser tab),
        // header title, and login-screen title must all be retitled.
        await page.goto('/?mode=demo');
        await page.waitForLoadState('networkidle');

        await expect(page).toHaveTitle('ICN Organizing Committee Demo');

        const headerTitle = page.locator('#app-title-header');
        await expect(headerTitle).toHaveText('ICN Organizing Committee Demo');
        await expect(headerTitle).not.toHaveText('ICN Timebank');

        const loginTitle = page.locator('#app-title-login');
        await expect(loginTitle).toHaveText('ICN Organizing Committee Demo');
    });

    test('non-demo path keeps "ICN Timebank" title', async ({ page }) => {
        // Discovery did not show the non-demo shell to be obsolete, so
        // the existing live/timebank surface stays as-is until the
        // user explicitly asks for a global rename.
        await page.goto('/');
        await page.waitForLoadState('networkidle');

        await expect(page).toHaveTitle('ICN Timebank');
        const loginTitle = page.locator('#app-title-login');
        await expect(loginTitle).toHaveText('ICN Timebank');
    });

    test('?mode=demo hides timebank-oriented nav buttons (Dashboard/Log Hours/History/Members/Profile)', async ({ page }) => {
        // The organizing-committee demo's preferred nav is:
        //   Demo Guide • Review Preview • My Standing & Action Cards • Governance • Receipts (• Federation, optional)
        // Timebank-oriented surfaces are hidden in demo mode (only — they
        // remain in the DOM and remain visible in non-demo mode).
        await page.goto('/?mode=demo');
        await page.waitForLoadState('networkidle');

        for (const tab of ['dashboard', 'log-hours', 'history', 'members', 'member-profile']) {
            const btn = page.locator(`[data-tab="${tab}"]`);
            await expect(btn).toHaveClass(/hidden/);
        }

        // Organizing-path nav remains visible (no hidden class).
        for (const tab of ['demo-guide', 'review-preview', 'my-standing', 'governance', 'receipts']) {
            const btn = page.locator(`[data-tab="${tab}"]`);
            await expect(btn).not.toHaveClass(/hidden/);
        }
    });

    test('non-demo path keeps timebank-oriented nav buttons visible', async ({ page }) => {
        await page.goto('/');
        await page.waitForLoadState('networkidle');

        for (const tab of ['dashboard', 'log-hours', 'history', 'members', 'member-profile']) {
            const btn = page.locator(`[data-tab="${tab}"]`);
            await expect(btn).not.toHaveClass(/hidden/);
        }
    });

    test('?mode=demo banner copy is current (no stale "PR 5" claim)', async ({ page }) => {
        await page.goto('/?mode=demo');
        await page.waitForLoadState('networkidle');

        const message = page.locator('#demo-mode-message');
        await expect(message).toContainText('Local organizer/member demo');
        await expect(message).toContainText('fixture-backed');
        await expect(message).not.toContainText('PR 5');
        await expect(message).not.toContainText('arrives in');
    });
});
