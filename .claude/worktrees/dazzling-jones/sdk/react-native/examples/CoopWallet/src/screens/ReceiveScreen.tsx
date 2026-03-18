/**
 * Receive Screen
 *
 * Generate QR code for receiving payments with share functionality.
 */

import React, { useState } from 'react';
import {
  View,
  Text,
  TextInput,
  StyleSheet,
  KeyboardAvoidingView,
  Platform,
  ScrollView,
  TouchableOpacity,
  Share,
  Alert,
  Clipboard,
} from 'react-native';
import QRCode from 'react-native-qrcode-svg';
import { useAuth, generateReceiveQR } from '@icn/react-native';
import { client } from '../client';
import { useTheme, Theme } from '../contexts/ThemeContext';

export function ReceiveScreen() {
  const { theme } = useTheme();
  const styles = createStyles(theme);
  const { did, coopId } = useAuth(client!);
  const [amount, setAmount] = useState('');
  const [memo, setMemo] = useState('');
  const [copied, setCopied] = useState(false);

  const qrData = generateReceiveQR(did || '', coopId || '', {
    suggestedAmount: amount ? parseFloat(amount) : undefined,
    memo: memo || undefined,
  });

  // Generate a shareable payment link
  const generatePaymentLink = () => {
    const params = new URLSearchParams();
    params.set('to', did || '');
    params.set('coop', coopId || '');
    if (amount) params.set('amount', amount);
    if (memo) params.set('memo', memo);
    return `icn://pay?${params.toString()}`;
  };

  // Generate human-readable request text
  const generateRequestText = () => {
    let text = `Payment request from ${did?.slice(0, 20)}...`;
    if (amount) text += `\nAmount: ${amount} hours`;
    if (memo) text += `\nFor: ${memo}`;
    text += `\n\nOpen in CoopWallet: ${generatePaymentLink()}`;
    return text;
  };

  const handleShare = async () => {
    try {
      const result = await Share.share({
        message: generateRequestText(),
        title: 'Payment Request',
      });

      if (result.action === Share.sharedAction) {
        // Shared successfully
      }
    } catch (error) {
      Alert.alert('Error', 'Failed to share payment request');
    }
  };

  const handleCopyLink = async () => {
    const link = generatePaymentLink();
    if (Platform.OS === 'web') {
      try {
        await navigator.clipboard.writeText(link);
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
      } catch {
        alert('Failed to copy');
      }
    } else {
      Clipboard.setString(link);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  };

  return (
    <KeyboardAvoidingView
      style={styles.container}
      behavior={Platform.OS === 'ios' ? 'padding' : 'height'}
    >
      <ScrollView contentContainerStyle={styles.content}>
        <View style={styles.qrContainer}>
          <QRCode
            value={qrData}
            size={250}
            backgroundColor="#fff"
            color="#333"
          />
        </View>

        <Text style={styles.instruction}>
          Show this QR code to receive a payment
        </Text>

        {/* Share Actions */}
        <View style={styles.shareActions}>
          <TouchableOpacity style={styles.shareButton} onPress={handleShare}>
            <Text style={styles.shareIcon}>📤</Text>
            <Text style={styles.shareButtonText}>Share Request</Text>
          </TouchableOpacity>

          <TouchableOpacity style={styles.copyButton} onPress={handleCopyLink}>
            <Text style={styles.shareIcon}>{copied ? '✓' : '📋'}</Text>
            <Text style={styles.copyButtonText}>
              {copied ? 'Copied!' : 'Copy Link'}
            </Text>
          </TouchableOpacity>
        </View>

        <View style={styles.form}>
          <Text style={styles.label}>Suggested Amount (optional)</Text>
          <TextInput
            style={styles.input}
            placeholder="0.0"
            value={amount}
            onChangeText={setAmount}
            keyboardType="decimal-pad"
          />

          <Text style={styles.label}>Memo (optional)</Text>
          <TextInput
            style={styles.input}
            placeholder="e.g., For tutoring session"
            value={memo}
            onChangeText={setMemo}
          />
        </View>

        <View style={styles.info}>
          <Text style={styles.infoLabel}>Your DID</Text>
          <Text style={styles.infoValue} numberOfLines={1} ellipsizeMode="middle">
            {did}
          </Text>
        </View>
      </ScrollView>
    </KeyboardAvoidingView>
  );
}

const createStyles = (theme: Theme) => StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: theme.colors.background,
  },
  content: {
    padding: 24,
    alignItems: 'center',
  },
  qrContainer: {
    backgroundColor: '#fff',
    padding: 24,
    borderRadius: 16,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 2 },
    shadowOpacity: 0.1,
    shadowRadius: 8,
    elevation: 4,
    marginBottom: 24,
  },
  instruction: {
    fontSize: 16,
    color: theme.colors.textSecondary,
    textAlign: 'center',
    marginBottom: 16,
  },
  shareActions: {
    flexDirection: 'row',
    gap: 12,
    marginBottom: 24,
  },
  shareButton: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: theme.colors.primary,
    paddingVertical: 14,
    paddingHorizontal: 16,
    borderRadius: 10,
    gap: 8,
  },
  copyButton: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: theme.colors.card,
    paddingVertical: 14,
    paddingHorizontal: 16,
    borderRadius: 10,
    borderWidth: 1,
    borderColor: theme.colors.primary,
    gap: 8,
  },
  shareIcon: {
    fontSize: 16,
  },
  shareButtonText: {
    color: '#fff',
    fontSize: 14,
    fontWeight: '600',
  },
  copyButtonText: {
    color: theme.colors.primary,
    fontSize: 14,
    fontWeight: '600',
  },
  form: {
    width: '100%',
    marginBottom: 24,
  },
  label: {
    fontSize: 14,
    fontWeight: '600',
    color: theme.colors.text,
    marginBottom: 8,
  },
  input: {
    backgroundColor: theme.colors.card,
    borderRadius: 8,
    padding: 16,
    fontSize: 16,
    marginBottom: 16,
    borderWidth: 1,
    borderColor: theme.colors.border,
    color: theme.colors.text,
  },
  info: {
    backgroundColor: theme.colors.card,
    borderRadius: 12,
    padding: 16,
    width: '100%',
  },
  infoLabel: {
    fontSize: 12,
    color: theme.colors.textSecondary,
    marginBottom: 4,
  },
  infoValue: {
    fontSize: 14,
    color: theme.colors.text,
    fontFamily: Platform.OS === 'ios' ? 'Menlo' : 'monospace',
  },
});
