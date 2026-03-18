/**
 * Tests for QR Code utilities
 */

import {
  generatePaymentQR,
  parsePaymentQR,
  isPaymentQR,
  generateReceiveQR,
  parseEnrollmentQR,
  isEnrollmentQR,
  detectQRType,
  parseQR,
  isICNQR,
  generateContactQR,
  parseContactQR,
  isContactQR,
  generateJoinQR,
  parseJoinQR,
  isJoinQR,
} from './qr';

describe('generatePaymentQR', () => {
  it('should generate a valid QR code string', () => {
    const qr = generatePaymentQR({
      to: 'did:icn:alice',
      amount: 5,
      coopId: 'my-timebank',
    });

    expect(qr).toMatch(/^icn:\/\/pay\?/);
    expect(qr).toContain('to=did%3Aicn%3Aalice');
    expect(qr).toContain('amount=5');
    expect(qr).toContain('coop=my-timebank');
    expect(qr).toContain('v=1');
  });

  it('should include memo when provided', () => {
    const qr = generatePaymentQR({
      to: 'did:icn:bob',
      amount: 10,
      coopId: 'food-coop',
      memo: 'For groceries',
    });

    expect(qr).toContain('memo=For+groceries');
  });

  it('should handle special characters in memo', () => {
    const qr = generatePaymentQR({
      to: 'did:icn:bob',
      amount: 10,
      coopId: 'food-coop',
      memo: 'Thanks & goodbye!',
    });

    expect(qr).toContain('memo=');
    // Special chars should be URL encoded
    expect(qr).not.toContain('&goodbye');
  });

  it('should handle zero amount', () => {
    const qr = generatePaymentQR({
      to: 'did:icn:alice',
      amount: 0,
      coopId: 'test-coop',
    });

    expect(qr).toContain('amount=0');
  });

  it('should handle decimal amounts', () => {
    const qr = generatePaymentQR({
      to: 'did:icn:alice',
      amount: 1.5,
      coopId: 'test-coop',
    });

    expect(qr).toContain('amount=1.5');
  });
});

describe('parsePaymentQR', () => {
  it('should parse a valid QR code string', () => {
    const qr = 'icn://pay?v=1&to=did:icn:alice&amount=5&coop=my-timebank';
    const result = parsePaymentQR(qr);

    expect(result).not.toBeNull();
    expect(result?.to).toBe('did:icn:alice');
    expect(result?.amount).toBe(5);
    expect(result?.coopId).toBe('my-timebank');
  });

  it('should parse QR with memo', () => {
    const qr = 'icn://pay?v=1&to=did:icn:bob&amount=10&coop=food-coop&memo=For+groceries';
    const result = parsePaymentQR(qr);

    expect(result?.memo).toBe('For groceries');
  });

  it('should return null for invalid prefix', () => {
    const qr = 'https://example.com/pay?to=alice&amount=5';
    const result = parsePaymentQR(qr);

    expect(result).toBeNull();
  });

  it('should return null for missing required fields', () => {
    expect(parsePaymentQR('icn://pay?v=1&to=alice')).toBeNull(); // missing amount, coop
    expect(parsePaymentQR('icn://pay?v=1&amount=5')).toBeNull(); // missing to, coop
    expect(parsePaymentQR('icn://pay?v=1&to=alice&amount=5')).toBeNull(); // missing coop
  });

  it('should return null for invalid amount', () => {
    const qr = 'icn://pay?v=1&to=alice&amount=invalid&coop=test';
    expect(parsePaymentQR(qr)).toBeNull();
  });

  it('should return null for negative amount', () => {
    expect(parsePaymentQR('icn://pay?v=1&to=alice&amount=-5&coop=test')).toBeNull();
  });

  it('should accept zero amount (receive QR without suggested amount)', () => {
    const qr = 'icn://pay?v=1&to=did:icn:alice&amount=0&coop=test';
    const result = parsePaymentQR(qr);

    expect(result).not.toBeNull();
    expect(result?.amount).toBe(0);
  });

  it('should handle decimal amounts', () => {
    const qr = 'icn://pay?v=1&to=did:icn:alice&amount=1.5&coop=test';
    const result = parsePaymentQR(qr);

    expect(result?.amount).toBe(1.5);
  });

  it('should return null for malformed URL', () => {
    expect(parsePaymentQR('not a url')).toBeNull();
    expect(parsePaymentQR('')).toBeNull();
  });

  it('should roundtrip with generatePaymentQR', () => {
    const original = {
      to: 'did:icn:alice',
      amount: 5,
      coopId: 'my-timebank',
      memo: 'Test payment',
    };

    const qr = generatePaymentQR(original);
    const parsed = parsePaymentQR(qr);

    expect(parsed).toEqual(original);
  });
});

