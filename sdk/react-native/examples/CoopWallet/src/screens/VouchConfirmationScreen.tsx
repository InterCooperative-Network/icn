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
import { useTheme, Theme } from '../contexts/ThemeContext';

type Props = {
  navigation: NativeStackNavigationProp<RootStackParamList, 'VouchConfirmation'>;
  route: RouteProp<RootStackParamList, 'VouchConfirmation'>;
};

export function VouchConfirmationScreen({ navigation, route }: Props) {
  const { theme } = useTheme();
  const styles = createStyles(theme);
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

const createStyles = (theme: Theme) => StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: theme.colors.background,
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
    color: theme.colors.text,
    marginBottom: 8,
  },
  subtitle: {
    fontSize: 16,
    color: theme.colors.textSecondary,
  },
  highlight: {
    color: theme.colors.primary,
    fontWeight: '600',
  },
  warningCard: {
    backgroundColor: theme.colors.warningCardBackground,
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
    color: theme.colors.warningDark,
    lineHeight: 20,
  },
  section: {
    marginBottom: 24,
  },
  sectionTitle: {
    fontSize: 16,
    fontWeight: '600',
    color: theme.colors.text,
    marginBottom: 4,
  },
  sectionDesc: {
    fontSize: 13,
    color: theme.colors.textSecondary,
    marginBottom: 12,
  },
  textInput: {
    backgroundColor: theme.colors.card,
    borderRadius: 12,
    padding: 16,
    fontSize: 14,
    minHeight: 120,
    borderWidth: 1,
    borderColor: theme.colors.border,
    color: theme.colors.text,
  },
  charCount: {
    fontSize: 12,
    color: theme.colors.success,
    textAlign: 'right',
    marginTop: 4,
  },
  charCountWarning: {
    color: theme.colors.warning,
  },
  checkboxRow: {
    flexDirection: 'row',
    alignItems: 'flex-start',
    backgroundColor: theme.colors.card,
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
    borderColor: theme.colors.border,
    alignItems: 'center',
    justifyContent: 'center',
  },
  checkboxChecked: {
    backgroundColor: theme.colors.primary,
    borderColor: theme.colors.primary,
  },
  checkmark: {
    color: theme.colors.primaryText,
    fontSize: 16,
    fontWeight: 'bold',
  },
  checkboxLabel: {
    flex: 1,
    fontSize: 14,
    color: theme.colors.text,
    lineHeight: 20,
  },
  errorContainer: {
    backgroundColor: theme.colors.errorBackground,
    borderRadius: 12,
    padding: 16,
    marginBottom: 24,
  },
  errorText: {
    color: theme.colors.errorDark,
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
    backgroundColor: theme.colors.card,
    borderRadius: 12,
    padding: 16,
    alignItems: 'center',
    borderWidth: 1,
    borderColor: theme.colors.border,
  },
  cancelButtonText: {
    color: theme.colors.textSecondary,
    fontSize: 16,
    fontWeight: '600',
  },
  submitButton: {
    flex: 1,
    backgroundColor: theme.colors.success,
    borderRadius: 12,
    padding: 16,
    alignItems: 'center',
  },
  submitButtonText: {
    color: theme.colors.primaryText,
    fontSize: 16,
    fontWeight: '600',
  },
  disabledButton: {
    opacity: 0.5,
  },
});
