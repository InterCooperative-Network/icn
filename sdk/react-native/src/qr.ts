/**
 * QR Code utilities for ICN
 *
 * Generate and parse QR codes for payments, contacts, and coop joining.
 *
 * QR Code Formats:
 * - Payment: icn://pay?v=1&to=DID&amount=N&coop=ID&memo=TEXT
 * - Contact: icn://contact?v=1&did=DID&name=NAME&coop=ID
 * - Join:    icn://join?v=1&coop=ID&gateway=URL&name=NAME
 */

import { PaymentQRData } from './types';

// QR URL prefixes
const QR_PREFIX_PAY = 'icn://pay';
const QR_PREFIX_CONTACT = 'icn://contact';
const QR_PREFIX_JOIN = 'icn://join';
const QR_VERSION = '1';

/** Type of QR code */
export type QRType = 'payment' | 'contact' | 'join' | 'unknown';

/** Data from a contact QR code */
export interface ContactQRData {
  did: string;
  name?: string;
  coopId?: string;
}

/** Data from a join/coop QR code */
export interface JoinQRData {
  coopId: string;
  gateway: string;
  name?: string;
}

/** Union type for all QR data */
export type ParsedQRData =
  | { type: 'payment'; data: PaymentQRData }
  | { type: 'contact'; data: ContactQRData }
  | { type: 'join'; data: JoinQRData }
  | { type: 'unknown'; data: null };

/**
 * Detect the type of QR code from its data string
 */
export function detectQRType(qrData: string): QRType {
  if (qrData.startsWith(QR_PREFIX_PAY)) return 'payment';
  if (qrData.startsWith(QR_PREFIX_CONTACT)) return 'contact';
  if (qrData.startsWith(QR_PREFIX_JOIN)) return 'join';
  // Also check for raw DIDs
  if (qrData.startsWith('did:icn:')) return 'contact';
  return 'unknown';
}

/**
 * Parse any ICN QR code and return typed data
 *
 * @example
 * ```typescript
 * const result = parseQR(scannedData);
 * switch (result.type) {
 *   case 'payment':
 *     navigation.navigate('Payment', result.data);
 *     break;
 *   case 'contact':
 *     addContact(result.data);
 *     break;
 *   case 'join':
 *     joinCoop(result.data);
 *     break;
 * }
 * ```
 */
export function parseQR(qrData: string): ParsedQRData {
  const type = detectQRType(qrData);

  switch (type) {
    case 'payment': {
      const data = parsePaymentQR(qrData);
      return data ? { type: 'payment', data } : { type: 'unknown', data: null };
    }
    case 'contact': {
      const data = parseContactQR(qrData);
      return data ? { type: 'contact', data } : { type: 'unknown', data: null };
    }
    case 'join': {
      const data = parseJoinQR(qrData);
      return data ? { type: 'join', data } : { type: 'unknown', data: null };
    }
    default:
      return { type: 'unknown', data: null };
  }
}

// Legacy constant for backwards compatibility
const QR_PREFIX = QR_PREFIX_PAY;

/**
 * Generate QR code data string for a payment request
 *
 * @example
 * ```typescript
 * import { generatePaymentQR } from '@icn/react-native';
 *
 * const qrData = generatePaymentQR({
 *   to: 'did:icn:alice',
 *   amount: 5,
 *   memo: 'Coffee',
 *   coopId: 'my-timebank',
 * });
 *
 * // Use with a QR code library
 * <QRCode value={qrData} />
 * ```
 */
export function generatePaymentQR(data: PaymentQRData): string {
  const params = new URLSearchParams();
  params.set('v', QR_VERSION);
  params.set('to', data.to);
  params.set('amount', data.amount.toString());
  params.set('coop', data.coopId);

  if (data.memo) {
    params.set('memo', data.memo);
  }

  return `${QR_PREFIX}?${params.toString()}`;
}

/**
 * Parse QR code data string into payment data
 *
 * @example
 * ```typescript
 * import { parsePaymentQR } from '@icn/react-native';
 *
 * // From QR scanner
 * const scannedData = 'icn://pay?v=1&to=did:icn:alice&amount=5&coop=my-timebank';
 * const payment = parsePaymentQR(scannedData);
 *
 * if (payment) {
 *   await client.pay(payment.coopId, {
 *     to: payment.to,
 *     amount: payment.amount,
 *     memo: payment.memo,
 *   });
 * }
 * ```
 */
