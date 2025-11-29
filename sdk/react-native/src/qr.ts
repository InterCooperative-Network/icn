/**
 * QR Code utilities for ICN payments
 *
 * Generate and parse QR codes for scan-to-pay functionality.
 */

import { PaymentQRData } from './types';

const QR_PREFIX = 'icn://pay';
const QR_VERSION = '1';

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
    if (isNaN(amount) || amount <= 0) {
      return null;
    }

    // Extract optional fields
    const memo = params.get('memo') || undefined;

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
