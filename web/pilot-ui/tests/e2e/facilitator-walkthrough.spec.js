import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

test.use({ serviceWorkers: 'block' });

test.describe('Fixture-only facilitator walkthrough', () => {
    test('is demo-only and states the read-only boundary', async ({ page }) => {
        await page.goto('/');
        await expect(page.locator('#facilitator-walkthrough')).toHaveClass(/hidden/);
        await expect(page.locator('[data-facilitator-continue]')).toHaveCount(3);
        for (const control of await page.locator('[data-facilitator-continue]').all()) {
            await expect(control).toBeHidden();
        }

        await page.goto('/?mode=demo');
        const walkthrough = page.locator('#facilitator-walkthrough');
        await expect(walkthrough).not.toHaveClass(/hidden/);
        for (const control of await page.locator('[data-facilitator-continue]').all()) {
            await expect(control).not.toHaveClass(/hidden/);
        }
        await expect(walkthrough).toContainText('COMMITTED FICTIONAL FIXTURES');
        await expect(walkthrough).toContainText('READ-ONLY');
        await expect(walkthrough).toContainText('NON-MUTATING');
    });

    test('keeps initial demo load and all four steps same-origin while QR sign-in stays deferred', async ({ page, context }, testInfo) => {
        const localOrigin = new URL(testInfo.project.use.baseURL || 'http://localhost:8000').origin;
        const observedRequests = [];
        context.on('request', (request) => {
            observedRequests.push({ url: request.url(), method: request.method() });
        });

        await page.goto('/?mode=demo');
        await page.waitForLoadState('networkidle');
        await expect(page.locator('script[data-qr-code-library]')).toHaveCount(0);

        await page.getByRole('button', { name: 'Start rehearsal: Standing' }).click();
        await expect(page.locator('#member-standing-content')).toContainText('Demo organizer (fictional)');
        await page.getByRole('button', { name: 'Continue to Action Cards' }).click();
        await expect(page.locator('.member-action-card')).toHaveCount(4);
        await page.getByRole('button', { name: 'Continue to Review Preview' }).click();
        await expect(page.locator('.organizer-review-row-select')).toHaveCount(4);
        await page.getByRole('button', { name: 'Continue to receipt and evidence explanation' }).click();
        await expect(page.locator('#facilitator-evidence-explanation')).toHaveAttribute('open', '');

        const externalRequests = observedRequests.filter((request) => new URL(request.url).origin !== localOrigin);
        expect(externalRequests).toEqual([]);
        expect(observedRequests.map((request) => request.url)).not.toContain(
            'https://cdnjs.cloudflare.com/ajax/libs/qrcodejs/1.0.0/qrcode.min.js'
        );

        const fixtureRequests = observedRequests.filter((request) => {
            const url = new URL(request.url);
            return url.pathname.startsWith('/fixtures/icn-organizer-demo/');
        });
        expect(fixtureRequests.length).toBeGreaterThan(0);
        expect(fixtureRequests.every((request) => (
            request.method === 'GET' && new URL(request.url).origin === localOrigin
        ))).toBeTruthy();
        expect(fixtureRequests.some((request) => request.url.endsWith('/standing.json'))).toBeTruthy();
        expect(fixtureRequests.some((request) => request.url.endsWith('/action-cards.json'))).toBeTruthy();
        expect(fixtureRequests.some((request) => request.url.endsWith('/preview-review.pending-publish-summary.json'))).toBeTruthy();
        expect(fixtureRequests.some((request) => request.url.endsWith('/pending-publish-summary.json'))).toBeTruthy();

        const walkthrough = page.locator('#facilitator-walkthrough');
        await expect(walkthrough).toContainText('COMMITTED FICTIONAL FIXTURES');
        await expect(walkthrough).toContainText('READ-ONLY');
        await expect(walkthrough).toContainText('NON-MUTATING');
        await expect(walkthrough).toContainText('No receipt or evidence packet is created');
    });

    test('continues the four-step story with forward Tab from each destination heading', async ({ page }) => {
        await page.goto('/?mode=demo');
        const writeRequests = [];
        page.on('request', (request) => {
            if (!['GET', 'HEAD'].includes(request.method())) writeRequests.push(request.url());
        });

        const firstStep = page.getByRole('button', { name: 'Start rehearsal: Standing' });
        await firstStep.focus();
        await page.keyboard.press('Enter');
        await expect(page.locator('#member-standing-heading')).toBeFocused();

        await page.keyboard.press('Tab');
        const continueToCards = page.getByRole('button', { name: 'Continue to Action Cards' });
        await expect(continueToCards).toBeFocused();
        await page.keyboard.press('Enter');
        await expect(page.locator('#member-action-cards-heading')).toBeFocused();

        await page.keyboard.press('Tab');
        const continueToReview = page.getByRole('button', { name: 'Continue to Review Preview' });
        await expect(continueToReview).toBeFocused();
        await page.keyboard.press('Enter');
        await expect(page.locator('#organizer-review-heading')).toBeFocused();

        await page.keyboard.press('Tab');
        const continueToEvidence = page.getByRole('button', { name: 'Continue to receipt and evidence explanation' });
        await expect(continueToEvidence).toBeFocused();
        await page.keyboard.press('Enter');
        await expect(page.locator('#facilitator-evidence-summary')).toBeFocused();
        await expect(page.locator('#facilitator-evidence-explanation')).toHaveAttribute('open', '');

        expect(writeRequests).toEqual([]);
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
        for (const button of await page.locator('[data-facilitator-continue]').all()) {
            await expect(button).toBeEnabled();
            await expect(button).not.toHaveAttribute('formaction');
        }

        await buttons.first().focus();
        await page.keyboard.press('Enter');
        await expect(buttons.first()).toHaveAttribute('aria-current', 'step');
        await expect(page.locator('#facilitator-walkthrough-status')).toContainText('No participant standing is queried or changed');
    });

    test('leads with plain fixture labels and identifies frozen dates and missing demo workflows', async ({ page }) => {
        await page.goto('/?mode=demo');
        await page.getByRole('button', { name: 'Start rehearsal: Standing' }).click();

        await expect(page.locator('.standing-status').first()).toHaveText('Member');
        await expect(page.locator('.standing-source').first()).toContainText('Committed demo member list');
        await expect(page.locator('.standing-role-scope .standing-scope-badge').first()).toHaveText('Schedule sessions');
        await expect(page.getByRole('heading', { name: 'Combined responsibilities' })).toBeVisible();

        const standingTechnical = page.getByText('Technical standing fixture values');
        await expect(standingTechnical).toBeVisible();
        await standingTechnical.click();
        await expect(page.locator('.member-fixture-technical-detail').first()).toContainText('program_review');

        await page.getByRole('button', { name: 'Continue to Action Cards' }).click();
        const firstCard = page.locator('.member-action-card').first();
        await expect(firstCard.locator('.badge-source')).toHaveText('Proposal');
        await expect(firstCard.locator('.badge-action')).toHaveText('Vote');
        await expect(firstCard.locator('.badge-risk')).toHaveText('Standard risk');
        await expect(firstCard).toContainText('Demo fixture date');
        await expect(firstCard).toContainText('frozen fictional value; not a current deadline');
        await expect(firstCard).toContainText('does not complete the action or create a receipt');

        const technicalCardValues = firstCard.getByText('Technical action-card fixture values');
        await expect(technicalCardValues).toBeVisible();
        await technicalCardValues.click();
        await expect(firstCard).toContainText('program_review');

        const demoLimits = page.locator('#member-action-cards-demo-limits');
        await expect(demoLimits).toBeVisible();
        await expect(demoLimits).toContainText('not implemented in this demo fixture');
        await expect(demoLimits).toContainText('challenge windows');
        await expect(demoLimits).toContainText('help-request paths');
    });

    test('has no automated WCAG A/AA violations in the touched demo surfaces', async ({ page }) => {
        await page.goto('/?mode=demo');
        await page.getByRole('button', { name: 'View Action Cards' }).click();
        const standingResults = await new AxeBuilder({ page })
            .include('#facilitator-walkthrough')
            .include('#my-standing')
            .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
            .analyze();
        expect(standingResults.violations).toEqual([]);

        await page.getByRole('button', { name: 'Open Review Preview' }).click();
        await page.getByRole('button', { name: 'Explain receipt / evidence' }).click();
        const reviewResults = await new AxeBuilder({ page })
            .include('#facilitator-walkthrough')
            .include('#review-preview')
            .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
            .analyze();
        expect(reviewResults.violations).toEqual([]);
    });

    test('does not overflow a narrow mobile viewport', async ({ page }) => {
        await page.setViewportSize({ width: 375, height: 812 });
        await page.goto('/?mode=demo');
        await page.getByRole('button', { name: 'View Action Cards' }).click();
        await expect(page.getByRole('button', { name: 'Continue to Review Preview' })).toBeVisible();
        const dimensions = await page.evaluate(() => ({
            viewport: document.documentElement.clientWidth,
            content: document.documentElement.scrollWidth,
        }));
        expect(dimensions.content).toBeLessThanOrEqual(dimensions.viewport);
    });
});
