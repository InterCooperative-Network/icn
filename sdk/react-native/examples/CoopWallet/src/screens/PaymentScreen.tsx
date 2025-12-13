/**
 * Payment Screen
 *
 * Send hours to another member.
 */

import React, { useState, useEffect } from 'react';
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
  ScrollView,
  FlatList,
} from 'react-native';
import { NativeStackNavigationProp } from '@react-navigation/native-stack';
import { RouteProp } from '@react-navigation/native';
import * as SecureStore from 'expo-secure-store';
import { useAuth, usePayment, useTransactions } from '@icn/react-native';
import { client } from '../client';
import { RootStackParamList } from '../../App';

const CONTACTS_KEY = 'icn_contacts';

interface Contact {
  did: string;
  name: string;
  addedAt: number;
}

interface RecentRecipient {
  did: string;
  name?: string;
  lastUsed: number;
}

type Props = {
  navigation: NativeStackNavigationProp<RootStackParamList, 'Payment'>;
  route: RouteProp<RootStackParamList, 'Payment'>;
};

export function PaymentScreen({ navigation, route }: Props) {
  const { coopId, did: userDid } = useAuth(client!);
  const { pay, isPaying, error } = usePayment(client!, coopId || '');
  const { transactions } = useTransactions(client!, coopId || '', 20);

  const [recipient, setRecipient] = useState(route.params?.recipient || route.params?.recipientDid || '');
  const [amount, setAmount] = useState(route.params?.amount?.toString() || '');
  const [memo, setMemo] = useState(route.params?.memo || '');
  const [contacts, setContacts] = useState<Contact[]>([]);
  const [recentRecipients, setRecentRecipients] = useState<RecentRecipient[]>([]);

  // Load contacts
  useEffect(() => {
    const loadContacts = async () => {
      try {
        const stored = await SecureStore.getItemAsync(CONTACTS_KEY);
        if (stored) {
          setContacts(JSON.parse(stored));
        }
      } catch (e) {
        console.warn('Failed to load contacts:', e);
      }
    };
    loadContacts();
  }, []);

  // Build recent recipients from transactions
  useEffect(() => {
    if (!transactions || !userDid) return;

    const recipientMap = new Map<string, RecentRecipient>();

    transactions.forEach((tx) => {
      // Only consider outgoing payments (sent by user)
      if (tx.from === userDid && tx.to !== userDid) {
        const existing = recipientMap.get(tx.to);
        if (!existing || tx.timestamp > existing.lastUsed) {
          // Check if this DID is in contacts
          const contact = contacts.find((c) => c.did === tx.to);
          recipientMap.set(tx.to, {
            did: tx.to,
            name: contact?.name,
            lastUsed: tx.timestamp,
          });
        }
      }
    });

    // Sort by most recent and take top 5
    const sorted = Array.from(recipientMap.values())
      .sort((a, b) => b.lastUsed - a.lastUsed)
      .slice(0, 5);

    setRecentRecipients(sorted);
  }, [transactions, userDid, contacts]);

  const formatDid = (did: string) => {
    if (did.length > 20) return `${did.slice(0, 10)}...${did.slice(-6)}`;
    return did;
  };

  const selectRecipient = (did: string) => {
    setRecipient(did);
  };

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
      <ScrollView style={styles.scrollContainer} keyboardShouldPersistTaps="handled">
        {/* Recent Recipients */}
        {recentRecipients.length > 0 && !recipient && (
          <View style={styles.quickSection}>
            <Text style={styles.quickTitle}>Recent</Text>
            <ScrollView horizontal showsHorizontalScrollIndicator={false}>
              {recentRecipients.map((r) => (
                <TouchableOpacity
                  key={r.did}
                  style={styles.quickChip}
                  onPress={() => selectRecipient(r.did)}
                >
                  <Text style={styles.quickChipName}>{r.name || formatDid(r.did)}</Text>
                </TouchableOpacity>
              ))}
            </ScrollView>
          </View>
        )}

        {/* Contacts */}
        {contacts.length > 0 && !recipient && (
          <View style={styles.quickSection}>
            <Text style={styles.quickTitle}>Contacts</Text>
            <ScrollView horizontal showsHorizontalScrollIndicator={false}>
              {contacts.slice(0, 8).map((c) => (
                <TouchableOpacity
                  key={c.did}
                  style={styles.quickChip}
                  onPress={() => selectRecipient(c.did)}
                >
                  <Text style={styles.quickChipName}>{c.name}</Text>
                </TouchableOpacity>
              ))}
              {contacts.length > 8 && (
                <TouchableOpacity
                  style={[styles.quickChip, styles.moreChip]}
                  onPress={() => navigation.navigate('Contacts')}
                >
                  <Text style={styles.moreChipText}>+{contacts.length - 8} more</Text>
                </TouchableOpacity>
              )}
            </ScrollView>
          </View>
        )}

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
          {recipient && (
            <TouchableOpacity style={styles.clearButton} onPress={() => setRecipient('')}>
              <Text style={styles.clearButtonText}>Clear</Text>
            </TouchableOpacity>
          )}

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
      </ScrollView>
    </KeyboardAvoidingView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#f5f5f5',
  },
  scrollContainer: {
    flex: 1,
  },
  quickSection: {
    padding: 16,
    paddingBottom: 8,
  },
  quickTitle: {
    fontSize: 14,
    fontWeight: '600',
    color: '#666',
    marginBottom: 12,
  },
  quickChip: {
    backgroundColor: '#fff',
    paddingHorizontal: 16,
    paddingVertical: 10,
    borderRadius: 20,
    marginRight: 8,
    borderWidth: 1,
    borderColor: '#e0e0e0',
  },
  quickChipName: {
    fontSize: 14,
    color: '#333',
    fontWeight: '500',
  },
  moreChip: {
    backgroundColor: '#f0f0f0',
    borderColor: '#ddd',
  },
  moreChipText: {
    fontSize: 14,
    color: '#666',
  },
  form: {
    padding: 24,
    paddingTop: 8,
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
  clearButton: {
    position: 'absolute',
    right: 32,
    top: 44,
  },
  clearButtonText: {
    color: '#4A90A4',
    fontSize: 14,
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
