/**
 * Device Management Hooks for ICN Mobile SDK
 *
 * Enable multi-device support for mobile apps.
 * Allows users to register, list, and revoke devices associated with their DID.
 */

import { useState, useEffect, useCallback } from 'react';
import { ICNMobileClient } from './client';
import { ICNWallet } from './types';

// ============================================================================
// Encoding Utilities (React Native compatible)
// ============================================================================

function stringToHex(str: string): string {
  const encoder = new TextEncoder();
  const bytes = encoder.encode(str);
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}

// ============================================================================
// Types
// ============================================================================

/**
 * Capability that can be granted to a device
 */
export type DeviceCapability =
  | 'sign'
  | 'add_device'
  | 'revoke_device'
  | 'rotate_key'
  | 'recover'
  | 'encrypt';

/**
 * Device information
 */
export interface DeviceInfo {
  /** Unique device identifier */
  id: string;
  /** Human-readable label */
  label: string;
  /** Key type (e.g., "Ed25519") */
  keyType: string;
  /** Capabilities granted to this device */
  capabilities: string[];
  /** Unix timestamp when device was added */
  addedAt: number;
  /** Whether the device has been revoked */
  revoked: boolean;
}

/**
 * Request to register a new device
 */
export interface RegisterDeviceRequest {
  /** Unique device identifier (e.g., "phone-1", "laptop-work") */
  deviceId: string;
  /** Human-readable label */
  label: string;
  /** Ed25519 public key (hex-encoded) */
  publicKey: string;
  /** Optional X25519 encryption public key (hex-encoded) */
  encryptionPublicKey?: string;
  /** Capabilities to grant */
  capabilities: DeviceCapability[];
}

/**
 * Response from device registration
 */
export interface RegisterDeviceResponse {
  device: DeviceInfo;
  message: string;
}

/**
 * Response from listing devices
 */
export interface ListDevicesResponse {
  devices: DeviceInfo[];
  total: number;
}

// ============================================================================
// API Client Extension
// ============================================================================

/**
 * Device management API methods
 */
export interface DeviceAPI {
  /**
   * Register a new device for the authenticated user's DID
   *
   * @param request - Device registration details
   * @param signingDeviceId - ID of the device signing this request (must have AddDevice capability)
   * @returns The registered device info
   */
  registerDevice(
    request: RegisterDeviceRequest,
    signingDeviceId: string
  ): Promise<RegisterDeviceResponse>;

  /**
   * List all devices for the authenticated user's DID
   *
   * @returns List of devices
   */
  listDevices(): Promise<ListDevicesResponse>;

  /**
   * Get a specific device by ID
   *
   * @param deviceId - Device ID to get
   * @returns Device info
   */
  getDevice(deviceId: string): Promise<DeviceInfo>;

  /**
   * Revoke a device
   *
   * @param deviceId - Device ID to revoke
   * @param signingDeviceId - ID of the device signing this request (must have RevokeDevice capability)
   */
  revokeDevice(deviceId: string, signingDeviceId: string): Promise<void>;
}

/**
 * Create device API methods for a client
 */
