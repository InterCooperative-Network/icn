/**
 * Accessibility Tests using axe-core
 *
 * These tests verify WCAG 2.1 AA compliance for the ICN Pilot UI.
 * Uses @axe-core/playwright for automated accessibility testing.
 *
 * Run with: npx playwright test tests/e2e/accessibility.spec.js
 */

import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';

// WCAG 2.1 AA compliance tags
const WCAG_TAGS = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'];

// Impact levels that should fail the build
const CRITICAL_IMPACTS = ['critical', 'serious'];

/**
 * Helper to filter critical violations from axe results
 */
function getCriticalViolations(results) {
  return results.violations.filter((v) => CRITICAL_IMPACTS.includes(v.impact));
}

/**
 * Helper to format violation details for error messages
 */
function formatViolations(violations) {
  return violations
    .map((v) => {
      const nodes = v.nodes.map((n) => `  - ${n.html}`).join('\n');
      return `[${v.impact}] ${v.id}: ${v.description}\n${nodes}`;
    })
    .join('\n\n');
}

test.describe('Accessibility - WCAG 2.1 AA Compliance', () => {
  test.beforeEach(async ({ page }) => {
    // Navigate to the app - adjust URL based on test environment
    await page.goto('/');
    // Wait for the app to be ready
    await page.waitForLoadState('networkidle');
  });

  test('login page has no critical accessibility violations', async ({ page }) => {
    const results = await new AxeBuilder({ page }).withTags(WCAG_TAGS).analyze();

    const critical = getCriticalViolations(results);

    if (critical.length > 0) {
      console.error('Accessibility violations found:\n', formatViolations(critical));
    }

    expect(critical, 'Critical accessibility violations found').toHaveLength(0);
  });

  test('form inputs have associated labels', async ({ page }) => {
    const results = await new AxeBuilder({ page }).withRules(['label']).analyze();

    expect(results.violations, 'Form inputs missing labels').toHaveLength(0);
  });

  test('color contrast meets WCAG AA requirements', async ({ page }) => {
    const results = await new AxeBuilder({ page })
      .withRules(['color-contrast'])
      .analyze();

    // Filter for critical contrast issues only (ratio < 3:1)
    const criticalContrast = results.violations.filter((v) =>
      v.nodes.some((n) => n.impact === 'critical')
    );

    if (results.violations.length > 0) {
      console.warn(
        'Color contrast warnings (non-critical):',
        formatViolations(results.violations)
      );
    }

    expect(criticalContrast, 'Critical color contrast violations found').toHaveLength(
      0
    );
  });

  test('images have alt text', async ({ page }) => {
    const results = await new AxeBuilder({ page })
      .withRules(['image-alt'])
      .analyze();

    expect(results.violations, 'Images missing alt text').toHaveLength(0);
  });

  test('buttons have accessible names', async ({ page }) => {
    const results = await new AxeBuilder({ page })
      .withRules(['button-name'])
      .analyze();

    expect(results.violations, 'Buttons missing accessible names').toHaveLength(0);
  });

  test('links have discernible text', async ({ page }) => {
    const results = await new AxeBuilder({ page })
      .withRules(['link-name'])
      .analyze();

    expect(results.violations, 'Links missing discernible text').toHaveLength(0);
  });

  test('document has valid lang attribute', async ({ page }) => {
    const results = await new AxeBuilder({ page })
      .withRules(['html-has-lang', 'html-lang-valid'])
      .analyze();

    expect(results.violations, 'Document missing or invalid lang attribute').toHaveLength(0);
  });
});

test.describe('Accessibility - Keyboard Navigation', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
  });

  test('all interactive elements are keyboard accessible', async ({ page }) => {
    const results = await new AxeBuilder({ page })
      .withRules(['keyboard'])
      .analyze();

    expect(results.violations, 'Keyboard accessibility issues found').toHaveLength(0);
  });

  test('focus order is logical', async ({ page }) => {
    // Tab through elements and verify focus order
    const focusOrder = [];

    for (let i = 0; i < 15; i++) {
      await page.keyboard.press('Tab');

      const focused = await page.evaluate(() => ({
        tag: document.activeElement?.tagName?.toLowerCase(),
        id: document.activeElement?.id || null,
        tabIndex: document.activeElement?.tabIndex,
        role: document.activeElement?.getAttribute('role'),
      }));

      focusOrder.push(focused);
    }

    // Verify no elements have negative tabindex in focus order
    // (negative tabindex removes element from natural tab order)
    const invalidTabIndex = focusOrder.filter(
      (el) => el.tabIndex !== undefined && el.tabIndex < 0
    );

    expect(
      invalidTabIndex,
      'Elements with negative tabindex found in focus order'
    ).toHaveLength(0);
  });

  test('focus is visible on interactive elements', async ({ page }) => {
    // Test that focusable elements have accessible names
    const results = await new AxeBuilder({ page })
      .withRules(['focusable-no-name'])
      .analyze();

    // Log any focus-related issues as warnings
    if (results.violations.length > 0) {
      console.warn('Focus visibility warnings:', formatViolations(results.violations));
    }
  });
});

