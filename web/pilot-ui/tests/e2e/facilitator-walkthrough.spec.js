import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

test.use({ serviceWorkers: 'block' });

test.describe('Fixture-only facilitator walkthrough', () => {
    test('is demo-only and states the read-only boundary', async ({ page }) => {
        await page.goto('/');
        await expect(page.locator('#facilitator-walkthrough')).toHaveClass(/hidden/);

        await page.goto('/?mode=demo');
        const walkthrough = page.locator('#facilitator-walkthrough');
        await expect(walkthrough).not.toHaveClass(/hidden/);
        await expect(walkthrough).toContainText('COMMITTED FICTIONAL FIXTURES');
        await expect(walkthrough).toContainText('READ-ONLY');
        await expect(walkthrough).toContainText('NON-MUTATING');
    });

    test('walks Standing, Action Cards, and Review Preview using local fixtures', async ({ page }) => {
        await page.goto('/?mode=demo');
        const fixtureRequests = [];
        page.on('request', (request) => {
            if (request.url().includes('/fixtures/icn-organizer-demo/')) fixtureRequests.push(request.url());
        });

        await page.getByRole('button', { name: 'Start rehearsal: Standing' }).click();
        await expect(page.locator('#my-standing')).toHaveClass(/active/);
        await expect(page.locator('#member-standing-heading')).toBeFocused();
        await expect(page.locator('#member-standing-content')).toContainText('Demo organizer (fictional)');

        await page.getByRole('button', { name: 'View Action Cards' }).click();
        await expect(page.locator('#member-action-cards-heading')).toBeFocused();
        await expect(page.locator('.member-action-card')).toHaveCount(4);

        await page.getByRole('button', { name: 'Open Review Preview' }).click();
        await expect(page.locator('#review-preview')).toHaveClass(/active/);
        await expect(page.locator('#organizer-review-heading')).toBeFocused();
        await expect(page.locator('.organizer-review-row-select')).toHaveCount(4);

        expect(fixtureRequests.every((url) => new URL(url).origin === 'http://localhost:8000')).toBeTruthy();
        expect(fixtureRequests.some((url) => url.endsWith('/standing.json'))).toBeTruthy();
        expect(fixtureRequests.some((url) => url.endsWith('/action-cards.json'))).toBeTruthy();
        expect(fixtureRequests.some((url) => url.endsWith('/preview-review.pending-publish-summary.json'))).toBeTruthy();
        expect(fixtureRequests.some((url) => url.endsWith('/pending-publish-summary.json'))).toBeTruthy();
    });

    test('explains fixture categories without claiming a receipt or export', async ({ page }) => {
        await page.goto('/?mode=demo');
        await page.getByRole('button', { name: 'Explain receipt / evidence' }).click();

        const explanation = page.locator('#facilitator-evidence-explanation');
        await expect(explanation).toHaveAttribute('open', '');
        await expect(page.locator('#facilitator-evidence-summary')).toBeFocused();
        await expect(explanation).toContainText('Action item completion receipt');
        await expect(explanation).toContainText('Governance receipt');
        await expect(explanation).toContainText('Attendance receipt');
        await expect(explanation).toContainText('Settlement receipt');
        await expect(explanation).toContainText('Expected does not mean issued');
        await expect(explanation).toContainText('creates and exports nothing');
        await expect(explanation).toContainText('Steward / operator boundary');
        await expect(explanation).toContainText('Future, not implemented');
    });

    test('keeps every walkthrough action keyboard reachable and non-mutating', async ({ page }) => {
        await page.goto('/?mode=demo');
        const buttons = page.locator('[data-facilitator-step]');
        await expect(buttons).toHaveCount(4);
        for (const button of await buttons.all()) {
            await expect(button).toBeEnabled();
            await expect(button).not.toHaveAttribute('formaction');
        }

        await buttons.first().focus();
        await page.keyboard.press('Enter');
        await expect(buttons.first()).toHaveAttribute('aria-current', 'step');
        await expect(page.locator('#facilitator-walkthrough-status')).toContainText('No participant standing is queried or changed');
    });

    test('has no automated WCAG A/AA violations in the walkthrough panel', async ({ page }) => {
        await page.goto('/?mode=demo');
        await page.getByRole('button', { name: 'Explain receipt / evidence' }).click();
        const results = await new AxeBuilder({ page })
            .include('#facilitator-walkthrough')
            .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
            .analyze();
        expect(results.violations).toEqual([]);
    });

    test('does not overflow a narrow mobile viewport', async ({ page }) => {
        await page.setViewportSize({ width: 375, height: 812 });
        await page.goto('/?mode=demo');
        await page.getByRole('button', { name: 'Explain receipt / evidence' }).click();
        const dimensions = await page.evaluate(() => ({
            viewport: document.documentElement.clientWidth,
            content: document.documentElement.scrollWidth,
        }));
        expect(dimensions.content).toBeLessThanOrEqual(dimensions.viewport);
    });
});