describe('isPaymentQR', () => {
  it('should return true for valid ICN payment QR', () => {
    expect(isPaymentQR('icn://pay?v=1&to=alice&amount=5')).toBe(true);
  });

  it('should return false for other URLs', () => {
    expect(isPaymentQR('https://example.com')).toBe(false);
    expect(isPaymentQR('bitcoin:address')).toBe(false);
    expect(isPaymentQR('random text')).toBe(false);
  });

  it('should return false for empty string', () => {
    expect(isPaymentQR('')).toBe(false);
  });
});

describe('generateReceiveQR', () => {
  it('should generate receive QR with basic params', () => {
    const qr = generateReceiveQR('did:icn:bob', 'my-timebank');

    expect(qr).toMatch(/^icn:\/\/pay\?/);
    expect(qr).toContain('to=did%3Aicn%3Abob');
    expect(qr).toContain('coop=my-timebank');
    expect(qr).toContain('amount=0');
  });

  it('should include suggested amount', () => {
    const qr = generateReceiveQR('did:icn:bob', 'my-timebank', {
      suggestedAmount: 10,
    });

    expect(qr).toContain('amount=10');
  });

  it('should include memo', () => {
    const qr = generateReceiveQR('did:icn:bob', 'my-timebank', {
      memo: 'Donation',
    });

    expect(qr).toContain('memo=Donation');
  });

  it('should include both amount and memo', () => {
    const qr = generateReceiveQR('did:icn:bob', 'my-timebank', {
      suggestedAmount: 25,
      memo: 'Monthly dues',
    });

    expect(qr).toContain('amount=25');
    expect(qr).toContain('memo=Monthly+dues');
  });

  it('should roundtrip with parsePaymentQR (no amount)', () => {
    const qr = generateReceiveQR('did:icn:bob', 'my-timebank');
    const parsed = parsePaymentQR(qr);

    expect(parsed).not.toBeNull();
    expect(parsed?.to).toBe('did:icn:bob');
    expect(parsed?.coopId).toBe('my-timebank');
    expect(parsed?.amount).toBe(0);
  });

  it('should roundtrip with parsePaymentQR (with amount)', () => {
    const qr = generateReceiveQR('did:icn:bob', 'my-timebank', {
      suggestedAmount: 15,
      memo: 'Service fee',
    });
    const parsed = parsePaymentQR(qr);

    expect(parsed).not.toBeNull();
    expect(parsed?.to).toBe('did:icn:bob');
    expect(parsed?.coopId).toBe('my-timebank');
    expect(parsed?.amount).toBe(15);
    expect(parsed?.memo).toBe('Service fee');
  });
});

describe('parseEnrollmentQR', () => {
  it('should parse valid enrollment QR', () => {
    const qrData = JSON.stringify({
      type: 'icn-enrollment',
      enrollment_id: 'abc-123',
      challenge: 'YWJjMTIz',
      gateway_url: 'http://localhost:8080',
    });
    const result = parseEnrollmentQR(qrData);

    expect(result).not.toBeNull();
    expect(result?.type).toBe('icn-enrollment');
    expect(result?.enrollment_id).toBe('abc-123');
    expect(result?.challenge).toBe('YWJjMTIz');
    expect(result?.gateway_url).toBe('http://localhost:8080');
  });

  it('should return null for wrong type', () => {
    const qrData = JSON.stringify({
      type: 'something-else',
      enrollment_id: 'abc-123',
      gateway_url: 'http://localhost:8080',
    });
    expect(parseEnrollmentQR(qrData)).toBeNull();
  });

  it('should return null for missing enrollment_id', () => {
    const qrData = JSON.stringify({
      type: 'icn-enrollment',
      gateway_url: 'http://localhost:8080',
    });
    expect(parseEnrollmentQR(qrData)).toBeNull();
  });

  it('should return null for missing gateway_url', () => {
    const qrData = JSON.stringify({
      type: 'icn-enrollment',
      enrollment_id: 'abc-123',
    });
    expect(parseEnrollmentQR(qrData)).toBeNull();
  });

  it('should return null for invalid JSON', () => {
    expect(parseEnrollmentQR('not json')).toBeNull();
    expect(parseEnrollmentQR('{broken')).toBeNull();
  });

  it('should handle empty challenge', () => {
    const qrData = JSON.stringify({
      type: 'icn-enrollment',
      enrollment_id: 'abc-123',
      gateway_url: 'http://localhost:8080',
    });
    const result = parseEnrollmentQR(qrData);
    expect(result?.challenge).toBe('');
  });
});

