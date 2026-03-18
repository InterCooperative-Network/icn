/**
 * Contacts Screen
 *
 * Manage saved contacts/favorite recipients for quick payments.
 */

import React, { useState, useEffect, useCallback } from 'react';
import {
  View,
  Text,
  StyleSheet,
  FlatList,
  TouchableOpacity,
  TextInput,
  Alert,
  ActivityIndicator,
} from 'react-native';
import * as SecureStore from 'expo-secure-store';
import { ICNMobileClient } from '@icn/react-native';
import { useNavigation } from '@react-navigation/native';
import { Identicon } from '../components/Identicon';
import { useTheme, Theme } from '../contexts/ThemeContext';

interface ContactsScreenProps {
  client: ICNMobileClient;
  coopId: string;
}

interface Contact {
  did: string;
  name: string;
  addedAt: number;
}

const CONTACTS_KEY = 'icn_contacts';

export function ContactsScreen({ client, coopId }: ContactsScreenProps) {
  const { theme } = useTheme();
  const styles = createStyles(theme);
  const navigation = useNavigation();
  const [contacts, setContacts] = useState<Contact[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [searchQuery, setSearchQuery] = useState('');
  const [isAdding, setIsAdding] = useState(false);
  const [newDid, setNewDid] = useState('');
  const [newName, setNewName] = useState('');

  useEffect(() => {
    loadContacts();
  }, []);

  const loadContacts = async () => {
    try {
      const stored = await SecureStore.getItemAsync(CONTACTS_KEY);
      if (stored) {
        setContacts(JSON.parse(stored));
      }
    } catch (error) {
      console.warn('Failed to load contacts:', error);
    } finally {
      setIsLoading(false);
    }
  };

  const saveContacts = async (updatedContacts: Contact[]) => {
    try {
      await SecureStore.setItemAsync(CONTACTS_KEY, JSON.stringify(updatedContacts));
      setContacts(updatedContacts);
    } catch (error) {
      Alert.alert('Error', 'Failed to save contacts');
    }
  };

  const addContact = async () => {
    if (!newDid.trim()) {
      Alert.alert('Error', 'Please enter a DID');
      return;
    }

    if (!newDid.startsWith('did:icn:')) {
      Alert.alert('Error', 'Invalid DID format. Must start with did:icn:');
      return;
    }

    if (contacts.some((c) => c.did === newDid.trim())) {
      Alert.alert('Error', 'Contact already exists');
      return;
    }

    const newContact: Contact = {
      did: newDid.trim(),
      name: newName.trim() || `Contact ${contacts.length + 1}`,
      addedAt: Date.now(),
    };

    await saveContacts([...contacts, newContact]);
    setNewDid('');
    setNewName('');
    setIsAdding(false);
  };

  const deleteContact = (did: string) => {
    Alert.alert('Delete Contact', 'Are you sure you want to remove this contact?', [
      { text: 'Cancel', style: 'cancel' },
      {
        text: 'Delete',
        style: 'destructive',
        onPress: async () => {
          const updated = contacts.filter((c) => c.did !== did);
          await saveContacts(updated);
        },
      },
    ]);
  };

  const handleSendPayment = (contact: Contact) => {
    (navigation as any).navigate('Payment', { recipientDid: contact.did });
  };

  const handleViewProfile = (contact: Contact) => {
    (navigation as any).navigate('MemberProfile', { memberDid: contact.did });
  };

  const filteredContacts = contacts.filter((contact) => {
    if (!searchQuery) return true;
    const query = searchQuery.toLowerCase();
    return (
      contact.name.toLowerCase().includes(query) ||
      contact.did.toLowerCase().includes(query)
    );
  });

  const formatDid = (did: string) => {
    return `${did.slice(0, 14)}...${did.slice(-6)}`;
  };

  const renderContact = useCallback(
    ({ item: contact }: { item: Contact }) => (
      <View style={styles.contactCard}>
        <TouchableOpacity
          style={styles.contactInfo}
          onPress={() => handleViewProfile(contact)}
        >
          <Identicon did={contact.did} size={44} />
          <View style={styles.contactDetails}>
            <Text style={styles.contactName}>{contact.name}</Text>
            <Text style={styles.contactDid}>{formatDid(contact.did)}</Text>
          </View>
        </TouchableOpacity>

        <View style={styles.contactActions}>
          <TouchableOpacity
            style={styles.actionButton}
            onPress={() => handleSendPayment(contact)}
          >
            <Text style={styles.actionButtonText}>Pay</Text>
          </TouchableOpacity>
          <TouchableOpacity
            style={[styles.actionButton, styles.deleteButton]}
            onPress={() => deleteContact(contact.did)}
          >
            <Text style={styles.deleteButtonText}>X</Text>
          </TouchableOpacity>
        </View>
      </View>
    ),
    [contacts]
  );

  const renderHeader = () => (
    <View style={styles.headerContainer}>
      {/* Search Bar */}
      <View style={styles.searchContainer}>
        <TextInput
          style={styles.searchInput}
          placeholder="Search contacts..."
          placeholderTextColor="#999"
          value={searchQuery}
          onChangeText={setSearchQuery}
        />
      </View>

      {/* Add Button */}
      {!isAdding && (
        <TouchableOpacity
          style={styles.addButton}
          onPress={() => setIsAdding(true)}
        >
          <Text style={styles.addButtonText}>+ Add Contact</Text>
        </TouchableOpacity>
      )}

      {/* Add Form */}
      {isAdding && (
        <View style={styles.addForm}>
          <TextInput
            style={styles.addInput}
            placeholder="DID (did:icn:...)"
            placeholderTextColor="#999"
            value={newDid}
            onChangeText={setNewDid}
            autoCapitalize="none"
            autoCorrect={false}
          />
          <TextInput
            style={styles.addInput}
            placeholder="Name (optional)"
            placeholderTextColor="#999"
            value={newName}
            onChangeText={setNewName}
          />
          <View style={styles.addFormButtons}>
            <TouchableOpacity
              style={[styles.formButton, styles.cancelButton]}
              onPress={() => {
                setIsAdding(false);
                setNewDid('');
                setNewName('');
              }}
            >
              <Text style={styles.cancelButtonText}>Cancel</Text>
            </TouchableOpacity>
            <TouchableOpacity
              style={[styles.formButton, styles.saveButton]}
              onPress={addContact}
            >
              <Text style={styles.saveButtonText}>Save</Text>
            </TouchableOpacity>
          </View>
        </View>
      )}
    </View>
  );

  const renderEmpty = () => {
    if (isLoading) return null;
    return (
      <View style={styles.emptyContainer}>
        <Text style={styles.emptyTitle}>No Contacts</Text>
        <Text style={styles.emptyText}>
          {searchQuery
            ? 'No contacts match your search.'
            : 'Add contacts for quick payments.'}
        </Text>
      </View>
    );
  };

  if (isLoading) {
    return (
      <View style={styles.loadingContainer}>
        <ActivityIndicator size="large" color={theme.colors.primary} />
      </View>
    );
  }

  return (
    <View style={styles.container}>
      <FlatList
        data={filteredContacts}
        keyExtractor={(item) => item.did}
        renderItem={renderContact}
        ListHeaderComponent={renderHeader}
        ListEmptyComponent={renderEmpty}
        contentContainerStyle={
          filteredContacts.length === 0 ? styles.emptyList : styles.listContent
        }
        showsVerticalScrollIndicator={false}
      />
    </View>
  );
}

const createStyles = (theme: Theme) => StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: theme.colors.background,
  },
  loadingContainer: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    backgroundColor: theme.colors.background,
  },
  listContent: {
    paddingBottom: 24,
  },
  headerContainer: {
    padding: 16,
    backgroundColor: theme.colors.surface,
    borderBottomWidth: 1,
    borderBottomColor: theme.colors.border,
  },
  searchContainer: {
    marginBottom: 12,
  },
  searchInput: {
    backgroundColor: theme.colors.borderLight,
    borderRadius: 8,
    paddingHorizontal: 16,
    paddingVertical: 12,
    fontSize: 16,
    color: theme.colors.text,
  },
  addButton: {
    backgroundColor: theme.colors.primary,
    padding: 14,
    borderRadius: 8,
    alignItems: 'center',
  },
  addButtonText: {
    color: '#fff',
    fontSize: 16,
    fontWeight: '600',
  },
  addForm: {
    backgroundColor: theme.colors.borderLight,
    borderRadius: 8,
    padding: 16,
  },
  addInput: {
    backgroundColor: theme.colors.card,
    borderRadius: 8,
    paddingHorizontal: 16,
    paddingVertical: 12,
    fontSize: 16,
    color: theme.colors.text,
    marginBottom: 12,
    borderWidth: 1,
    borderColor: theme.colors.border,
  },
  addFormButtons: {
    flexDirection: 'row',
    gap: 12,
  },
  formButton: {
    flex: 1,
    padding: 14,
    borderRadius: 8,
    alignItems: 'center',
  },
  cancelButton: {
    backgroundColor: theme.colors.border,
  },
  cancelButtonText: {
    color: theme.colors.textSecondary,
    fontWeight: '600',
  },
  saveButton: {
    backgroundColor: theme.colors.primary,
  },
  saveButtonText: {
    color: '#fff',
    fontWeight: '600',
  },
  contactCard: {
    backgroundColor: theme.colors.card,
    marginHorizontal: 16,
    marginTop: 12,
    borderRadius: 12,
    padding: 16,
    flexDirection: 'row',
    alignItems: 'center',
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 1 },
    shadowOpacity: 0.1,
    shadowRadius: 2,
    elevation: 2,
  },
  contactInfo: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
  },
  avatar: {
    width: 48,
    height: 48,
    borderRadius: 24,
    backgroundColor: theme.colors.primary,
    justifyContent: 'center',
    alignItems: 'center',
  },
  avatarText: {
    fontSize: 20,
    fontWeight: '700',
    color: '#fff',
  },
  contactDetails: {
    marginLeft: 12,
    flex: 1,
  },
  contactName: {
    fontSize: 16,
    fontWeight: '600',
    color: theme.colors.text,
  },
  contactDid: {
    fontSize: 12,
    color: theme.colors.textMuted,
    fontFamily: 'monospace',
    marginTop: 2,
  },
  contactActions: {
    flexDirection: 'row',
    gap: 8,
  },
  actionButton: {
    backgroundColor: theme.colors.primary,
    paddingHorizontal: 16,
    paddingVertical: 8,
    borderRadius: 6,
  },
  actionButtonText: {
    color: '#fff',
    fontWeight: '600',
    fontSize: 14,
  },
  deleteButton: {
    backgroundColor: theme.colors.borderLight,
    paddingHorizontal: 12,
  },
  deleteButtonText: {
    color: theme.colors.textMuted,
    fontWeight: '600',
    fontSize: 14,
  },
  emptyList: {
    flex: 1,
  },
  emptyContainer: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    padding: 32,
    marginTop: 60,
  },
  emptyTitle: {
    fontSize: 18,
    fontWeight: '600',
    color: theme.colors.text,
    marginBottom: 8,
  },
  emptyText: {
    fontSize: 14,
    color: theme.colors.textMuted,
    textAlign: 'center',
  },
});

export default ContactsScreen;
