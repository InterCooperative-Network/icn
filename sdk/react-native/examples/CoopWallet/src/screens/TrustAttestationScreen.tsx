/**
 * Trust Attestation Screen
 * 
 * Allows users to create trust attestations for other members.
 */

import React, { useState } from 'react';
import {
  View,
  Text,
  TextInput,
  TouchableOpacity,
  StyleSheet,
  ScrollView,
  Alert,
  ActivityIndicator,
} from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';
import { NativeStackScreenProps } from '@react-navigation/native-stack';
import { RootStackParamList } from '../../App';
import { useICNContext } from '../contexts/ICNContext';
import { useTrustAttestation } from '@icn/react-native';
import { TrustBadge } from '../components/TrustBadge';
import { useTheme, Theme } from '../contexts/ThemeContext';

type Props = NativeStackScreenProps<RootStackParamList, 'TrustAttestation'>;

export function TrustAttestationScreen({ navigation, route }: Props) {
  const { theme } = useTheme();
  const styles = createStyles(theme);
  const { client } = useICNContext();
  const { attest, isSubmitting, error: attestError, success } = useTrustAttestation(client);
  const { targetDid, targetName } = route.params || {};

  const [recipientDid, setRecipientDid] = useState(targetDid || '');
  const [trustScore, setTrustScore] = useState('0.5');
  const [memo, setMemo] = useState('');


  const handleScorePreset = (score: number) => {
    setTrustScore(score.toFixed(1));
  };

  const handleSubmit = async () => {
    if (!recipientDid.trim()) {
      Alert.alert('Error', 'Please enter a recipient DID');
      return;
    }

    const score = parseFloat(trustScore);
    if (isNaN(score) || score < 0 || score > 1) {
      Alert.alert('Error', 'Trust score must be between 0.0 and 1.0');
      return;
    }

    if (!memo.trim()) {
      Alert.alert('Error', 'Please provide a reason for this attestation');
      return;
    }

    try {
      await attest({
        targetDid: recipientDid.trim(),
        score,
        context: memo.trim(),
      });

      Alert.alert('Success', 'Trust attestation created successfully', [
        {
          text: 'OK',
          onPress: () => navigation.goBack(),
        },
      ]);
    } catch (error) {
      console.error('Failed to create attestation:', error);
      Alert.alert(
        'Error',
        error instanceof Error ? error.message : 'Failed to create trust attestation'
      );
    }
  };

  const score = parseFloat(trustScore);
  const isValidScore = !isNaN(score) && score >= 0 && score <= 1;

  return (
    <SafeAreaView style={styles.container} edges={['bottom']}>
      <ScrollView style={styles.content}>
        {/* Header */}
        <View style={styles.header}>
          <Text style={styles.title}>Create Trust Attestation</Text>
          <Text style={styles.subtitle}>
            Attest your trust relationship with another member
          </Text>
        </View>

        {/* Recipient */}
        <View style={styles.section}>
          <Text style={styles.label}>Recipient DID</Text>
          <TextInput
            style={styles.input}
            value={recipientDid}
            onChangeText={setRecipientDid}
            placeholder="did:icn:..."
            placeholderTextColor="#999"
            autoCapitalize="none"
            autoCorrect={false}
            editable={!targetDid} // Lock if passed from navigation
          />
          {targetName && (
            <Text style={styles.hint}>Attesting for: {targetName}</Text>
          )}
        </View>

        {/* Trust Score */}
        <View style={styles.section}>
          <Text style={styles.label}>Trust Score (0.0 - 1.0)</Text>
          <TextInput
            style={styles.input}
            value={trustScore}
            onChangeText={setTrustScore}
            placeholder="0.5"
            keyboardType="decimal-pad"
          />

          {/* Score Preview */}
          {isValidScore && (
            <View style={styles.scorePreview}>
              <Text style={styles.previewLabel}>Preview:</Text>
              <TrustBadge trustScore={score} size="medium" />
            </View>
          )}

          {/* Quick Presets */}
          <View style={styles.presets}>
            <Text style={styles.presetsLabel}>Quick Select:</Text>
            <View style={styles.presetButtons}>
              <TouchableOpacity
                style={[styles.presetButton, styles.presetIsolated]}
                onPress={() => handleScorePreset(0.05)}
              >
                <Text style={styles.presetText}>🔴 Unknown</Text>
                <Text style={styles.presetScore}>0.05</Text>
              </TouchableOpacity>

              <TouchableOpacity
                style={[styles.presetButton, styles.presetKnown]}
                onPress={() => handleScorePreset(0.3)}
              >
                <Text style={styles.presetText}>🟡 Known</Text>
                <Text style={styles.presetScore}>0.3</Text>
              </TouchableOpacity>

              <TouchableOpacity
                style={[styles.presetButton, styles.presetPartner]}
                onPress={() => handleScorePreset(0.6)}
              >
                <Text style={styles.presetText}>🟢 Partner</Text>
                <Text style={styles.presetScore}>0.6</Text>
              </TouchableOpacity>

              <TouchableOpacity
                style={[styles.presetButton, styles.presetFederated]}
                onPress={() => handleScorePreset(0.9)}
              >
                <Text style={styles.presetText}>🔵 Trusted</Text>
                <Text style={styles.presetScore}>0.9</Text>
              </TouchableOpacity>
            </View>
          </View>
        </View>

        {/* Memo */}
        <View style={styles.section}>
          <Text style={styles.label}>Reason / Context (required)</Text>
          <TextInput
            style={[styles.input, styles.memoInput]}
            value={memo}
            onChangeText={setMemo}
            placeholder="Why do you trust this person? How do you know them?"
            placeholderTextColor="#999"
            multiline
            numberOfLines={4}
            textAlignVertical="top"
          />
          <Text style={styles.hint}>
            This helps others understand your trust relationship
          </Text>
        </View>

        {/* Info Box */}
        <View style={styles.infoBox}>
          <Text style={styles.infoTitle}>ℹ️ About Trust Attestations</Text>
          <Text style={styles.infoText}>
            • Your attestation creates a trust edge in the network
          </Text>
          <Text style={styles.infoText}>
            • Others can see your trust score (but not your memo)
          </Text>
          <Text style={styles.infoText}>
            • Trust flows transitively through the network
          </Text>
          <Text style={styles.infoText}>
            • You can update your attestation anytime
          </Text>
        </View>

        {/* Submit Button */}
        <TouchableOpacity
          style={[
            styles.submitButton,
            (!isValidScore || !memo.trim() || !recipientDid.trim() || isSubmitting) &&
              styles.submitButtonDisabled,
          ]}
          onPress={handleSubmit}
          disabled={!isValidScore || !memo.trim() || !recipientDid.trim() || isSubmitting}
        >
          {isSubmitting ? (
            <ActivityIndicator color="#FFF" />
          ) : (
            <Text style={styles.submitButtonText}>Create Attestation</Text>
          )}
        </TouchableOpacity>

        {/* Cancel Button */}
        <TouchableOpacity
          style={styles.cancelButton}
          onPress={() => navigation.goBack()}
          disabled={isSubmitting}
        >
          <Text style={styles.cancelButtonText}>Cancel</Text>
        </TouchableOpacity>
      </ScrollView>
    </SafeAreaView>
  );
}