describe('isEnrollmentQR', () => {
  it('should return true for valid enrollment QR', () => {
    const qrData = JSON.stringify({
      type: 'icn-enrollment',
      enrollment_id: 'abc-123',
    });
    expect(isEnrollmentQR(qrData)).toBe(true);
  });

  it('should return false for non-enrollment JSON', () => {
    const qrData = JSON.stringify({
      type: 'other-type',
    });
    expect(isEnrollmentQR(qrData)).toBe(false);
  });

  it('should return false for non-JSON', () => {
    expect(isEnrollmentQR('not json')).toBe(false);
    expect(isEnrollmentQR('')).toBe(false);
    expect(isEnrollmentQR('icn://pay?to=alice')).toBe(false);
  });
});

describe('detectQRType', () => {
  it('should detect payment QR', () => {
    expect(detectQRType('icn://pay?v=1&to=alice&amount=5&coop=test')).toBe('payment');
  });

  it('should detect contact QR', () => {
    expect(detectQRType('icn://contact?v=1&did=did:icn:alice')).toBe('contact');
  });

  it('should detect raw DID as contact', () => {
    expect(detectQRType('did:icn:alice123')).toBe('contact');
  });

  it('should detect join QR', () => {
    expect(detectQRType('icn://join?v=1&coop=test&gateway=http://example.com')).toBe('join');
  });

  it('should detect enrollment QR', () => {
    const enrollmentQR = JSON.stringify({ type: 'icn-enrollment', enrollment_id: 'abc' });
    expect(detectQRType(enrollmentQR)).toBe('enrollment');
  });

  it('should return unknown for unrecognized format', () => {
    expect(detectQRType('https://example.com')).toBe('unknown');
    expect(detectQRType('random text')).toBe('unknown');
    expect(detectQRType('')).toBe('unknown');
  });
});

describe('parseQR', () => {
  it('should parse payment QR', () => {
    const qr = 'icn://pay?v=1&to=did:icn:alice&amount=5&coop=test';
    const result = parseQR(qr);

    expect(result.type).toBe('payment');
    expect(result.data).not.toBeNull();
    if (result.type === 'payment') {
      expect(result.data.to).toBe('did:icn:alice');
      expect(result.data.amount).toBe(5);
    }
  });

  it('should parse contact QR', () => {
    const qr = 'icn://contact?v=1&did=did:icn:bob&name=Bob';
    const result = parseQR(qr);

    expect(result.type).toBe('contact');
    if (result.type === 'contact') {
      expect(result.data.did).toBe('did:icn:bob');
      expect(result.data.name).toBe('Bob');
    }
  });

  it('should parse raw DID as contact', () => {
    const qr = 'did:icn:charlie123';
    const result = parseQR(qr);

    expect(result.type).toBe('contact');
    if (result.type === 'contact') {
      expect(result.data.did).toBe('did:icn:charlie123');
    }
  });

  it('should parse join QR', () => {
    const qr = 'icn://join?v=1&coop=my-coop&gateway=http://example.com&name=My%20Coop';
    const result = parseQR(qr);

    expect(result.type).toBe('join');
    if (result.type === 'join') {
      expect(result.data.coopId).toBe('my-coop');
      expect(result.data.gateway).toBe('http://example.com');
      expect(result.data.name).toBe('My Coop');
    }
  });

  it('should parse enrollment QR', () => {
    const qr = JSON.stringify({
      type: 'icn-enrollment',
      enrollment_id: 'enroll-123',
      challenge: 'abc',
      gateway_url: 'http://localhost:8080',
    });
    const result = parseQR(qr);

    expect(result.type).toBe('enrollment');
    if (result.type === 'enrollment') {
      expect(result.data.enrollment_id).toBe('enroll-123');
      expect(result.data.gateway_url).toBe('http://localhost:8080');
    }
  });

  it('should return unknown for invalid QR', () => {
    expect(parseQR('random text').type).toBe('unknown');
    expect(parseQR('').type).toBe('unknown');
    expect(parseQR('https://example.com').type).toBe('unknown');
  });

  it('should return unknown for malformed payment QR', () => {
    const result = parseQR('icn://pay?v=1&to=alice');
    expect(result.type).toBe('unknown');
  });
});

describe('isICNQR', () => {
  it('should return true for payment QR', () => {
    expect(isICNQR('icn://pay?v=1&to=alice&amount=5&coop=test')).toBe(true);
  });

  it('should return true for contact QR', () => {
    expect(isICNQR('icn://contact?v=1&did=alice')).toBe(true);
  });

  it('should return true for raw DID', () => {
    expect(isICNQR('did:icn:alice123')).toBe(true);
  });

  it('should return true for join QR', () => {
    expect(isICNQR('icn://join?v=1&coop=test&gateway=http://example.com')).toBe(true);
  });

  it('should return true for enrollment QR', () => {
    const enrollmentQR = JSON.stringify({ type: 'icn-enrollment', enrollment_id: 'abc' });
    expect(isICNQR(enrollmentQR)).toBe(true);
  });

  it('should return false for other URLs', () => {
    expect(isICNQR('https://example.com')).toBe(false);
    expect(isICNQR('bitcoin:address')).toBe(false);
    expect(isICNQR('random text')).toBe(false);
    expect(isICNQR('')).toBe(false);
  });
});

