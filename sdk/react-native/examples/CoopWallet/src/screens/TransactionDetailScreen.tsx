/**
 * Transaction Detail Screen
 *
 * Shows full details of a transaction with options to view profiles and copy data.
 */

import React, { useState, useEffect } from 'react';
import {
  View,
  Text,
  StyleSheet,
  ScrollView,
  TouchableOpacity,
  Alert,
  Platform,
  Clipboard,
} from 'react-native';
import { NativeStackScreenProps } from '@react-navigation/native-stack';
import * as SecureStore from 'expo-secure-store';
import { useAuth } from '@icn/react-native';
import { client } from '../client';
import { RootStackParamList } from '../../App';

const CONTACTS_KEY = 'icn_contacts';

interface Contact {
  did: string;
  name: string;
  addedAt: number;
}

type Props = NativeStackScreenProps<RootStackParamList, 'TransactionDetail'>;

// Copy to clipboard helper
const copyToClipboard = async (text: string): Promise<boolean> => {
  if (Platform.OS === 'web') {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      return false;
    }
  } else {
    Clipboard.setString(text);
    return true;
  }
};

export function TransactionDetailScreen({ navigation, route }: Props) {
  const { transaction } = route.params;
  const { did: userDid } = useAuth(client!);
  const [copiedField, setCopiedField] = useState<string | null>(null);
  const [isContact, setIsContact] = useState(false);
  const [contactName, setContactName] = useState<string | null>(null);

  const isSent = transaction.from === userDid;
  const otherPartyDid = isSent ? transaction.to : transaction.from;

  // Check if other party is already a contact
  useEffect(() => {
    const checkContact = async () => {
      try {
        const stored = await SecureStore.getItemAsync(CONTACTS_KEY);
        if (stored) {
          const contacts: Contact[] = JSON.parse(stored);
          const existing = contacts.find(c => c.did === otherPartyDid);
          if (existing) {
            setIsContact(true);
            setContactName(existing.name);
          }
        }
      } catch (e) {
        console.warn('Failed to check contacts:', e);
      }
    };
    checkContact();
  }, [otherPartyDid]);

  const handleAddToContacts = async () => {
    const promptForName = () => {
      if (Platform.OS === 'web') {
        const name = prompt('Enter a name for this contact:');
        if (name) saveContact(name);
      } else {
        Alert.prompt(
          'Add Contact',
          'Enter a name for this contact:',
          [
            { text: 'Cancel', style: 'cancel' },
            { text: 'Save', onPress: (name: string | undefined) => name && saveContact(name) },
          ],
          'plain-text',
          formatDid(otherPartyDid)
        );
      }
    };

    const saveContact = async (name: string) => {
      try {
        const stored = await SecureStore.getItemAsync(CONTACTS_KEY);
        const contacts: Contact[] = stored ? JSON.parse(stored) : [];

        // Check if already exists
        if (contacts.some(c => c.did === otherPartyDid)) {
          Alert.alert('Already Saved', 'This contact is already in your list.');
          return;
        }

        contacts.push({
          did: otherPartyDid,
          name: name.trim(),
          addedAt: Date.now(),
        });

        await SecureStore.setItemAsync(CONTACTS_KEY, JSON.stringify(contacts));
        setIsContact(true);
        setContactName(name.trim());

        if (Platform.OS === 'web') {
          alert('Contact saved!');
        } else {
          Alert.alert('Saved', `${name} has been added to your contacts.`);
        }
      } catch (e) {
        console.warn('Failed to save contact:', e);
        Alert.alert('Error', 'Failed to save contact');
      }
    };

    promptForName();
  };

  const handleRemoveContact = async () => {
    const remove = async () => {
      try {
        const stored = await SecureStore.getItemAsync(CONTACTS_KEY);
        if (stored) {
          const contacts: Contact[] = JSON.parse(stored);
          const filtered = contacts.filter(c => c.did !== otherPartyDid);
          await SecureStore.setItemAsync(CONTACTS_KEY, JSON.stringify(filtered));
          setIsContact(false);
          setContactName(null);
        }
      } catch (e) {
        console.warn('Failed to remove contact:', e);
      }
    };

    if (Platform.OS === 'web') {
      if (confirm('Remove this contact?')) remove();
    } else {
      Alert.alert(
        'Remove Contact',
        `Remove ${contactName || 'this contact'} from your contacts?`,
        [
          { text: 'Cancel', style: 'cancel' },
          { text: 'Remove', style: 'destructive', onPress: remove },
        ]
      );
    }
  };

  const formatDid = (did: string) => {
    if (did.length > 24) {
      return `${did.slice(0, 16)}...${did.slice(-8)}`;
    }
    return did;
  };

  const formatTimestamp = (timestamp: number) => {
    const date = new Date(timestamp * 1000);
    return date.toLocaleString(undefined, {
      weekday: 'short',
      year: 'numeric',
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  };

  const formatRelativeTime = (timestamp: number) => {
    const now = Date.now();
    const diff = now - timestamp * 1000;
    const minutes = Math.floor(diff / 60000);
    const hours = Math.floor(diff / 3600000);
    const days = Math.floor(diff / 86400000);

    if (minutes < 1) return 'Just now';
    if (minutes < 60) return `${minutes}m ago`;
    if (hours < 24) return `${hours}h ago`;
    if (days < 7) return `${days}d ago`;
    return formatTimestamp(timestamp);
  };

  const handleCopy = async (text: string, field: string) => {
    const success = await copyToClipboard(text);
    if (success) {
      setCopiedField(field);
      setTimeout(() => setCopiedField(null), 2000);
    }
  };

  const handleViewProfile = () => {
    navigation.navigate('MemberProfile', { memberDid: otherPartyDid });
  };

  return (
    <ScrollView style={styles.container}>
      {/* Transaction Type Header */}
      <View style={[styles.header, isSent ? styles.headerSent : styles.headerReceived]}>
        <Text style={styles.headerIcon}>{isSent ? '↑' : '↓'}</Text>
        <View style={styles.headerContent}>
          <Text style={styles.headerType}>
            {isSent ? 'Payment Sent' : 'Payment Received'}
          </Text>
          <Text style={styles.headerTime}>{formatRelativeTime(transaction.timestamp)}</Text>
        </View>
      </View>

      {/* Amount */}
      <View style={styles.amountCard}>
        <Text style={[styles.amount, isSent ? styles.amountSent : styles.amountReceived]}>
          {isSent ? '-' : '+'}{transaction.amount}
        </Text>
        <Text style={styles.currency}>{transaction.currency || 'hours'}</Text>
      </View>

      {/* Details Section */}
      <View style={styles.section}>
        <Text style={styles.sectionTitle}>Transaction Details</Text>

        {/* Transaction ID */}
        <TouchableOpacity
          style={styles.detailRow}
          onPress={() => handleCopy(transaction.id, 'id')}
        >
          <Text style={styles.detailLabel}>Transaction ID</Text>
          <View style={styles.detailValueRow}>
            <Text style={styles.detailValue}>{formatDid(transaction.id)}</Text>
            <Text style={styles.copyIcon}>
              {copiedField === 'id' ? '✓' : '📋'}
            </Text>
          </View>
        </TouchableOpacity>

        {/* From */}
        <TouchableOpacity
          style={styles.detailRow}
          onPress={() => handleCopy(transaction.from, 'from')}
        >
          <Text style={styles.detailLabel}>From</Text>
          <View style={styles.detailValueRow}>
            <Text style={[styles.detailValue, transaction.from === userDid && styles.youBadge]}>
              {transaction.from === userDid ? 'You' : formatDid(transaction.from)}
            </Text>
            <Text style={styles.copyIcon}>
              {copiedField === 'from' ? '✓' : '📋'}
            </Text>
          </View>
        </TouchableOpacity>

        {/* To */}
        <TouchableOpacity
          style={styles.detailRow}
          onPress={() => handleCopy(transaction.to, 'to')}
        >
          <Text style={styles.detailLabel}>To</Text>
          <View style={styles.detailValueRow}>
            <Text style={[styles.detailValue, transaction.to === userDid && styles.youBadge]}>
              {transaction.to === userDid ? 'You' : formatDid(transaction.to)}
            </Text>
            <Text style={styles.copyIcon}>
              {copiedField === 'to' ? '✓' : '📋'}
            </Text>
          </View>
        </TouchableOpacity>

        {/* Date/Time */}
        <View style={styles.detailRow}>
          <Text style={styles.detailLabel}>Date & Time</Text>
          <Text style={styles.detailValue}>{formatTimestamp(transaction.timestamp)}</Text>
        </View>

        {/* Memo */}
        {transaction.memo && (
          <View style={styles.memoSection}>
            <Text style={styles.detailLabel}>Memo</Text>
            <View style={styles.memoBox}>
              <Text style={styles.memoText}>{transaction.memo}</Text>
            </View>
          </View>
        )}
      </View>

      {/* Actions */}
      <View style={styles.actions}>
        <TouchableOpacity style={styles.actionButton} onPress={handleViewProfile}>
          <Text style={styles.actionIcon}>👤</Text>
          <Text style={styles.actionText}>
            View {isSent ? 'Recipient' : 'Sender'} Profile
          </Text>
        </TouchableOpacity>

        {isContact ? (
          <TouchableOpacity style={styles.actionButton} onPress={handleRemoveContact}>
            <Text style={styles.actionIcon}>⭐</Text>
            <Text style={styles.actionText}>
              {contactName || 'Saved'}
            </Text>
          </TouchableOpacity>
        ) : (
          <TouchableOpacity style={styles.actionButton} onPress={handleAddToContacts}>
            <Text style={styles.actionIcon}>➕</Text>
            <Text style={styles.actionText}>Add to Contacts</Text>
          </TouchableOpacity>
        )}
      </View>

      {/* Quick Actions */}
      <View style={styles.actions}>
        {isSent ? (
          <TouchableOpacity
            style={styles.actionButton}
            onPress={() => navigation.navigate('Payment', {
              recipientDid: transaction.to,
              memo: `Re: ${transaction.memo || 'previous payment'}`,
            })}
          >
            <Text style={styles.actionIcon}>🔁</Text>
            <Text style={styles.actionText}>Send Again</Text>
          </TouchableOpacity>
        ) : (
          <TouchableOpacity
            style={styles.actionButton}
            onPress={() => navigation.navigate('Payment', {
              recipientDid: transaction.from,
              memo: `Thanks for: ${transaction.memo || 'your payment'}`,
            })}
          >
            <Text style={styles.actionIcon}>↩️</Text>
            <Text style={styles.actionText}>Send Back</Text>
          </TouchableOpacity>
        )}
      </View>

      {/* Status */}
      <View style={styles.statusSection}>
        <View style={styles.statusBadge}>
          <Text style={styles.statusIcon}>✓</Text>
          <Text style={styles.statusText}>Confirmed</Text>
        </View>
      </View>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#f5f5f5',
  },
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    padding: 20,
    borderBottomLeftRadius: 24,
    borderBottomRightRadius: 24,
  },
  headerSent: {
    backgroundColor: '#ffebee',
  },
  headerReceived: {
    backgroundColor: '#e8f5e9',
  },
  headerIcon: {
    fontSize: 32,
    marginRight: 16,
  },
  headerContent: {
    flex: 1,
  },
  headerType: {
    fontSize: 20,
    fontWeight: '600',
    color: '#333',
  },
  headerTime: {
    fontSize: 14,
    color: '#666',
    marginTop: 4,
  },
  amountCard: {
    backgroundColor: '#fff',
    margin: 16,
    padding: 24,
    borderRadius: 16,
    alignItems: 'center',
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 2 },
    shadowOpacity: 0.1,
    shadowRadius: 4,
    elevation: 3,
  },
  amount: {
    fontSize: 48,
    fontWeight: '700',
  },
  amountSent: {
    color: '#d32f2f',
  },
  amountReceived: {
    color: '#388e3c',
  },
  currency: {
    fontSize: 18,
    color: '#666',
    marginTop: 4,
  },
  section: {
    backgroundColor: '#fff',
    margin: 16,
    marginTop: 0,
    padding: 16,
    borderRadius: 16,
  },
  sectionTitle: {
    fontSize: 16,
    fontWeight: '600',
    color: '#333',
    marginBottom: 16,
  },
  detailRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    paddingVertical: 12,
    borderBottomWidth: 1,
    borderBottomColor: '#f0f0f0',
  },
  detailLabel: {
    fontSize: 14,
    color: '#666',
  },
  detailValueRow: {
    flexDirection: 'row',
    alignItems: 'center',
  },
  detailValue: {
    fontSize: 14,
    color: '#333',
    fontWeight: '500',
  },
  youBadge: {
    color: '#4a90d9',
    fontWeight: '600',
  },
  copyIcon: {
    marginLeft: 8,
    fontSize: 14,
  },
  memoSection: {
    paddingTop: 12,
  },
  memoBox: {
    backgroundColor: '#f8f9fa',
    padding: 12,
    borderRadius: 8,
    marginTop: 8,
  },
  memoText: {
    fontSize: 14,
    color: '#333',
    lineHeight: 20,
  },
  actions: {
    flexDirection: 'row',
    margin: 16,
    marginTop: 0,
    gap: 12,
  },
  actionButton: {
    flex: 1,
    backgroundColor: '#fff',
    padding: 16,
    borderRadius: 12,
    alignItems: 'center',
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 1 },
    shadowOpacity: 0.05,
    shadowRadius: 2,
    elevation: 1,
  },
  actionIcon: {
    fontSize: 24,
    marginBottom: 8,
  },
  actionText: {
    fontSize: 12,
    color: '#333',
    textAlign: 'center',
  },
  statusSection: {
    alignItems: 'center',
    padding: 16,
    paddingBottom: 32,
  },
  statusBadge: {
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: '#e8f5e9',
    paddingHorizontal: 16,
    paddingVertical: 8,
    borderRadius: 20,
  },
  statusIcon: {
    color: '#388e3c',
    marginRight: 6,
  },
  statusText: {
    color: '#388e3c',
    fontWeight: '500',
  },
});
