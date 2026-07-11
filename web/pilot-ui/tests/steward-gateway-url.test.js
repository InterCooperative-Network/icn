/**
 * Tests for steward dashboard gateway URL derivation logic.
 * Loads the real deriveGatewayUrl function from steward-dashboard.js
 * so tests exercise the production implementation.
 */

const fs = require('fs');
const path = require('path');

function loadDeriveGatewayUrl() {
    const stewardDashboardPath = path.resolve(__dirname, '../steward-dashboard.js');
    const source = fs.readFileSync(stewardDashboardPath, 'utf8');
    const match = source.match(/function\s+deriveGatewayUrl\s*\([^)]*\)\s*\{[\s\S]*?\n\}/);

    if (!match) {
        throw new Error(`Could not locate deriveGatewayUrl in ${stewardDashboardPath}`);
    }

    return new Function(`${match[0]}; return deriveGatewayUrl;`)();
}

const deriveGatewayUrl = loadDeriveGatewayUrl();

describe('deriveGatewayUrl', () => {
    test('explicit localStorage override wins over everything', () => {
        expect(deriveGatewayUrl('192.0.2.10', 'http:', 'http://custom-gateway:9000'))
            .toBe('http://custom-gateway:9000');
    });

    test('explicit override wins even on localhost', () => {
        expect(deriveGatewayUrl('localhost', 'http:', 'http://override:8888'))
            .toBe('http://override:8888');
    });

    test('non-localhost host derives port 30080 from same hostname', () => {
        expect(deriveGatewayUrl('192.0.2.10', 'http:', null))
            .toBe('http://192.0.2.10:30080');
    });

    test('preserves https protocol for TLS deployments', () => {
        expect(deriveGatewayUrl('icn.example.coop', 'https:', null))
            .toBe('https://icn.example.coop:30080');
    });

    test('localhost falls back to dev gateway', () => {
        expect(deriveGatewayUrl('localhost', 'http:', null))
            .toBe('http://localhost:8080');
    });

    test('127.0.0.1 falls back to dev gateway', () => {
        expect(deriveGatewayUrl('127.0.0.1', 'http:', null))
            .toBe('http://localhost:8080');
    });

    test('undefined savedGateway treated as no override', () => {
        expect(deriveGatewayUrl('192.0.2.10', 'http:', undefined))
            .toBe('http://192.0.2.10:30080');
    });

    test('empty string savedGateway treated as no override', () => {
        expect(deriveGatewayUrl('localhost', 'http:', ''))
            .toBe('http://localhost:8080');
    });
});
