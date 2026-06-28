import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

test.use({ serviceWorkers: 'block' });

async function openReviewPreview(page) {
    await page.goto('/?mode=demo');
    await page.waitForLoadState('networkidle');
    const nav = page.locator('#review-preview-nav-btn');
    await nav.click();
    await expect(page.locator('#review-preview')).toHaveClass(/active/);
    await expect(page.locator('.organizer-review-row-select')).toHaveCount(4);
}

test.describe('Fixture-only organizer review preview', () => {
    test('is hidden outside demo mode and visible in demo mode', async ({ page }) => {
        await page.goto('/');
        await page.waitForLoadState('networkidle');
        await expect(page.locator('#review-preview-nav-btn')).toHaveClass(/hidden/);

        await page.goto('/?mode=demo');
        await page.waitForLoadState('networkidle');
        await expect(page.locator('#review-preview-nav-btn')).not.toHaveClass(/hidden/);
    });

    test('loads only the two same-origin review fixtures when opened', async ({ page }) => {
        await page.goto('/?mode=demo');
        await page.waitForLoadState('networkidle');

        const requests = [];
        page.on('request', (request) => requests.push(request.url()));
        await page.locator('#review-preview-nav-btn').click();
        await expect(page.locator('.organizer-review-row-select')).toHaveCount(4);

        expect(requests).toHaveLength(2);
        expect(requests.every((url) => new URL(url).origin === 'http://localhost:8000')).toBeTruthy();
        expect(requests.some((url) => url.endsWith('/preview-review.pending-publish-summary.json'))).toBeTruthy();
        expect(requests.some((url) => url.endsWith('/pending-publish-summary.json'))).toBeTruthy();
    });

    test('states the fixture and no-mutation boundary before row controls', async ({ page }) => {
        await openReviewPreview(page);

        const overview = page.locator('.organizer-review-overview');
        await expect(overview).toContainText('FIXTURE ONLY');
        await expect(overview).toContainText('READ-ONLY');
        await expect(overview).toContainText('NOTHING CHANGES HERE');
        await expect(overview).toContainText('4 fictional items are ready for review');
        await expect(overview).toContainText('Reviewed clean (fictional fixture)');
        await expect(overview).toContainText('Safe to show (fictional fixture)');
    });

    test('renders plain-language row detail, assignee-label boundary, and expected receipt', async ({ page }) => {
        await openReviewPreview(page);

        const detail = page.locator('.organizer-review-detail');
        await expect(detail).toContainText('Example Working Group to draft a sample agenda');
        await expect(detail).toContainText('Assignee label');
        await expect(detail).toContainText('Example steward');
        await expect(detail).toContainText('not a DID');
        await expect(detail).toContainText('Before anything changes');
        await expect(detail).toContainText('No record is created by rendering this row');
        await expect(detail).toContainText('Expected evidence: Action item completion receipt');
        await expect(detail).not.toContainText('did:icn:');
    });

    test('renders only contract-provided review choices and keeps every choice disabled', async ({ page }) => {
        await openReviewPreview(page);

        const firstActions = page.locator('.organizer-review-actions button');
        await expect(firstActions).toHaveCount(4);
        await expect(firstActions.nth(0)).toHaveText('Approve');
        await expect(firstActions.nth(1)).toHaveText('Reject');
        await expect(firstActions.nth(2)).toHaveText('Request changes');
        await expect(firstActions.nth(3)).toHaveText('Ask for information');
        for (const button of await firstActions.all()) {
            await expect(button).toBeDisabled();
        }

        await page.locator('.organizer-review-row-select').nth(1).click();
        const decisionActions = page.locator('.organizer-review-actions button');
        await expect(decisionActions).toHaveCount(5);
        await expect(decisionActions.last()).toHaveText('Review later');
        await expect(decisionActions.last()).toHaveAttribute('data-review-action', 'defer');
        for (const button of await decisionActions.all()) {
            await expect(button).toBeDisabled();
        }
    });

    test('keeps wrapper and row status copy distinct', async ({ page }) => {
        await openReviewPreview(page);

        await expect(page.locator('.organizer-review-overview-facts')).toContainText('Ready for review');
        await expect(page.locator('.organizer-review-detail-facts')).toContainText('Awaiting row review');

        await page.locator('.organizer-review-row-select').nth(2).click();
        await expect(page.locator('.organizer-review-detail-facts')).toContainText('Row needs more information');
    });

    test('navigation is keyboard reachable and selected rows expose pressed state', async ({ page }) => {
        await page.goto('/?mode=demo');
        await page.waitForLoadState('networkidle');

        const nav = page.locator('#review-preview-nav-btn');
        await nav.focus();
        await page.keyboard.press('Enter');
        await expect(page.locator('#review-preview')).toHaveClass(/active/);

        const rows = page.locator('.organizer-review-row-select');
        await expect(rows).toHaveCount(4);
        await expect(rows.first()).toHaveAttribute('aria-pressed', 'true');
        await rows.nth(1).click();
        await expect(rows.nth(1)).toHaveAttribute('aria-pressed', 'true');
    });

    test('has no automated WCAG A/AA violations in the rendered review surface', async ({ page }) => {
        await openReviewPreview(page);
        const results = await new AxeBuilder({ page })
            .include('#review-preview')
            .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
            .analyze();
        expect(results.violations).toEqual([]);
    });

    test('does not overflow the viewport at the configured desktop/mobile widths', async ({ page }) => {
        await openReviewPreview(page);
        const dimensions = await page.evaluate(() => ({
            viewport: document.documentElement.clientWidth,
            content: document.documentElement.scrollWidth,
        }));
        expect(dimensions.content).toBeLessThanOrEqual(dimensions.viewport);
    });
});
