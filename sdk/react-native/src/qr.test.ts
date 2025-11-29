/**
 * Tests for QR Code utilities
 */

import { generatePaymentQR, parsePaymentQR, isPaymentQR, generateReceiveQR } from './qr';

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
