/**
 * SDIS Types Tests
 */

import {
  ageProof,
  citizenshipProof,
  membershipProof,
  nonRevocationProof,
  formatProofType,
  PROOF_PRESETS,
  ProofType,
} from './sdis-types';

describe('SDIS Types', () => {
  describe('Proof Type Helpers', () => {
    describe('ageProof', () => {
      it('should create age proof type with threshold', () => {
        const proof = ageProof(21);
        expect(proof.type).toBe('age');
        expect(proof.threshold).toBe(21);
      });

      it('should work with different thresholds', () => {
        expect(ageProof(18).threshold).toBe(18);
        expect(ageProof(65).threshold).toBe(65);
      });
    });

    describe('citizenshipProof', () => {
      it('should create citizenship proof type', () => {
        const proof = citizenshipProof('US');
        expect(proof.type).toBe('citizenship');
        expect(proof.country_code).toBe('US');
      });

      it('should work with different country codes', () => {
        expect(citizenshipProof('CA').country_code).toBe('CA');
        expect(citizenshipProof('GB').country_code).toBe('GB');
      });
    });

    describe('membershipProof', () => {
      it('should create membership proof type', () => {
        const proof = membershipProof('did:icn:org123');
        expect(proof.type).toBe('membership');
        expect(proof.org_did).toBe('did:icn:org123');
      });
    });

    describe('nonRevocationProof', () => {
      it('should create non-revocation proof type', () => {
        const proof = nonRevocationProof();
        expect(proof.type).toBe('non_revocation');
      });
    });
  });

  describe('formatProofType', () => {
    it('should format age proof', () => {
      expect(formatProofType({ type: 'age', threshold: 21 })).toBe('Age 21+');
      expect(formatProofType({ type: 'age', threshold: 18 })).toBe('Age 18+');
    });

    it('should format citizenship proof', () => {
      expect(formatProofType({ type: 'citizenship', country_code: 'US' })).toBe(
        'Citizenship: US',
      );
    });

    it('should format membership proof', () => {
      expect(formatProofType({ type: 'membership' })).toBe('Membership');
    });

    it('should format non-revocation proof', () => {
      expect(formatProofType({ type: 'non_revocation' })).toBe('Not Revoked');
    });

    it('should format custom proof', () => {
      expect(formatProofType({ type: 'custom' })).toBe('Custom Proof');
    });

    it('should handle unknown types', () => {
      const unknownType = { type: 'unknown' } as unknown as ProofType;
      expect(formatProofType(unknownType)).toBe('Unknown');
    });
  });

  describe('PROOF_PRESETS', () => {
    it('should have AGE_18 preset', () => {
      expect(PROOF_PRESETS.AGE_18.type).toBe('age');
      expect(PROOF_PRESETS.AGE_18.threshold).toBe(18);
    });

    it('should have AGE_21 preset', () => {
      expect(PROOF_PRESETS.AGE_21.type).toBe('age');
      expect(PROOF_PRESETS.AGE_21.threshold).toBe(21);
    });

    it('should have citizenship presets', () => {
      expect(PROOF_PRESETS.CITIZENSHIP_US.type).toBe('citizenship');
      expect(PROOF_PRESETS.CITIZENSHIP_US.country_code).toBe('US');

      expect(PROOF_PRESETS.CITIZENSHIP_CA.country_code).toBe('CA');
      expect(PROOF_PRESETS.CITIZENSHIP_UK.country_code).toBe('GB');
    });

    it('should have NON_REVOCATION preset', () => {
      expect(PROOF_PRESETS.NON_REVOCATION.type).toBe('non_revocation');
    });
  });

  describe('Type Definitions', () => {
    it('should allow valid proof types', () => {
      const validTypes: ProofType[] = [
        { type: 'age', threshold: 21 },
        { type: 'citizenship', country_code: 'US' },
        { type: 'membership', org_did: 'did:icn:org' },
        { type: 'non_revocation' },
        { type: 'custom', circuit_id: 'custom-circuit-id' },
      ];

      // Type checking - this should compile without errors
      validTypes.forEach((proof) => {
        expect(proof.type).toBeDefined();
      });
    });
  });
});
