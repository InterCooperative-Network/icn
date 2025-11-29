/**
 * Receive Screen
 *
 * Generate QR code for receiving payments.
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
} from 'react-native';
import QRCode from 'react-native-qrcode-svg';
import { useAuth, generateReceiveQR } from '@icn/react-native';
import { client } from '../client';

export function ReceiveScreen() {
  const { did, coopId } = useAuth(client);
  const [amount, setAmount] = useState('');
  const [memo, setMemo] = useState('');

  const qrData = generateReceiveQR(did || '', coopId || '', {
    suggestedAmount: amount ? parseFloat(amount) : undefined,
    memo: memo || undefined,
  });

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

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#f5f5f5',
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
    color: '#666',
    textAlign: 'center',
    marginBottom: 32,
  },
  form: {
    width: '100%',
    marginBottom: 24,
  },
  label: {
    fontSize: 14,
    fontWeight: '600',
    color: '#333',
    marginBottom: 8,
  },
  input: {
    backgroundColor: '#fff',
    borderRadius: 8,
    padding: 16,
    fontSize: 16,
    marginBottom: 16,
    borderWidth: 1,
    borderColor: '#e0e0e0',
  },
  info: {
    backgroundColor: '#fff',
    borderRadius: 12,
    padding: 16,
    width: '100%',
  },
  infoLabel: {
    fontSize: 12,
    color: '#666',
    marginBottom: 4,
  },
  infoValue: {
    fontSize: 14,
    color: '#333',
    fontFamily: Platform.OS === 'ios' ? 'Menlo' : 'monospace',
  },
});
