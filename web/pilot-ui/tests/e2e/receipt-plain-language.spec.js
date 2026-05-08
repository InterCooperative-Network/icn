/**
 * E2E Tests — Receipt Plain-Language Summary (PR 4)
 *
 * Verifies that the new receipt plain-language summary section is
 * wired correctly above the existing structured chain detail and
 * does not replace, duplicate, or modify the existing renderers.
 *
 * As with PR 2 / PR 3, we assert against DOM presence + text content
 * rather than driving the full login flow + a working gateway.
 * Behavior of renderResults() under live data is exercised by
 * integration tests elsewhere; the tests here protect the pilot-ui-
 * side composition: container exists, lives above the structured
 * detail, default empty-state is plain language, and the structured
 * detail wrapper preserves the existing allocations / intents /
 * ledger renderers below.
 */

import { test, expect } from '@playwright/test';

test.use({ serviceWorkers: 'block' });

test.describe('Receipt Plain-Language Summary (PR 4)', () => {
    test('summary container exists and is positioned above the structured detail', async ({ page }) => {
        await page.goto('/');
        await page.waitForLoadState('networkidle');

        const summary = page.locator('#receipt-plain-language-summary');
        await expect(summary).toHaveCount(1);

        const detail = page.locator('#receipt-structured-detail');
        await expect(detail).toHaveCount(1);

        // Summary must precede the structured detail in DOM order so
        // viewers see plain language first.
        const summaryHandle = await summary.elementHandle();
        const detailHandle = await detail.elementHandle();
        const positionInfo = await summaryHandle.evaluate((s, d) => {
            return s.compareDocumentPosition(d) & Node.DOCUMENT_POSITION_FOLLOWING;
        }, detailHandle);
        expect(positionInfo).toBeTruthy();
    });

    test('default empty-state is plain language (pre-query)', async ({ page }) => {
        await page.goto('/');
        await page.waitForLoadState('networkidle');

        const summary = page.locator('#receipt-plain-language-summary');
        await expect(summary).toContainText('Query a decision hash');
        await expect(summary).toContainText('plain language');
    });

    test('structured detail wrapper preserves the three existing renderer sections', async ({ page }) => {
        await page.goto('/');
        await page.waitForLoadState('networkidle');

        // The <details> element has a <summary> child describing the section.
        const detail = page.locator('#receipt-structured-detail');
        await expect(detail).toContainText('Structured detail');
        await expect(detail).toContainText('allocations');
        await expect(detail).toContainText('intents');
        await expect(detail).toContainText('ledger');

        // Existing renderers are still inside the wrapper.
        const allocationsList = page.locator('#allocations-list');
        const intentsList = page.locator('#intents-list');
        const ledgerList = page.locator('#ledger-list');
        await expect(allocationsList).toHaveCount(1);
        await expect(intentsList).toHaveCount(1);
        await expect(ledgerList).toHaveCount(1);
    });

    test('summary block is open by default (no click required to see plain language)', async ({ page }) => {
        await page.goto('/');
        await page.waitForLoadState('networkidle');

        // The summary itself is not a <details>; it's a regular div that
        // is always visible. The structured detail IS a <details>; it
        // should be `open` by default so stewards see it without an
        // extra click, but the user does not need to expand anything to
        // see the plain-language summary above.
        const summary = page.locator('#receipt-plain-language-summary');
        await expect(summary).toBeAttached();

        // The structured detail's `open` attribute is set in HTML.
        const detail = page.locator('#receipt-structured-detail');
        const openAttr = await detail.getAttribute('open');
        // <details open> serializes as either "" (empty string) or "open" depending on browser
        expect(openAttr).not.toBeNull();
    });
});