export function createDeviceAPI(
  client: ICNMobileClient,
  wallet: ICNWallet
): DeviceAPI {
  const baseUrl = (client as unknown as { baseUrl: string }).baseUrl;

  // Get current DID from auth state
  const getDid = () => {
    const did = client.authState.did;
    if (!did) {
      throw new Error('Not authenticated. Login first.');
    }
    return did;
  };

  // Get auth headers
  const getAuthHeaders = () => {
    const token = (client as unknown as { authToken?: string }).authToken;
    if (!token) {
      throw new Error('Not authenticated. Login first.');
    }
    return {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${token}`,
    };
  };

  return {
    async registerDevice(
      request: RegisterDeviceRequest,
      signingDeviceId: string
    ): Promise<RegisterDeviceResponse> {
      const did = getDid();

      // Create message to sign: ICN_ADD_DEVICE:{did}:{deviceId}:{label}
      const message = `ICN_ADD_DEVICE:${did}:${request.deviceId}:${request.label}`;
      const messageHex = stringToHex(message);

      // Sign with wallet
      const signature = await wallet.sign(messageHex);

      const response = await fetch(`${baseUrl}/v1/devices/${encodeURIComponent(did)}`, {
        method: 'POST',
        headers: getAuthHeaders(),
        body: JSON.stringify({
          device_id: request.deviceId,
          label: request.label,
          public_key: request.publicKey,
          encryption_public_key: request.encryptionPublicKey,
          capabilities: request.capabilities,
          signing_device_id: signingDeviceId,
          signature,
        }),
      });

      if (!response.ok) {
        const error = await response.text();
        throw new Error(`Failed to register device: ${error}`);
      }

      return response.json();
    },

    async listDevices(): Promise<ListDevicesResponse> {
      const did = getDid();

      const response = await fetch(`${baseUrl}/v1/devices/${encodeURIComponent(did)}`, {
        method: 'GET',
        headers: getAuthHeaders(),
      });

      if (!response.ok) {
        const error = await response.text();
        throw new Error(`Failed to list devices: ${error}`);
      }

      return response.json();
    },

    async getDevice(deviceId: string): Promise<DeviceInfo> {
      const did = getDid();

      const response = await fetch(
        `${baseUrl}/v1/devices/${encodeURIComponent(did)}/${encodeURIComponent(deviceId)}`,
        {
          method: 'GET',
          headers: getAuthHeaders(),
        }
      );

      if (!response.ok) {
        const error = await response.text();
        throw new Error(`Failed to get device: ${error}`);
      }

      return response.json();
    },

    async revokeDevice(deviceId: string, signingDeviceId: string): Promise<void> {
      const did = getDid();

      // Create message to sign: ICN_REVOKE_DEVICE:{did}:{deviceId}
      const message = `ICN_REVOKE_DEVICE:${did}:${deviceId}`;
      const messageHex = stringToHex(message);

      // Sign with wallet
      const signature = await wallet.sign(messageHex);

      const response = await fetch(
        `${baseUrl}/v1/devices/${encodeURIComponent(did)}/${encodeURIComponent(deviceId)}`,
        {
          method: 'DELETE',
          headers: getAuthHeaders(),
          body: JSON.stringify({
            signing_device_id: signingDeviceId,
            signature,
          }),
        }
      );

      if (!response.ok) {
        const error = await response.text();
        throw new Error(`Failed to revoke device: ${error}`);
      }
    },
  };
}

// ============================================================================
// Hooks
// ============================================================================

/**
 * Hook for managing devices associated with the authenticated user's DID
 *
 * @example
 * ```tsx
 * function DevicesScreen() {
 *   const {
 *     devices,
 *     isLoading,
 *     error,
 *     registerDevice,
 *     revokeDevice,
 *     refresh,
 *   } = useDevices(client, wallet, 'device-1');
 *
 *   const handleAddDevice = async () => {
 *     const keyPair = await wallet.generateKeyPair();
 *     await registerDevice({
 *       deviceId: 'phone-2',
 *       label: 'Work Phone',
 *       publicKey: keyPair.publicKey,
 *       capabilities: ['sign'],
 *     });
 *   };
 *
 *   return (
 *     <View>
 *       <FlatList
 *         data={devices}
 *         renderItem={({ item }) => (
 *           <View>
 *             <Text>{item.label}</Text>
 *             <Button onPress={() => revokeDevice(item.id)} title="Revoke" />
 *           </View>
 *         )}
 *       />
 *       <Button onPress={handleAddDevice} title="Add Device" />
 *     </View>
 *   );
 * }
 * ```
 */
export function useDevices(
  client: ICNMobileClient,
  wallet: ICNWallet,
  currentDeviceId: string
) {
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const api = createDeviceAPI(client, wallet);

  const refresh = useCallback(async () => {
    if (!client.authState.isAuthenticated) {
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const result = await api.listDevices();
      setDevices(result.devices);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [client.authState.isAuthenticated, api]);

  // Load devices when authenticated
  useEffect(() => {
    if (client.authState.isAuthenticated) {
      refresh();
    }
  }, [client.authState.isAuthenticated, refresh]);

  const registerDevice = useCallback(
    async (request: RegisterDeviceRequest) => {
      setIsLoading(true);
      setError(null);

      try {
        const result = await api.registerDevice(request, currentDeviceId);
        await refresh();
        return result;
      } catch (err) {
        setError((err as Error).message);
        throw err;
      } finally {
        setIsLoading(false);
      }
    },
    [api, currentDeviceId, refresh]
  );

  const revokeDevice = useCallback(
    async (deviceId: string) => {
      setIsLoading(true);
      setError(null);

      try {
        await api.revokeDevice(deviceId, currentDeviceId);
        await refresh();
      } catch (err) {
        setError((err as Error).message);
        throw err;
      } finally {
        setIsLoading(false);
      }
    },
    [api, currentDeviceId, refresh]
  );

  const getDevice = useCallback(
    async (deviceId: string) => {
      return api.getDevice(deviceId);
    },
    [api]
  );

  return {
    devices,
    isLoading,
    error,
    registerDevice,
    revokeDevice,
    getDevice,
    refresh,
    currentDeviceId,
  };
}

/**
 * Hook for signing operations with the current device
 *
 * Provides helpers for signing messages that can be verified by the gateway.
 *
 * @example
 * ```tsx
 * function TransactionScreen() {
 *   const { signMessage, signTransaction, deviceId } = useDeviceSigning(wallet, 'device-1');
 *
 *   const handlePayment = async () => {
 *     const signature = await signTransaction({
 *       type: 'payment',
 *       from: myDid,
 *       to: recipientDid,
 *       amount: 100,
 *     });
 *     // Send to gateway with signature
 *   };
 * }
 * ```
 */
export function useDeviceSigning(wallet: ICNWallet, deviceId: string) {
  const [isSigning, setIsSigning] = useState(false);

  /**
   * Sign an arbitrary message
   *
   * @param message - Message string to sign
   * @returns Hex-encoded signature
   */
  const signMessage = useCallback(
    async (message: string): Promise<string> => {
      setIsSigning(true);
      try {
        // Convert message to hex for signing
        const messageHex = stringToHex(message);
        return await wallet.sign(messageHex);
      } finally {
        setIsSigning(false);
      }
    },
    [wallet]
  );

  /**
   * Sign a transaction payload
   *
   * Serializes the transaction to JSON and signs it.
   *
   * @param transaction - Transaction object to sign
   * @returns Hex-encoded signature
   */
  const signTransaction = useCallback(
    async (transaction: Record<string, unknown>): Promise<string> => {
      setIsSigning(true);
      try {
        // Serialize transaction to JSON and sign
        const messageJson = JSON.stringify(transaction);
        const messageHex = stringToHex(messageJson);
        return await wallet.sign(messageHex);
      } finally {
        setIsSigning(false);
      }
    },
    [wallet]
  );

  /**
   * Create a signed request that includes device ID and signature
   *
   * @param payload - The payload to sign
   * @returns Payload with signature and device info
   */
  const createSignedRequest = useCallback(
    async <T extends Record<string, unknown>>(
      payload: T
    ): Promise<T & { device_id: string; signature: string }> => {
      const signature = await signTransaction(payload);
      return {
        ...payload,
        device_id: deviceId,
        signature,
      };
    },
    [deviceId, signTransaction]
  );

  return {
    deviceId,
    isSigning,
    signMessage,
    signTransaction,
    createSignedRequest,
  };
}
