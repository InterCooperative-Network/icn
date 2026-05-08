/**
 * E2E Tests — My Standing & Action Cards (PR 3)
 *
 * Verifies that the new "My Standing & Action Cards" pilot-ui surface
 * is wired correctly: the nav button is in the DOM with the right
 * data-tab, the section structure exists with the expected sub-sections,
 * the empty-state messaging is plain language, and the section is
 * explicitly distinct from the existing per-domain Action Items tab.
 *
 * As with PR 2, we assert against DOM presence + text content rather
 * than driving the full login flow + a working gateway. The fetch
 * behavior of loadMemberStanding() / loadMemberActionCards() requires
 * a live gateway and is exercised by integration tests elsewhere; the
 * tests here protect the pilot-ui-side composition.
 */

import { test, expect } from '@playwright/test';

test.use({ serviceWorkers: 'block' });

test.describe('My Standing & Action Cards (PR 3)', () => {
    test('nav button exists with correct data-tab', async ({ page }) => {
        await page.goto('/');
        await page.waitForLoadState('networkidle');

        const navBtn = page.locator('[data-tab="my-standing"]');
        await expect(navBtn).toHaveCount(1);
        await expect(navBtn).toContainText('My Standing');
        await expect(navBtn).toContainText('Action Cards');
    });

    test('tab section structure has both standing and action-cards sub-sections', async ({ page }) => {
        await page.goto('/');
        await page.waitForLoadState('networkidle');

        const section = page.locator('#my-standing');
        await expect(section).toHaveCount(1);
        await expect(section).toContainText('My Standing');
        await expect(section).toContainText('Action Cards');

        // Standing sub-section
        const standingHeading = page.locator('#member-standing-heading');
        await expect(standingHeading).toHaveText('My Standing');
        const standingContent = page.locator('#member-standing-content');
        await expect(standingContent).toHaveCount(1);

        // Action cards sub-section
        const cardsHeading = page.locator('#member-action-cards-heading');
        await expect(cardsHeading).toHaveText('Action Cards');
        const cardsList = page.locator('#member-action-cards-list');
        await expect(cardsList).toHaveCount(1);
    });

    test('intro text labels Action Cards as distinct from Action Items', async ({ page }) => {
        await page.goto('/');
        await page.waitForLoadState('networkidle');

        const intro = page.locator('.member-action-cards-intro');
        await expect(intro).toContainText('distinct');
        await expect(intro).toContainText('Action Items tab');
        await expect(intro).toContainText('different endpoint and concept');
        // Cite the actual endpoint that this surface consumes
        await expect(intro).toContainText('/v1/gov/me/action-cards');
    });

    test('standing intro labels the standing read-model endpoint', async ({ page }) => {
        await page.goto('/');
        await page.waitForLoadState('networkidle');

        const intro = page.locator('.member-standing-intro');
        await expect(intro).toContainText('/v1/gov/me/standing');
        await expect(intro).toContainText('trust graph');
        await expect(intro).toContainText('unverified');
    });

    test('empty-state messaging is plain language pre-fetch', async ({ page }) => {
        await page.goto('/');
        await page.waitForLoadState('networkidle');

        // Pre-login / pre-fetch: empty-state placeholders should be visible
        // in DOM. The "Loading…" copy avoids alarming the user before any
        // fetch has run.
        const standingContent = page.locator('#member-standing-content');
        await expect(standingContent).toContainText('Loading');

        const cardsList = page.locator('#member-action-cards-list');
        await expect(cardsList).toContainText('Loading');
    });

    test('Action Items tab (governance subtab) is unchanged', async ({ page }) => {
        await page.goto('/');
        await page.waitForLoadState('networkidle');

        // The existing Action Items button under Governance must remain.
        // PR 3 must NOT collapse Action Items into Action Cards.
        const actionItemsBtn = page.locator('[data-governance-tab="action-items"]');
        await expect(actionItemsBtn).toHaveCount(1);
        await expect(actionItemsBtn).toContainText('Action Items');

        // The action-items list container (driven by loadActionItems) is
        // still present with its own id, distinct from member-action-cards-list.
        const actionItemsList = page.locator('#action-items-list');
        await expect(actionItemsList).toHaveCount(1);

        // Sanity: the new member-action-cards-list is not the same element.
        const cardsList = page.locator('#member-action-cards-list');
        await expect(cardsList).toHaveCount(1);
    });
});
