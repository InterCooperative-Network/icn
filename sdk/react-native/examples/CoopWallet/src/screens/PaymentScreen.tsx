/**
 * Payment Screen
 *
 * Send hours to another member.
 */

import React, { useState } from 'react';
import {
  View,
  Text,
  TextInput,
  TouchableOpacity,
  StyleSheet,
  Alert,
  ActivityIndicator,
  KeyboardAvoidingView,
  Platform,
} from 'react-native';
import { NativeStackNavigationProp } from '@react-navigation/native-stack';
import { RouteProp } from '@react-navigation/native';
import { useAuth, usePayment } from '@icn/react-native';
import { client } from '../client';
import { RootStackParamList } from '../../App';

type Props = {
  navigation: NativeStackNavigationProp<RootStackParamList, 'Payment'>;
  route: RouteProp<RootStackParamList, 'Payment'>;
};

export function PaymentScreen({ navigation, route }: Props) {
  const { coopId } = useAuth(client!);
  const { pay, isPaying, error } = usePayment(client!, coopId || '');

  const [recipient, setRecipient] = useState(route.params?.to || '');
  const [amount, setAmount] = useState(route.params?.amount?.toString() || '');
  const [memo, setMemo] = useState(route.params?.memo || '');

  const handleSend = async () => {
    if (!recipient.trim()) {
      Alert.alert('Error', 'Please enter a recipient DID');
      return;
    }

    const amountNum = parseFloat(amount);
    if (isNaN(amountNum) || amountNum <= 0) {
      Alert.alert('Error', 'Please enter a valid amount');
      return;
    }

    try {
      await pay({
        to: recipient.trim(),
        amount: amountNum,
        memo: memo.trim() || undefined,
      });
      Alert.alert('Success', `Sent ${amountNum} hours!`, [
        { text: 'OK', onPress: () => navigation.goBack() },
      ]);
    } catch (err) {
      Alert.alert('Payment Failed', (err as Error).message);
    }
  };

  return (
    <KeyboardAvoidingView
      style={styles.container}
      behavior={Platform.OS === 'ios' ? 'padding' : 'height'}
    >
      <View style={styles.form}>
        <Text style={styles.label}>Recipient DID</Text>
        <TextInput
          style={styles.input}
          placeholder="did:icn:..."
          value={recipient}
          onChangeText={setRecipient}
          autoCapitalize="none"
          autoCorrect={false}
        />

        <Text style={styles.label}>Amount (hours)</Text>
        <TextInput
          style={styles.input}
          placeholder="0.0"
          value={amount}
          onChangeText={setAmount}
          keyboardType="decimal-pad"
        />

        <Text style={styles.label}>Memo (optional)</Text>
        <TextInput
          style={[styles.input, styles.memoInput]}
          placeholder="What's this for?"
          value={memo}
          onChangeText={setMemo}
          multiline
        />

        {error && <Text style={styles.error}>{error}</Text>}

        <TouchableOpacity
          style={[styles.button, isPaying && styles.buttonDisabled]}
          onPress={handleSend}
          disabled={isPaying}
        >
          {isPaying ? (
            <ActivityIndicator color="#fff" />
          ) : (
            <Text style={styles.buttonText}>Send Payment</Text>
          )}
        </TouchableOpacity>

        <TouchableOpacity
          style={styles.scanButton}
          onPress={() => navigation.navigate('Scan')}
        >
          <Text style={styles.scanButtonText}>Scan QR Code Instead</Text>
        </TouchableOpacity>
      </View>
    </KeyboardAvoidingView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#f5f5f5',
  },
  form: {
    padding: 24,
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
  memoInput: {
    height: 80,
    textAlignVertical: 'top',
  },
  button: {
    backgroundColor: '#4A90A4',
    borderRadius: 8,
    padding: 16,
    alignItems: 'center',
    marginTop: 8,
  },
  buttonDisabled: {
    opacity: 0.7,
  },
  buttonText: {
    color: '#fff',
    fontSize: 18,
    fontWeight: '600',
  },
  error: {
    color: '#e53935',
    fontSize: 14,
    marginBottom: 16,
    textAlign: 'center',
  },
  scanButton: {
    marginTop: 16,
    padding: 16,
    alignItems: 'center',
  },
  scanButtonText: {
    color: '#4A90A4',
    fontSize: 16,
  },
});
