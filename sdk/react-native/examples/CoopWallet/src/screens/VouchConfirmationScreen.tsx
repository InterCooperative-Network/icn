/**
 * Vouch Confirmation Screen
 *
 * Modal-style screen for confirming a steward vouch.
 */

import React, { useState } from 'react';
import {
  View,
  Text,
  TouchableOpacity,
  StyleSheet,
  ScrollView,
  TextInput,
  Alert,
  KeyboardAvoidingView,
  Platform,
} from 'react-native';
import { NativeStackNavigationProp } from '@react-navigation/native-stack';
import { RouteProp } from '@react-navigation/native';
import { useVouch } from '@icn/react-native';
import { client } from '../client';
import { RootStackParamList } from '../../App';

type Props = {
  navigation: NativeStackNavigationProp<RootStackParamList, 'VouchConfirmation'>;
  route: RouteProp<RootStackParamList, 'VouchConfirmation'>;
};

export function VouchConfirmationScreen({ navigation, route }: Props) {
  const { enrollmentId, identityName } = route.params;
  const [statement, setStatement] = useState('');
  const [check1, setCheck1] = useState(false);
  const [check2, setCheck2] = useState(false);

  const { vouch, isSubmitting, error } = useVouch(client!);

  const isValid = statement.trim().length >= 10 && check1 && check2;

  const handleSubmit = async () => {
    if (!isValid) return;

    const success = await vouch(enrollmentId, statement);
    if (success) {
      Alert.alert(
        'Vouch Submitted',
        `You have successfully vouched for ${identityName}.`,
        [{ text: 'OK', onPress: () => navigation.navigate('StewardDashboard') }]
      );
    } else if (error) {
      Alert.alert('Error', error);
    }
  };

  return (
    <KeyboardAvoidingView
      style={styles.container}
      behavior={Platform.OS === 'ios' ? 'padding' : 'height'}
    >
      <ScrollView contentContainerStyle={styles.content}>
        {/* Header */}
        <View style={styles.header}>
          <Text style={styles.title}>Vouch for Identity</Text>
          <Text style={styles.subtitle}>
            You are vouching for <Text style={styles.highlight}>{identityName}</Text>
          </Text>
        </View>

        {/* Warning */}
        <View style={styles.warningCard}>
          <Text style={styles.warningIcon}>⚠️</Text>
          <Text style={styles.warningText}>
            By vouching for this identity, you are attesting that you have verified this person's
            identity through a trusted process. Your reputation as a steward is affected by the
            accuracy of your vouches.
          </Text>
        </View>

        {/* Statement Input */}
        <View style={styles.section}>
          <Text style={styles.sectionTitle}>Vouch Statement</Text>
          <Text style={styles.sectionDesc}>
            Describe how you verified this person's identity (minimum 10 characters)
          </Text>
          <TextInput
            style={styles.textInput}
            placeholder="e.g., I verified this person's identity in person at our cooperative's office..."
            value={statement}
            onChangeText={setStatement}
            multiline
            numberOfLines={4}
            textAlignVertical="top"
          />
          <Text style={[styles.charCount, statement.length < 10 && styles.charCountWarning]}>
            {statement.length}/10 minimum
          </Text>
        </View>

        {/* Verification Checkboxes */}
        <View style={styles.section}>
          <Text style={styles.sectionTitle}>Verification Checklist</Text>
          <TouchableOpacity
            style={styles.checkboxRow}
            onPress={() => setCheck1(!check1)}
          >
            <View style={[styles.checkbox, check1 && styles.checkboxChecked]}>
              {check1 && <Text style={styles.checkmark}>✓</Text>}
            </View>
            <Text style={styles.checkboxLabel}>
              I have personally verified this person's identity through a trusted process
            </Text>
          </TouchableOpacity>
          <TouchableOpacity
            style={styles.checkboxRow}
            onPress={() => setCheck2(!check2)}
          >
            <View style={[styles.checkbox, check2 && styles.checkboxChecked]}>
              {check2 && <Text style={styles.checkmark}>✓</Text>}
            </View>
            <Text style={styles.checkboxLabel}>
              I understand that false vouches may result in loss of steward privileges
            </Text>
          </TouchableOpacity>
        </View>

        {/* Error Display */}
        {error && (
          <View style={styles.errorContainer}>
            <Text style={styles.errorText}>{error}</Text>
          </View>
        )}

        {/* Action Buttons */}
        <View style={styles.actions}>
          <TouchableOpacity
            style={styles.cancelButton}
            onPress={() => navigation.goBack()}
          >
            <Text style={styles.cancelButtonText}>Cancel</Text>
          </TouchableOpacity>
          <TouchableOpacity
            style={[styles.submitButton, !isValid && styles.disabledButton]}
            onPress={handleSubmit}
            disabled={!isValid || isSubmitting}
          >
            <Text style={styles.submitButtonText}>
              {isSubmitting ? 'Submitting...' : 'Submit Vouch'}
            </Text>
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
  content: {
    padding: 16,
  },
  header: {
    marginBottom: 24,
  },
  title: {
    fontSize: 28,
    fontWeight: 'bold',
    color: '#333',
    marginBottom: 8,
  },
  subtitle: {
    fontSize: 16,
    color: '#666',
  },
  highlight: {
    color: '#4A90A4',
    fontWeight: '600',
  },
  warningCard: {
    backgroundColor: '#FFF3E0',
    borderRadius: 12,
    padding: 16,
    flexDirection: 'row',
    gap: 12,
    marginBottom: 24,
  },
  warningIcon: {
    fontSize: 24,
  },
  warningText: {
    flex: 1,
    fontSize: 14,
    color: '#E65100',
    lineHeight: 20,
  },
  section: {
    marginBottom: 24,
  },
  sectionTitle: {
    fontSize: 16,
    fontWeight: '600',
    color: '#333',
    marginBottom: 4,
  },
  sectionDesc: {
    fontSize: 13,
    color: '#666',
    marginBottom: 12,
  },
  textInput: {
    backgroundColor: '#fff',
    borderRadius: 12,
    padding: 16,
    fontSize: 14,
    minHeight: 120,
    borderWidth: 1,
    borderColor: '#e0e0e0',
  },
  charCount: {
    fontSize: 12,
    color: '#4caf50',
    textAlign: 'right',
    marginTop: 4,
  },
  charCountWarning: {
    color: '#ff9800',
  },
  checkboxRow: {
    flexDirection: 'row',
    alignItems: 'flex-start',
    backgroundColor: '#fff',
    borderRadius: 12,
    padding: 16,
    marginBottom: 12,
    gap: 12,
  },
  checkbox: {
    width: 24,
    height: 24,
    borderRadius: 6,
    borderWidth: 2,
    borderColor: '#ccc',
    alignItems: 'center',
    justifyContent: 'center',
  },
  checkboxChecked: {
    backgroundColor: '#4A90A4',
    borderColor: '#4A90A4',
  },
  checkmark: {
    color: '#fff',
    fontSize: 16,
    fontWeight: 'bold',
  },
  checkboxLabel: {
    flex: 1,
    fontSize: 14,
    color: '#333',
    lineHeight: 20,
  },
  errorContainer: {
    backgroundColor: '#ffcdd2',
    borderRadius: 12,
    padding: 16,
    marginBottom: 24,
  },
  errorText: {
    color: '#c62828',
    fontSize: 14,
  },
  actions: {
    flexDirection: 'row',
    gap: 12,
    marginTop: 8,
    marginBottom: 32,
  },
  cancelButton: {
    flex: 1,
    backgroundColor: '#fff',
    borderRadius: 12,
    padding: 16,
    alignItems: 'center',
    borderWidth: 1,
    borderColor: '#e0e0e0',
  },
  cancelButtonText: {
    color: '#666',
    fontSize: 16,
    fontWeight: '600',
  },
  submitButton: {
    flex: 1,
    backgroundColor: '#4caf50',
    borderRadius: 12,
    padding: 16,
    alignItems: 'center',
  },
  submitButtonText: {
    color: '#fff',
    fontSize: 16,
    fontWeight: '600',
  },
  disabledButton: {
    opacity: 0.5,
  },
});