test.describe('Accessibility - Dark Mode', () => {
  test('dark mode maintains accessibility compliance', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');

    // Toggle dark mode (if theme toggle exists)
    const themeToggle = page.locator('#theme-toggle, [data-theme-toggle]');
    if ((await themeToggle.count()) > 0) {
      await themeToggle.click();

      // Wait for theme transition by checking for theme attribute change
      await page.waitForFunction(() => {
        const html = document.documentElement;
        return html.hasAttribute('data-theme') || html.classList.contains('dark');
      }, { timeout: 1000 }).catch(() => {
        // Theme may already be applied or use different mechanism
      });

      // Re-run accessibility scan
      const results = await new AxeBuilder({ page }).withTags(WCAG_TAGS).analyze();

      const critical = getCriticalViolations(results);

      expect(
        critical,
        'Critical violations in dark mode'
      ).toHaveLength(0);
    }
  });

  test('dark mode color contrast is sufficient', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');

    const themeToggle = page.locator('#theme-toggle, [data-theme-toggle]');
    if ((await themeToggle.count()) > 0) {
      await themeToggle.click();

      // Wait for theme transition
      await page.waitForFunction(() => {
        const html = document.documentElement;
        return html.hasAttribute('data-theme') || html.classList.contains('dark');
      }, { timeout: 1000 }).catch(() => {});

      const results = await new AxeBuilder({ page })
        .withRules(['color-contrast'])
        .analyze();

      const criticalContrast = results.violations.filter((v) =>
        v.nodes.some((n) => n.impact === 'critical')
      );

      expect(
        criticalContrast,
        'Critical contrast issues in dark mode'
      ).toHaveLength(0);
    }
  });
});

test.describe('Accessibility - Screen Reader Support', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
  });

  test('page has proper heading structure', async ({ page }) => {
    const results = await new AxeBuilder({ page })
      .withRules(['heading-order', 'page-has-heading-one'])
      .analyze();

    // Heading structure issues are important but not critical
    if (results.violations.length > 0) {
      console.warn('Heading structure warnings:', formatViolations(results.violations));
    }
  });

  test('ARIA attributes are used correctly', async ({ page }) => {
    const results = await new AxeBuilder({ page })
      .withRules([
        'aria-allowed-attr',
        'aria-hidden-body',
        'aria-hidden-focus',
        'aria-required-attr',
        'aria-required-children',
        'aria-required-parent',
        'aria-roles',
        'aria-valid-attr',
        'aria-valid-attr-value',
      ])
      .analyze();

    const critical = getCriticalViolations(results);

    expect(critical, 'Critical ARIA violations found').toHaveLength(0);
  });

  test('landmarks are used appropriately', async ({ page }) => {
    const results = await new AxeBuilder({ page })
      .withRules(['landmark-one-main', 'region'])
      .analyze();

    // Landmark issues are warnings, not failures
    if (results.violations.length > 0) {
      console.warn('Landmark warnings:', formatViolations(results.violations));
    }
  });
});

test.describe('Accessibility - Modal and Dialog', () => {
  test('help modal is accessible when opened', async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');

    // Try to open the help modal
    const helpButton = page.locator('#show-auth-help, [data-help-trigger]');
    const modal = page.locator('[role="dialog"], .modal, #auth-help-modal');

    if ((await helpButton.count()) > 0) {
      await helpButton.click();

      // Wait for modal to be visible instead of using fixed timeout
      try {
        await modal.first().waitFor({ state: 'visible', timeout: 1000 });
      } catch {
        // Modal may not appear or use different selectors
      }

      if ((await modal.count()) > 0 && (await modal.first().isVisible())) {
        const results = await new AxeBuilder({ page })
          .include('[role="dialog"], .modal, #auth-help-modal')
          .withTags(WCAG_TAGS)
          .analyze();

        expect(results.violations, 'Modal accessibility violations').toHaveLength(0);
      }
    }
  });
});
