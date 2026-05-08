/**
 * E2E Tests — ICN Organizer Demo Fixture Preload (PR 5 narrow slice)
 *
 * Verifies the FIRST fixture slice for the guided pilot-ui
 * organizer/member demo: when ?mode=demo is set, loadMemberStanding()
 * and loadMemberActionCards() short-circuit to committed JSON fixtures
 * under web/pilot-ui/fixtures/icn-organizer-demo/ instead of fetching
 * the gateway. This is a frontend-only slice; the backend
 * --demo-mode flag (ICN #1727) remains OPEN.
 *
 * The test asserts the fixture files are reachable at the expected
 * relative URL and that they conform to the expected shape. The
 * actual render path through pilot-ui requires sign-in to expose
 * #main-screen and is exercised by integration tests / manual
 * verification rather than driven here, consistent with PRs 2-4.
 */

import { test, expect } from '@playwright/test';

test.use({ serviceWorkers: 'block' });

test.describe('ICN organizer demo fixture preload (PR 5 narrow slice)', () => {
    test('standing fixture is reachable and has expected shape', async ({ page }) => {
        await page.goto('/');
        const standing = await page.evaluate(async () => {
            const r = await fetch('fixtures/icn-organizer-demo/standing.json', {
                headers: { 'Accept': 'application/json' },
            });
            if (!r.ok) throw new Error(`standing fixture fetch failed: ${r.status}`);
            return r.json();
        });

        // Required top-level fields per StandingResponse.
        expect(typeof standing.did).toBe('string');
        expect(standing.did.startsWith('did:icn:example-')).toBeTruthy();
        expect(standing.did.endsWith('-not-live')).toBeTruthy();
        expect(Array.isArray(standing.domains)).toBeTruthy();
        expect(Array.isArray(standing.roles)).toBeTruthy();
        expect(Array.isArray(standing.authority_scopes)).toBeTruthy();
        expect(typeof standing.generated_at).toBe('number');

        // Each domain has the expected fields.
        for (const d of standing.domains) {
            expect(typeof d.domain_id).toBe('string');
            expect(typeof d.domain_name).toBe('string');
            expect(['static_list', 'trust_threshold']).toContain(d.membership_source);
            expect(['member', 'unverified']).toContain(d.status);
        }

        // Each role has the expected fields. StandingRoleAssignment
        // requires `start_date` (u64) per the Rust model in
        // icn/apps/governance/src/http/models.rs — pin it here so
        // fixture drift from the wire shape gets caught.
        for (const r of standing.roles) {
            expect(typeof r.role).toBe('string');
            expect(typeof r.structure_id).toBe('string');
            expect(typeof r.role_assignment_id).toBe('string');
            expect(Array.isArray(r.authority_scope)).toBeTruthy();
            expect(typeof r.start_date).toBe('number');
            // end_date is optional; when present it must also be a number.
            if (r.end_date !== undefined) {
                expect(typeof r.end_date).toBe('number');
            }
        }
    });

    test('action-cards fixture is reachable and has expected wrapper shape', async ({ page }) => {
        await page.goto('/');
        const response = await page.evaluate(async () => {
            const r = await fetch('fixtures/icn-organizer-demo/action-cards.json', {
                headers: { 'Accept': 'application/json' },
            });
            if (!r.ok) throw new Error(`action-cards fixture fetch failed: ${r.status}`);
            return r.json();
        });

        // ActionCardsResponse wrapper: { did, cards, generated_at }.
        // Per icn/apps/governance/src/http/models.rs — NOT a bare array.
        expect(typeof response.did).toBe('string');
        expect(response.did.startsWith('did:icn:example-')).toBeTruthy();
        expect(typeof response.generated_at).toBe('number');
        expect(Array.isArray(response.cards)).toBeTruthy();
        expect(response.cards.length).toBeGreaterThan(0);

        const cards = response.cards;

        // Schema enum coverage: risk_level must use schema values
        // (low | normal | elevated). PR 3 review caught a previous
        // version that used the wrong values; this test pins the
        // schema-correct enum.
        const validRiskLevels = ['low', 'normal', 'elevated'];
        // ActionCard.scope is also an enum per the schema:
        // entity | structure | individual. PR 5 review caught a
        // previous version that used a domain id as scope.
        const validScopes = ['entity', 'structure', 'individual'];

        for (const c of cards) {
            // Required fields per action-card.schema.json (id,
            // source_kind, action_kind, scope, title, summary,
            // authority_basis, required_authority_scope, risk_level,
            // accessibility_hint, receipt_expected, source_id).
            expect(typeof c.id).toBe('string');
            expect(typeof c.source_kind).toBe('string');
            expect(typeof c.action_kind).toBe('string');
            expect(validScopes).toContain(c.scope);
            expect(typeof c.title).toBe('string');
            expect(typeof c.summary).toBe('string');
            expect(typeof c.authority_basis).toBe('string');
            expect(Array.isArray(c.required_authority_scope)).toBeTruthy();
            expect(validRiskLevels).toContain(c.risk_level);
            expect(typeof c.accessibility_hint).toBe('string');
            expect(typeof c.receipt_expected).toBe('boolean');
            expect(typeof c.source_id).toBe('string');
        }
    });

    test('fixture pack uses fictional non-live identifiers throughout', async ({ page }) => {
        await page.goto('/');
        const [standing, cards] = await page.evaluate(async () => {
            const s = await (await fetch('fixtures/icn-organizer-demo/standing.json')).json();
            const c = await (await fetch('fixtures/icn-organizer-demo/action-cards.json')).json();
            return [s, c];
        });

        // Standing DID convention.
        expect(standing.did).toMatch(/^did:icn:example-.*-not-live$/);

        // No real-contact pattern leaks. Stringify the full pack and
        // check against common real-data shapes; pure fictional
        // fixtures should produce zero matches.
        const blob = JSON.stringify(standing) + JSON.stringify(cards);
        // Real email-provider domains (gmail, outlook, hotmail, yahoo,
        // proton, icloud).
        expect(blob).not.toMatch(/@gmail\.com|@outlook\.com|@hotmail\.com|@yahoo\.com|@proton\.me|@icloud\.com/i);
        // Common phone formats (XXX-XXX-XXXX, XXX.XXX.XXXX).
        expect(blob).not.toMatch(/\b\d{3}[-.]\d{3}[-.]\d{4}\b/);
    });

    test('fixture pack covers the three source-kind classes the §3 narrative needs', async ({ page }) => {
        await page.goto('/');
        const response = await page.evaluate(async () =>
            (await fetch('fixtures/icn-organizer-demo/action-cards.json')).json()
        );

        // The fixture is an ActionCardsResponse wrapper; extract cards array.
        const cards = response.cards;
        const sourceKinds = new Set(cards.map((c) => c.source_kind));
        // The §3 Guided workflow narrative walks through proposal/vote,
        // meeting/attend, and action_item/complete. Cover all three so
        // the demo path is recognizable end-to-end.
        expect(sourceKinds.has('proposal')).toBeTruthy();
        expect(sourceKinds.has('meeting')).toBeTruthy();
        expect(sourceKinds.has('action_item')).toBeTruthy();
    });
});