export function parsePaymentQR(qrData: string): PaymentQRData | null {
  try {
    // Check prefix
    if (!qrData.startsWith(QR_PREFIX)) {
      return null;
    }

    // Parse URL parameters
    const url = new URL(qrData);
    const params = url.searchParams;

    // Validate version
    const version = params.get('v');
    if (version !== QR_VERSION) {
      console.warn(`Unknown QR version: ${version}`);
    }

    // Extract required fields
    const to = params.get('to');
    const amountStr = params.get('amount');
    const coopId = params.get('coop');

    if (!to || !amountStr || !coopId) {
      return null;
    }

    const amount = parseFloat(amountStr);
    if (isNaN(amount) || amount < 0) {
      return null;
    }

    // Extract optional fields
    const memo = params.get('memo') ?? undefined;

    return {
      to,
      amount,
      coopId,
      memo,
    };
  } catch (error) {
    console.warn('Failed to parse QR code:', error);
    return null;
  }
}

/**
 * Validate if a string is a valid ICN payment QR code
 */
export function isPaymentQR(qrData: string): boolean {
  return qrData.startsWith(QR_PREFIX);
}

/**
 * Generate a receive QR code for your own DID
 *
 * @example
 * ```typescript
 * const receiveQR = generateReceiveQR('did:icn:bob', 'my-timebank', {
 *   suggestedAmount: 10,
 *   memo: 'For tutoring session',
 * });
 * ```
 */
export function generateReceiveQR(
  did: string,
  coopId: string,
  options?: {
    suggestedAmount?: number;
    memo?: string;
  }
): string {
  return generatePaymentQR({
    to: did,
    amount: options?.suggestedAmount || 0,
    coopId,
    memo: options?.memo,
  });
}

// ============================================
// Contact QR Code Functions
// ============================================

/**
 * Generate QR code data for sharing your contact info
 *
 * @example
 * ```typescript
 * const contactQR = generateContactQR({
 *   did: 'did:icn:alice',
 *   name: 'Alice',
 *   coopId: 'my-timebank',
 * });
 * ```
 */
export function generateContactQR(data: ContactQRData): string {
  const params = new URLSearchParams();
  params.set('v', QR_VERSION);
  params.set('did', data.did);

  if (data.name) {
    params.set('name', data.name);
  }
  if (data.coopId) {
    params.set('coop', data.coopId);
  }

  return `${QR_PREFIX_CONTACT}?${params.toString()}`;
}

/**
 * Parse a contact QR code
 */
export function parseContactQR(qrData: string): ContactQRData | null {
  try {
    // Handle raw DID (just "did:icn:...")
    if (qrData.startsWith('did:icn:') && !qrData.includes('?')) {
      return { did: qrData };
    }

    // Handle full contact URL
    if (!qrData.startsWith(QR_PREFIX_CONTACT)) {
      return null;
    }

    const url = new URL(qrData);
    const params = url.searchParams;

    const did = params.get('did');
    if (!did) {
      return null;
    }

    return {
      did,
      name: params.get('name') ?? undefined,
      coopId: params.get('coop') ?? undefined,
    };
  } catch (error) {
    console.warn('Failed to parse contact QR:', error);
    return null;
  }
}

/**
 * Check if a string is a contact QR code
 */
export function isContactQR(qrData: string): boolean {
  return qrData.startsWith(QR_PREFIX_CONTACT) || qrData.startsWith('did:icn:');
}

// ============================================
// Join/Coop QR Code Functions
// ============================================

/**
 * Generate QR code for joining a cooperative
 *
 * @example
 * ```typescript
 * // Steward generates this for new members to scan
 * const joinQR = generateJoinQR({
 *   coopId: 'my-timebank',
 *   gateway: 'https://api.mycoop.org',
 *   name: 'My Timebank Coop',
 * });
 * ```
 */
export function generateJoinQR(data: JoinQRData): string {
  const params = new URLSearchParams();
  params.set('v', QR_VERSION);
  params.set('coop', data.coopId);
  params.set('gateway', data.gateway);

  if (data.name) {
    params.set('name', data.name);
  }

  return `${QR_PREFIX_JOIN}?${params.toString()}`;
}

/**
 * Parse a join/coop QR code
 */
export function parseJoinQR(qrData: string): JoinQRData | null {
  try {
    if (!qrData.startsWith(QR_PREFIX_JOIN)) {
      return null;
    }

    const url = new URL(qrData);
    const params = url.searchParams;

    const coopId = params.get('coop');
    const gateway = params.get('gateway');

    if (!coopId || !gateway) {
      return null;
    }

    return {
      coopId,
      gateway,
      name: params.get('name') ?? undefined,
    };
  } catch (error) {
    console.warn('Failed to parse join QR:', error);
    return null;
  }
}

/**
 * Check if a string is a join QR code
 */
export function isJoinQR(qrData: string): boolean {
  return qrData.startsWith(QR_PREFIX_JOIN);
}

/**
 * Check if a string is any valid ICN QR code
 */
export function isICNQR(qrData: string): boolean {
  return isPaymentQR(qrData) || isContactQR(qrData) || isJoinQR(qrData);
}