describe('generateContactQR', () => {
  it('should generate contact QR with DID only', () => {
    const qr = generateContactQR({ did: 'did:icn:alice' });

    expect(qr).toMatch(/^icn:\/\/contact\?/);
    expect(qr).toContain('did=did%3Aicn%3Aalice');
    expect(qr).toContain('v=1');
  });

  it('should include name when provided', () => {
    const qr = generateContactQR({ did: 'did:icn:alice', name: 'Alice' });

    expect(qr).toContain('name=Alice');
  });

  it('should include coop when provided', () => {
    const qr = generateContactQR({ did: 'did:icn:alice', coopId: 'my-coop' });

    expect(qr).toContain('coop=my-coop');
  });
});

describe('parseContactQR', () => {
  it('should parse contact QR URL', () => {
    const qr = 'icn://contact?v=1&did=did:icn:alice&name=Alice&coop=my-coop';
    const result = parseContactQR(qr);

    expect(result?.did).toBe('did:icn:alice');
    expect(result?.name).toBe('Alice');
    expect(result?.coopId).toBe('my-coop');
  });

  it('should parse raw DID', () => {
    const result = parseContactQR('did:icn:bob123');

    expect(result?.did).toBe('did:icn:bob123');
    expect(result?.name).toBeUndefined();
  });

  it('should return null for invalid QR', () => {
    expect(parseContactQR('https://example.com')).toBeNull();
    expect(parseContactQR('icn://contact?v=1')).toBeNull(); // missing did
  });

  it('should roundtrip with generateContactQR', () => {
    const original = { did: 'did:icn:alice', name: 'Alice', coopId: 'my-coop' };
    const qr = generateContactQR(original);
    const parsed = parseContactQR(qr);

    expect(parsed).toEqual(original);
  });
});

describe('isContactQR', () => {
  it('should return true for contact URL', () => {
    expect(isContactQR('icn://contact?v=1&did=alice')).toBe(true);
  });

  it('should return true for raw DID', () => {
    expect(isContactQR('did:icn:alice123')).toBe(true);
  });

  it('should return false for other QR types', () => {
    expect(isContactQR('icn://pay?to=alice')).toBe(false);
    expect(isContactQR('https://example.com')).toBe(false);
  });
});

describe('generateJoinQR', () => {
  it('should generate join QR with required fields', () => {
    const qr = generateJoinQR({
      coopId: 'my-coop',
      gateway: 'http://localhost:8080',
    });

    expect(qr).toMatch(/^icn:\/\/join\?/);
    expect(qr).toContain('coop=my-coop');
    expect(qr).toContain('gateway=http%3A%2F%2Flocalhost%3A8080');
    expect(qr).toContain('v=1');
  });

  it('should include name when provided', () => {
    const qr = generateJoinQR({
      coopId: 'my-coop',
      gateway: 'http://localhost:8080',
      name: 'My Cooperative',
    });

    expect(qr).toContain('name=My+Cooperative');
  });
});

describe('parseJoinQR', () => {
  it('should parse join QR', () => {
    const qr = 'icn://join?v=1&coop=my-coop&gateway=http://localhost:8080&name=My+Coop';
    const result = parseJoinQR(qr);

    expect(result?.coopId).toBe('my-coop');
    expect(result?.gateway).toBe('http://localhost:8080');
    expect(result?.name).toBe('My Coop');
  });

  it('should return null for missing required fields', () => {
    expect(parseJoinQR('icn://join?v=1&coop=my-coop')).toBeNull(); // missing gateway
    expect(parseJoinQR('icn://join?v=1&gateway=http://example.com')).toBeNull(); // missing coop
  });

  it('should return null for invalid prefix', () => {
    expect(parseJoinQR('https://example.com')).toBeNull();
  });

  it('should roundtrip with generateJoinQR', () => {
    const original = {
      coopId: 'my-coop',
      gateway: 'http://localhost:8080',
      name: 'My Cooperative',
    };
    const qr = generateJoinQR(original);
    const parsed = parseJoinQR(qr);

    expect(parsed).toEqual(original);
  });
});

describe('isJoinQR', () => {
  it('should return true for join URL', () => {
    expect(isJoinQR('icn://join?v=1&coop=test&gateway=http://example.com')).toBe(true);
  });

  it('should return false for other QR types', () => {
    expect(isJoinQR('icn://pay?to=alice')).toBe(false);
    expect(isJoinQR('icn://contact?did=alice')).toBe(false);
    expect(isJoinQR('https://example.com')).toBe(false);
  });
});
