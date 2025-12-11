/**
 * SDIS React Hooks
 *
 * Easy-to-use hooks for SDIS identity operations.
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { ICNMobileClient } from './client';
import {
  ProofType,
  EphemeralProof,
  VerifyResult,
  VerificationLevel,
  GenerateProofRequest,
  SdisHealth,
} from './sdis-types';
import { generateSdisQR, isProofExpired, formatTimeRemaining } from './sdis-qr';

/**
 * Hook for generating and managing SDIS proofs (Wallet mode)
 *
 * @example
 * ```tsx
 * function IdentityScreen() {
 *   const { proof, qrData, isGenerating, error, generate, timeRemaining } = useSdisProof(client);
 *
 *   return (
 *     <View>
 *       {qrData && <QRCode value={qrData} />}
 *       <Text>Expires: {timeRemaining}</Text>
 *       <Button onPress={() => generate({ type: 'age', threshold: 21 })} title="Generate Age Proof" />
 *     </View>
 *   );
 * }
 * ```
 */
export function useSdisProof(client: ICNMobileClient) {
  const [proof, setProof] = useState<EphemeralProof | null>(null);
  const [qrData, setQrData] = useState<string | null>(null);
  const [proofType, setProofType] = useState<ProofType | null>(null);
  const [isGenerating, setIsGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [timeRemaining, setTimeRemaining] = useState<string>('');
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Clear timer on unmount
  useEffect(() => {
    return () => {
      if (timerRef.current) {
        clearInterval(timerRef.current);
      }
    };
  }, []);

  // Update time remaining every second
  useEffect(() => {
    if (!proof) {
      setTimeRemaining('');
      return;
    }

    const updateTime = () => {
      if (isProofExpired(proof.expires_at)) {
        setTimeRemaining('Expired');
        if (timerRef.current) {
          clearInterval(timerRef.current);
          timerRef.current = null;
        }
      } else {
        setTimeRemaining(formatTimeRemaining(proof.expires_at));
      }
    };

    updateTime();
    timerRef.current = setInterval(updateTime, 1000);

    return () => {
      if (timerRef.current) {
        clearInterval(timerRef.current);
        timerRef.current = null;
      }
    };
  }, [proof]);

  const generate = useCallback(
    async (type: ProofType, validitySecs?: number) => {
      setIsGenerating(true);
      setError(null);

      try {
        const request: GenerateProofRequest = {
          proof_type: type,
          validity_secs: validitySecs ?? 3600,
          channels: [],
        };

        const newProof = await client.generateEphemeralProof(request);
        setProof(newProof);
        setProofType(type);
        setQrData(generateSdisQR(newProof));
      } catch (err) {
        setError((err as Error).message);
        setProof(null);
        setQrData(null);
      } finally {
        setIsGenerating(false);
      }
    },
    [client],
  );

  const clear = useCallback(() => {
    setProof(null);
    setQrData(null);
    setProofType(null);
    setError(null);
    setTimeRemaining('');
  }, []);

  const isExpired = proof ? isProofExpired(proof.expires_at) : false;

  return {
    /** Current proof (if generated) */
    proof,
    /** QR code data string */
    qrData,
    /** Current proof type */
    proofType,
    /** Whether proof is being generated */
    isGenerating,
    /** Error message (if any) */
    error,
    /** Time remaining until expiry */
    timeRemaining,
    /** Whether the current proof is expired */
    isExpired,
    /** Generate a new proof */
    generate,
    /** Clear the current proof */
    clear,
  };
}

/**
 * Hook for verifying SDIS proofs (Verifier mode)
 *
 * @example
 * ```tsx
 * function VerifyScreen() {
 *   const { verify, result, isVerifying, error, reset } = useSdisVerifier(client);
 *
 *   const handleScan = (qrData: string) => {
 *     verify(qrData, 1); // Level 1 verification
 *   };
 *
 *   return (
 *     <View>
 *       <Scanner onScan={handleScan} />
 *       {result && (
 *         <Text style={{ color: result.valid ? 'green' : 'red' }}>
 *           {result.valid ? 'Valid' : 'Invalid'}
 *         </Text>
 *       )}
 *     </View>
 *   );
 * }
 * ```
 */
export function useSdisVerifier(client: ICNMobileClient) {
  const [result, setResult] = useState<VerifyResult | null>(null);
  const [isVerifying, setIsVerifying] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const verify = useCallback(
    async (qrData: string, level: VerificationLevel = 1, binding?: string) => {
      setIsVerifying(true);
      setError(null);
      setResult(null);

      try {
        let verifyResult: VerifyResult;

        if (level === 1) {
          verifyResult = await client.verifyLevel1(qrData);
        } else {
          verifyResult = await client.verifyLevel2(qrData, binding);
        }

        setResult(verifyResult);
        return verifyResult;
      } catch (err) {
        setError((err as Error).message);
        return null;
      } finally {
        setIsVerifying(false);
      }
    },
    [client],
  );

  const reset = useCallback(() => {
    setResult(null);
    setError(null);
  }, []);

  return {
    /** Last verification result */
    result,
    /** Whether verification is in progress */
    isVerifying,
    /** Error message (if any) */
    error,
    /** Verify a proof */
    verify,
    /** Reset state */
    reset,
  };
}

/**
 * Hook for checking SDIS service health
 *
 * @example
 * ```tsx
 * function StatusIndicator() {
 *   const { isAvailable, lastChecked, check } = useSdisHealth(client);
 *
 *   return (
 *     <View>
 *       <Text>SDIS: {isAvailable ? 'Online' : 'Offline'}</Text>
 *       <Button onPress={check} title="Refresh" />
 *     </View>
 *   );
 * }
 * ```
 */
export function useSdisHealth(client: ICNMobileClient, autoCheck = true) {
  const [health, setHealth] = useState<SdisHealth | null>(null);
  const [isAvailable, setIsAvailable] = useState<boolean | null>(null);
  const [lastChecked, setLastChecked] = useState<number | null>(null);
  const [isChecking, setIsChecking] = useState(false);

  const check = useCallback(async () => {
    setIsChecking(true);
    try {
      const result = await client.getSdisHealth();
      setHealth(result);
      setIsAvailable(result.status === 'ok');
      setLastChecked(Date.now());
    } catch {
      setIsAvailable(false);
      setLastChecked(Date.now());
    } finally {
      setIsChecking(false);
    }
  }, [client]);

  // Auto-check on mount
  useEffect(() => {
    if (autoCheck) {
      check();
    }
  }, [autoCheck, check]);

  return {
    /** Full health response */
    health,
    /** Whether service is available */
    isAvailable,
    /** Timestamp of last check */
    lastChecked,
    /** Whether check is in progress */
    isChecking,
    /** Manually trigger health check */
    check,
  };
}

/**
 * Hook for managing verification history
 *
 * @example
 * ```tsx
 * function HistoryScreen() {
 *   const { history, addEntry, clearHistory } = useSdisHistory();
 *
 *   return (
 *     <FlatList
 *       data={history}
 *       renderItem={({ item }) => (
 *         <Text>{item.result.valid ? 'Valid' : 'Invalid'} - {item.proofType}</Text>
 *       )}
 *     />
 *   );
 * }
 * ```
 */
export interface HistoryEntry {
  id: string;
  timestamp: number;
  result: VerifyResult;
  proofTypeLabel: string;
  level: VerificationLevel;
}

export function useSdisHistory(maxEntries = 50) {
  const [history, setHistory] = useState<HistoryEntry[]>([]);

  const addEntry = useCallback(
    (result: VerifyResult, proofTypeLabel: string, level: VerificationLevel) => {
      const entry: HistoryEntry = {
        id: `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
        timestamp: Date.now(),
        result,
        proofTypeLabel,
        level,
      };

      setHistory((prev) => {
        const updated = [entry, ...prev];
        return updated.slice(0, maxEntries);
      });

      return entry;
    },
    [maxEntries],
  );

  const removeEntry = useCallback((id: string) => {
    setHistory((prev) => prev.filter((entry) => entry.id !== id));
  }, []);

  const clearHistory = useCallback(() => {
    setHistory([]);
  }, []);

  return {
    /** Verification history (newest first) */
    history,
    /** Add a new entry */
    addEntry,
    /** Remove an entry by ID */
    removeEntry,
    /** Clear all history */
    clearHistory,
  };
}

/**
 * Hook that combines proof generation with history tracking
 */
export function useSdisProofWithHistory(client: ICNMobileClient) {
  const proofHook = useSdisProof(client);
  const [generationHistory, setGenerationHistory] = useState<
    Array<{
      id: string;
      proofType: ProofType;
      generatedAt: number;
      expiresAt: number;
    }>
  >([]);

  const generateWithHistory = useCallback(
    async (type: ProofType, validitySecs?: number) => {
      await proofHook.generate(type, validitySecs);

      if (proofHook.proof) {
        setGenerationHistory((prev) => [
          {
            id: `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
            proofType: type,
            generatedAt: Date.now(),
            expiresAt: proofHook.proof!.expires_at * 1000,
          },
          ...prev.slice(0, 19), // Keep last 20
        ]);
      }
    },
    [proofHook],
  );

  return {
    ...proofHook,
    generateWithHistory,
    generationHistory,
  };
}

/**
 * Hook that combines verification with history tracking
 */
export function useSdisVerifierWithHistory(client: ICNMobileClient) {
  const verifierHook = useSdisVerifier(client);
  const historyHook = useSdisHistory();

  const verifyWithHistory = useCallback(
    async (
      qrData: string,
      level: VerificationLevel = 1,
      proofTypeLabel: string = 'Unknown',
      binding?: string,
    ) => {
      const result = await verifierHook.verify(qrData, level, binding);

      if (result) {
        historyHook.addEntry(result, proofTypeLabel, level);
      }

      return result;
    },
    [verifierHook, historyHook],
  );

  return {
    ...verifierHook,
    verifyWithHistory,
    history: historyHook.history,
    clearHistory: historyHook.clearHistory,
  };
}
