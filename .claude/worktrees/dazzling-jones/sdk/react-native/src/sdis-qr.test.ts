/**
 * SDIS QR Utilities Tests
 */

import {
  generateSdisQR,
  parseSdisQR,
  isSdisQR,
  isIcnQR,
  getIcnQRType,
  formatTimeRemaining,
  isProofExpired,
  formatVerificationResult,
} from './sdis-qr';
import { EphemeralProof } from './sdis-types';

describe('SDIS QR Utilities', () => {
  // Mock ephemeral proof
  const mockProof: EphemeralProof = {
    qr_data: 'SUM=' + btoa('IC' + String.fromCharCode(1, 21) + 'a'.repeat(131)),
    expires_at: Math.floor(Date.now() / 1000) + 3600,
    session_id: 'session-123',
    qr_estimate: {
      data_size: 137,
      qr_version: 10,
      modules: 57,
    },
  };

  describe('generateSdisQR', () => {
    it('should generate valid SDIS QR string', () => {
      const qrString = generateSdisQR(mockProof);

      expect(qrString).toContain('icn://sdis');
      expect(qrString).toContain('v=1');
      expect(qrString).toContain(`d=${encodeURIComponent(mockProof.qr_data)}`);
    });

    it('should include version parameter', () => {
      const qrString = generateSdisQR(mockProof);
      expect(qrString).toMatch(/v=1/);
    });
  });

  describe('parseSdisQR', () => {
    it('should parse valid SDIS QR string', () => {
      const qrString = generateSdisQR(mockProof);
      const parsed = parseSdisQR(qrString);

      expect(parsed).not.toBeNull();
      expect(parsed?.raw).toBe(mockProof.qr_data);
      expect(parsed?.version).toBe('1');
    });

    it('should return null for non-SDIS QR', () => {
      const parsed = parseSdisQR('icn://pay?v=1&to=did:icn:abc');
      expect(parsed).toBeNull();
    });

    it('should return null for invalid URL', () => {
      const parsed = parseSdisQR('not-a-valid-url');
      expect(parsed).toBeNull();
    });

    it('should return null for missing data parameter', () => {
      const parsed = parseSdisQR('icn://sdis?v=1');
      expect(parsed).toBeNull();
    });
  });

  describe('isSdisQR', () => {
    it('should return true for SDIS QR strings', () => {
      expect(isSdisQR('icn://sdis?v=1&d=abc')).toBe(true);
    });

    it('should return false for payment QR strings', () => {
      expect(isSdisQR('icn://pay?v=1&to=did:icn:abc')).toBe(false);
    });

    it('should return false for non-ICN strings', () => {
      expect(isSdisQR('https://example.com')).toBe(false);
    });
  });

  describe('isIcnQR', () => {
    it('should return true for ICN QR strings', () => {
      expect(isIcnQR('icn://sdis?v=1')).toBe(true);
      expect(isIcnQR('icn://pay?v=1')).toBe(true);
    });

    it('should return false for non-ICN strings', () => {
      expect(isIcnQR('https://example.com')).toBe(false);
    });
  });

  describe('getIcnQRType', () => {
    it('should identify SDIS QR type', () => {
      expect(getIcnQRType('icn://sdis?v=1&d=abc')).toBe('sdis');
    });

    it('should identify payment QR type', () => {
      expect(getIcnQRType('icn://pay?v=1&to=did')).toBe('payment');
    });

    it('should return unknown for other ICN URLs', () => {
      expect(getIcnQRType('icn://other?v=1')).toBe('unknown');
    });

    it('should return unknown for non-ICN URLs', () => {
      expect(getIcnQRType('https://example.com')).toBe('unknown');
    });
  });

  describe('formatTimeRemaining', () => {
    it('should format time with hours', () => {
      const future = Math.floor(Date.now() / 1000) + 3700; // ~1h 1m
      const result = formatTimeRemaining(future);
      expect(result).toMatch(/1h \d+m/);
    });

    it('should format time with minutes and seconds', () => {
      const future = Math.floor(Date.now() / 1000) + 125; // 2m 5s
      const result = formatTimeRemaining(future);
      expect(result).toMatch(/2:\d{2}/);
    });

    it('should format time with seconds only', () => {
      const future = Math.floor(Date.now() / 1000) + 30;
      const result = formatTimeRemaining(future);
      expect(result).toMatch(/\d+s/);
    });

    it('should return Expired for past times', () => {
      const past = Math.floor(Date.now() / 1000) - 100;
      const result = formatTimeRemaining(past);
      expect(result).toBe('Expired');
    });
  });

  describe('isProofExpired', () => {
    it('should return false for future expiry', () => {
      const future = Math.floor(Date.now() / 1000) + 3600;
      expect(isProofExpired(future)).toBe(false);
    });

    it('should return true for past expiry', () => {
      const past = Math.floor(Date.now() / 1000) - 100;
      expect(isProofExpired(past)).toBe(true);
    });
  });

  describe('formatVerificationResult', () => {
    it('should format valid result', () => {
      const result = formatVerificationResult(true, 1, 'Age 21+');
      expect(result).toContain('Valid');
      expect(result).toContain('Level 1');
      expect(result).toContain('Age 21+');
    });

    it('should format invalid result', () => {
      const result = formatVerificationResult(false, 2, 'Citizenship');
      expect(result).toContain('Invalid');
      expect(result).toContain('Level 2');
    });

    it('should handle missing proof type', () => {
      const result = formatVerificationResult(true, 1);
      expect(result).toContain('Unknown');
    });
  });
});
