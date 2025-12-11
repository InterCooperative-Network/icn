/**
 * Local Storage for SDIS History
 *
 * Persists verification and proof generation history using AsyncStorage.
 */

import AsyncStorage from '@react-native-async-storage/async-storage';
import {
  VerificationHistoryEntry,
  ProofHistoryEntry,
  VerifyResult,
  ProofType,
  VerificationLevel,
} from '@icn/react-native';

const VERIFICATION_HISTORY_KEY = '@icn/verification_history';
const PROOF_HISTORY_KEY = '@icn/proof_history';
const MAX_HISTORY_ITEMS = 100;

/**
 * Storage manager for SDIS-related history
 */
export class SdisStorage {
  // ============================================================================
  // Verification History
  // ============================================================================

  /**
   * Get all verification history entries
   */
  static async getVerificationHistory(): Promise<VerificationHistoryEntry[]> {
    try {
      const json = await AsyncStorage.getItem(VERIFICATION_HISTORY_KEY);
      if (!json) return [];
      return JSON.parse(json);
    } catch (error) {
      console.error('Failed to load verification history:', error);
      return [];
    }
  }

  /**
   * Add a verification entry to history
   */
  static async addVerificationEntry(
    result: VerifyResult,
    proofType: ProofType,
    level: VerificationLevel,
    notes?: string,
  ): Promise<VerificationHistoryEntry> {
    const entry: VerificationHistoryEntry = {
      id: generateId(),
      result,
      proof_type: proofType,
      timestamp: Date.now(),
      level,
      notes,
    };

    try {
      const history = await this.getVerificationHistory();
      const updated = [entry, ...history].slice(0, MAX_HISTORY_ITEMS);
      await AsyncStorage.setItem(VERIFICATION_HISTORY_KEY, JSON.stringify(updated));
    } catch (error) {
      console.error('Failed to save verification entry:', error);
    }

    return entry;
  }

  /**
   * Remove a verification entry by ID
   */
  static async removeVerificationEntry(id: string): Promise<void> {
    try {
      const history = await this.getVerificationHistory();
      const updated = history.filter((entry) => entry.id !== id);
      await AsyncStorage.setItem(VERIFICATION_HISTORY_KEY, JSON.stringify(updated));
    } catch (error) {
      console.error('Failed to remove verification entry:', error);
    }
  }

  /**
   * Clear all verification history
   */
  static async clearVerificationHistory(): Promise<void> {
    try {
      await AsyncStorage.removeItem(VERIFICATION_HISTORY_KEY);
    } catch (error) {
      console.error('Failed to clear verification history:', error);
    }
  }

  // ============================================================================
  // Proof Generation History
  // ============================================================================

  /**
   * Get all proof generation history entries
   */
  static async getProofHistory(): Promise<ProofHistoryEntry[]> {
    try {
      const json = await AsyncStorage.getItem(PROOF_HISTORY_KEY);
      if (!json) return [];
      return JSON.parse(json);
    } catch (error) {
      console.error('Failed to load proof history:', error);
      return [];
    }
  }

  /**
   * Add a proof generation entry to history
   */
  static async addProofEntry(
    proofType: ProofType,
    expiresAt: number,
  ): Promise<ProofHistoryEntry> {
    const entry: ProofHistoryEntry = {
      id: generateId(),
      proof_type: proofType,
      timestamp: Date.now(),
      expires_at: expiresAt,
    };

    try {
      const history = await this.getProofHistory();
      const updated = [entry, ...history].slice(0, MAX_HISTORY_ITEMS);
      await AsyncStorage.setItem(PROOF_HISTORY_KEY, JSON.stringify(updated));
    } catch (error) {
      console.error('Failed to save proof entry:', error);
    }

    return entry;
  }

  /**
   * Clear all proof generation history
   */
  static async clearProofHistory(): Promise<void> {
    try {
      await AsyncStorage.removeItem(PROOF_HISTORY_KEY);
    } catch (error) {
      console.error('Failed to clear proof history:', error);
    }
  }

  // ============================================================================
  // Statistics
  // ============================================================================

  /**
   * Get verification statistics
   */
  static async getVerificationStats(): Promise<{
    total: number;
    valid: number;
    invalid: number;
    byLevel: Record<number, number>;
  }> {
    const history = await this.getVerificationHistory();

    const stats = {
      total: history.length,
      valid: history.filter((e) => e.result.valid).length,
      invalid: history.filter((e) => !e.result.valid).length,
      byLevel: {} as Record<number, number>,
    };

    for (const entry of history) {
      stats.byLevel[entry.level] = (stats.byLevel[entry.level] || 0) + 1;
    }

    return stats;
  }

  /**
   * Get proof generation statistics
   */
  static async getProofStats(): Promise<{
    total: number;
    byType: Record<string, number>;
  }> {
    const history = await this.getProofHistory();

    const stats = {
      total: history.length,
      byType: {} as Record<string, number>,
    };

    for (const entry of history) {
      const typeKey = entry.proof_type.type;
      stats.byType[typeKey] = (stats.byType[typeKey] || 0) + 1;
    }

    return stats;
  }

  // ============================================================================
  // Cleanup
  // ============================================================================

  /**
   * Clear all SDIS-related storage
   */
  static async clearAll(): Promise<void> {
    await Promise.all([
      this.clearVerificationHistory(),
      this.clearProofHistory(),
    ]);
  }

  /**
   * Remove expired entries from history
   */
  static async cleanupExpired(): Promise<void> {
    // Proof history: remove entries where proof has expired more than 24h ago
    const proofHistory = await this.getProofHistory();
    const oneDayAgo = Date.now() - 24 * 60 * 60 * 1000;
    const validProofHistory = proofHistory.filter(
      (entry) => entry.expires_at * 1000 > oneDayAgo,
    );

    if (validProofHistory.length !== proofHistory.length) {
      await AsyncStorage.setItem(
        PROOF_HISTORY_KEY,
        JSON.stringify(validProofHistory),
      );
    }

    // Verification history: remove entries older than 30 days
    const verificationHistory = await this.getVerificationHistory();
    const thirtyDaysAgo = Date.now() - 30 * 24 * 60 * 60 * 1000;
    const validVerificationHistory = verificationHistory.filter(
      (entry) => entry.timestamp > thirtyDaysAgo,
    );

    if (validVerificationHistory.length !== verificationHistory.length) {
      await AsyncStorage.setItem(
        VERIFICATION_HISTORY_KEY,
        JSON.stringify(validVerificationHistory),
      );
    }
  }
}

/**
 * Generate a unique ID for history entries
 */
function generateId(): string {
  return `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
}

/**
 * React hook for using SDIS storage
 */
export function useSdisStorage() {
  return {
    // Verification
    getVerificationHistory: SdisStorage.getVerificationHistory,
    addVerificationEntry: SdisStorage.addVerificationEntry,
    removeVerificationEntry: SdisStorage.removeVerificationEntry,
    clearVerificationHistory: SdisStorage.clearVerificationHistory,
    getVerificationStats: SdisStorage.getVerificationStats,

    // Proofs
    getProofHistory: SdisStorage.getProofHistory,
    addProofEntry: SdisStorage.addProofEntry,
    clearProofHistory: SdisStorage.clearProofHistory,
    getProofStats: SdisStorage.getProofStats,

    // General
    clearAll: SdisStorage.clearAll,
    cleanupExpired: SdisStorage.cleanupExpired,
  };
}
