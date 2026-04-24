/**
 * Tests for steward dashboard gateway URL derivation logic.
 * Mirrors the deriveGatewayUrl function in steward-dashboard.js.
 */

// Inline the function under test (browser script has no module exports)
function deriveGatewayUrl(hostname, protocol, savedGateway) {
    if (savedGateway) return savedGateway;
    if (hostname !== 'localhost' && hostname !== '127.0.0.1') {
        return `${protocol}//${hostname}:30080`;
    }
    return 'http://localhost:8080';
}

describe('deriveGatewayUrl', () => {
    test('explicit localStorage override wins over everything', () => {
        expect(deriveGatewayUrl('10.8.30.40', 'http:', 'http://custom-gateway:9000'))
            .toBe('http://custom-gateway:9000');
    });

    test('explicit override wins even on localhost', () => {
        expect(deriveGatewayUrl('localhost', 'http:', 'http://override:8888'))
            .toBe('http://override:8888');
    });

    test('non-localhost host derives port 30080 from same hostname', () => {
        expect(deriveGatewayUrl('10.8.30.40', 'http:', null))
            .toBe('http://10.8.30.40:30080');
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
        expect(deriveGatewayUrl('10.8.30.40', 'http:', undefined))
            .toBe('http://10.8.30.40:30080');
    });

    test('empty string savedGateway treated as no override', () => {
        expect(deriveGatewayUrl('localhost', 'http:', ''))
            .toBe('http://localhost:8080');
    });
});