const createStyles = (theme: Theme) => StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: theme.colors.background,
  },
  content: {
    flex: 1,
    padding: 20,
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
  section: {
    marginBottom: 24,
  },
  label: {
    fontSize: 16,
    fontWeight: '600',
    color: theme.colors.text,
    marginBottom: 8,
  },
  input: {
    backgroundColor: theme.colors.card,
    borderWidth: 1,
    borderColor: theme.colors.border,
    borderRadius: 8,
    padding: 12,
    fontSize: 16,
    color: theme.colors.text,
  },
  memoInput: {
    minHeight: 100,
    paddingTop: 12,
  },
  hint: {
    fontSize: 14,
    color: theme.colors.textSecondary,
    marginTop: 4,
    fontStyle: 'italic',
  },
  scorePreview: {
    flexDirection: 'row',
    alignItems: 'center',
    marginTop: 12,
    padding: 12,
    backgroundColor: theme.colors.borderLight,
    borderRadius: 8,
  },
  previewLabel: {
    fontSize: 14,
    color: theme.colors.textSecondary,
    marginRight: 12,
  },
  presets: {
    marginTop: 16,
  },
  presetsLabel: {
    fontSize: 14,
    fontWeight: '600',
    color: theme.colors.textSecondary,
    marginBottom: 8,
  },
  presetButtons: {
    flexDirection: 'row',
    justifyContent: 'space-between',
  },
  presetButton: {
    flex: 1,
    alignItems: 'center',
    padding: 12,
    borderRadius: 8,
    marginHorizontal: 4,
    borderWidth: 2,
  },
  presetIsolated: {
    backgroundColor: theme.mode === 'dark' ? 'rgba(239, 68, 68, 0.2)' : '#FEF2F2',
    borderColor: '#EF4444',
  },
  presetKnown: {
    backgroundColor: theme.mode === 'dark' ? 'rgba(245, 158, 11, 0.2)' : '#FEF3C7',
    borderColor: '#F59E0B',
  },
  presetPartner: {
    backgroundColor: theme.mode === 'dark' ? 'rgba(16, 185, 129, 0.2)' : '#D1FAE5',
    borderColor: '#10B981',
  },
  presetFederated: {
    backgroundColor: theme.mode === 'dark' ? 'rgba(59, 130, 246, 0.2)' : '#DBEAFE',
    borderColor: '#3B82F6',
  },
  presetText: {
    fontSize: 14,
    fontWeight: '600',
    marginBottom: 4,
  },
  presetScore: {
    fontSize: 12,
    color: theme.colors.textSecondary,
  },
  infoBox: {
    backgroundColor: theme.mode === 'dark' ? 'rgba(59, 130, 246, 0.2)' : '#EFF6FF',
    borderLeftWidth: 4,
    borderLeftColor: '#3B82F6',
    padding: 16,
    borderRadius: 8,
    marginBottom: 24,
  },
  infoTitle: {
    fontSize: 16,
    fontWeight: '600',
    color: theme.mode === 'dark' ? '#93C5FD' : '#1E40AF',
    marginBottom: 8,
  },
  infoText: {
    fontSize: 14,
    color: theme.mode === 'dark' ? '#93C5FD' : '#1E40AF',
    marginBottom: 4,
  },
  submitButton: {
    backgroundColor: '#3B82F6',
    padding: 16,
    borderRadius: 8,
    alignItems: 'center',
    marginBottom: 12,
  },
  submitButtonDisabled: {
    backgroundColor: theme.colors.border,
  },
  submitButtonText: {
    color: '#FFF',
    fontSize: 18,
    fontWeight: '600',
  },
  cancelButton: {
    padding: 16,
    borderRadius: 8,
    alignItems: 'center',
    marginBottom: 32,
  },
  cancelButtonText: {
    color: theme.colors.textSecondary,
    fontSize: 16,
    fontWeight: '600',
  },
});
