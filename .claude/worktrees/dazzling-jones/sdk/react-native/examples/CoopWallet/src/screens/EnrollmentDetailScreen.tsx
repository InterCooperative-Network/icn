/**
 * Enrollment Detail Screen
 *
 * Shows detailed information about an enrollment and allows stewards to vouch or reject.
 */

import React, { useState } from 'react';
import {
  View,
  Text,
  TouchableOpacity,
  StyleSheet,
  ScrollView,
  ActivityIndicator,
  Alert,
  TextInput,
  Modal,
} from 'react-native';
import { NativeStackNavigationProp } from '@react-navigation/native-stack';
import { RouteProp } from '@react-navigation/native';
import { useEnrollmentDetail, useReject } from '@icn/react-native';
import { client } from '../client';
import { RootStackParamList } from '../../App';
import { useTheme, Theme } from '../contexts/ThemeContext';

type Props = {
  navigation: NativeStackNavigationProp<RootStackParamList, 'EnrollmentDetail'>;
  route: RouteProp<RootStackParamList, 'EnrollmentDetail'>;
};

export function EnrollmentDetailScreen({ navigation, route }: Props) {
  const { theme } = useTheme();
  const styles = createStyles(theme);
  const { enrollmentId } = route.params;
  const [showRejectModal, setShowRejectModal] = useState(false);
  const [rejectReason, setRejectReason] = useState('');

  const { enrollment, isLoading, error, refresh } = useEnrollmentDetail(client!, enrollmentId);
  const { reject, isSubmitting: isRejecting, error: rejectError } = useReject(client!);

  const handleVouch = () => {
    if (enrollment) {
      navigation.navigate('VouchConfirmation', {
        enrollmentId: enrollment.enrollment_id,
        identityName: enrollment.identity_name,
      });
    }
  };

  const handleReject = async () => {
    if (!rejectReason.trim()) {
      Alert.alert('Error', 'Please provide a reason for rejection.');
      return;
    }

    const success = await reject(enrollmentId, rejectReason);
    if (success) {
      setShowRejectModal(false);
      Alert.alert('Rejected', 'The enrollment has been rejected.', [
        { text: 'OK', onPress: () => navigation.goBack() },
      ]);
    } else if (rejectError) {
      Alert.alert('Error', rejectError);
    }
  };

  const canVouch = enrollment && enrollment.level >= 1 && !enrollment.has_steward_vouch && !enrollment.rejected;
  const canReject = enrollment && !enrollment.rejected;

  if (isLoading) {
    return (
      <View style={styles.loadingContainer}>
        <ActivityIndicator size="large" color={theme.colors.primary} />
        <Text style={styles.loadingText}>Loading enrollment...</Text>
      </View>
    );
  }

  if (error) {
    return (
      <View style={styles.errorContainer}>
        <Text style={styles.errorText} accessibilityRole="alert">{error}</Text>
        <TouchableOpacity
          style={styles.retryButton}
          onPress={refresh}
          accessibilityRole="button"
          accessibilityLabel="Retry loading enrollment"
        >
          <Text style={styles.retryText}>Retry</Text>
        </TouchableOpacity>
      </View>
    );
  }

  if (!enrollment) {
    return (
      <View style={styles.errorContainer}>
        <Text style={styles.errorText} accessibilityRole="alert">Enrollment not found</Text>
        <TouchableOpacity
          style={styles.retryButton}
          onPress={() => navigation.goBack()}
          accessibilityRole="button"
          accessibilityLabel="Go back to previous screen"
        >
          <Text style={styles.retryText}>Go Back</Text>
        </TouchableOpacity>
      </View>
    );
  }

  return (
    <ScrollView style={styles.container}>
      {/* Status Banner */}
      {enrollment.rejected && (
        <View style={styles.rejectedBanner}>
          <Text style={styles.rejectedText}>This enrollment has been rejected</Text>
          {enrollment.rejection_reason && (
            <Text style={styles.rejectionReason}>Reason: {enrollment.rejection_reason}</Text>
          )}
        </View>
      )}

      {/* Identity Info */}
      <View style={styles.section}>
        <Text style={styles.sectionTitle}>Identity Information</Text>
        <View style={styles.infoCard}>
          <View style={styles.infoRow}>
            <Text style={styles.infoLabel}>Name</Text>
            <Text style={styles.infoValue}>{enrollment.identity_name}</Text>
          </View>
          <View style={styles.infoRow}>
            <Text style={styles.infoLabel}>Enrollment ID</Text>
            <Text style={[styles.infoValue, styles.monospace]}>
              {enrollment.enrollment_id.substring(0, 16)}...
            </Text>
          </View>
          <View style={styles.infoRow}>
            <Text style={styles.infoLabel}>Cooperative</Text>
            <Text style={styles.infoValue}>{enrollment.coop_id}</Text>
          </View>
        </View>
      </View>

      {/* Verification Status */}
      <View style={styles.section}>
        <Text style={styles.sectionTitle}>Verification Status</Text>
        <View style={styles.infoCard}>
          <View style={styles.infoRow}>
            <Text style={styles.infoLabel}>Level</Text>
            <View style={[styles.levelBadge, getLevelStyle(enrollment.level, theme)]}>
              <Text style={styles.levelText}>Level {enrollment.level}</Text>
            </View>
          </View>
          <View style={styles.infoRow}>
            <Text style={styles.infoLabel}>Status</Text>
            <Text style={styles.statusText}>{formatStatus(enrollment.status)}</Text>
          </View>
          <View style={styles.infoRow}>
            <Text style={styles.infoLabel}>Device Verified</Text>
            <Text style={enrollment.level >= 1 ? styles.verified : styles.pending}>
              {enrollment.level >= 1 ? '✓ Yes' : '○ No'}
            </Text>
          </View>
          <View style={styles.infoRow}>
            <Text style={styles.infoLabel}>Steward Vouch</Text>
            <Text style={enrollment.has_steward_vouch ? styles.verified : styles.pending}>
              {enrollment.has_steward_vouch ? '✓ Yes' : '○ No'}
            </Text>
          </View>
        </View>
      </View>

      {/* Timeline */}
      <View style={styles.section}>
        <Text style={styles.sectionTitle}>Timeline</Text>
        <View style={styles.infoCard}>
          <View style={styles.infoRow}>
            <Text style={styles.infoLabel}>Created</Text>
            <Text style={styles.infoValue}>{formatDate(enrollment.created_at)}</Text>
          </View>
          <View style={styles.infoRow}>
            <Text style={styles.infoLabel}>Expires</Text>
            <Text style={[styles.infoValue, isExpiringSoon(enrollment.expires_at) && styles.warning]}>
              {formatDate(enrollment.expires_at)}
            </Text>
          </View>
          {enrollment.rejected_at && (
            <View style={styles.infoRow}>
              <Text style={styles.infoLabel}>Rejected</Text>
              <Text style={styles.infoValue}>{formatDate(enrollment.rejected_at)}</Text>
            </View>
          )}
        </View>
      </View>

      {/* Level Description */}
      <View style={styles.section}>
        <Text style={styles.sectionTitle}>What This Means</Text>
        <View style={styles.descriptionCard}>
          <Text style={styles.descriptionText}>{getLevelDescription(enrollment.level)}</Text>
        </View>
      </View>

      {/* Action Buttons */}
      {!enrollment.rejected && (
        <View style={styles.actions}>
          <TouchableOpacity
            style={[styles.vouchButton, !canVouch && styles.disabledButton]}
            onPress={handleVouch}
            disabled={!canVouch}
            accessibilityRole="button"
            accessibilityLabel={
              enrollment.has_steward_vouch
                ? 'Already vouched for this identity'
                : enrollment.level < 1
                ? 'Awaiting device verification before vouching'
                : `Vouch for ${enrollment.identity_name}`
            }
            accessibilityState={{ disabled: !canVouch }}
          >
            <Text style={styles.vouchButtonText}>
              {enrollment.has_steward_vouch
                ? 'Already Vouched'
                : enrollment.level < 1
                ? 'Awaiting Device Verification'
                : 'Vouch for Identity'}
            </Text>
          </TouchableOpacity>

          <TouchableOpacity
            style={[styles.rejectButton, !canReject && styles.disabledButton]}
            onPress={() => setShowRejectModal(true)}
            disabled={!canReject}
            accessibilityRole="button"
            accessibilityLabel={`Reject ${enrollment.identity_name}'s enrollment`}
            accessibilityState={{ disabled: !canReject }}
          >
            <Text style={styles.rejectButtonText}>Reject Enrollment</Text>
          </TouchableOpacity>
        </View>
      )}

      {/* Reject Modal */}
      <Modal visible={showRejectModal} transparent animationType="slide">
        <View style={styles.modalOverlay}>
          <View style={styles.modalContent}>
            <Text style={styles.modalTitle}>Reject Enrollment</Text>
            <Text style={styles.modalSubtitle}>
              Please provide a reason for rejecting {enrollment.identity_name}'s enrollment.
            </Text>
            <TextInput
              style={styles.reasonInput}
              placeholder="Reason for rejection..."
              value={rejectReason}
              onChangeText={setRejectReason}
              multiline
              numberOfLines={4}
              textAlignVertical="top"
              accessibilityLabel="Reason for rejection"
              accessibilityHint="Enter the reason for rejecting this enrollment"
            />
            <View style={styles.modalActions}>
              <TouchableOpacity
                style={styles.cancelButton}
                onPress={() => {
                  setShowRejectModal(false);
                  setRejectReason('');
                }}
                accessibilityRole="button"
                accessibilityLabel="Cancel rejection"
              >
                <Text style={styles.cancelButtonText}>Cancel</Text>
              </TouchableOpacity>
              <TouchableOpacity
                style={[styles.confirmRejectButton, isRejecting && styles.disabledButton]}
                onPress={handleReject}
                disabled={isRejecting}
                accessibilityRole="button"
                accessibilityLabel={isRejecting ? 'Rejecting enrollment' : 'Confirm rejection'}
                accessibilityState={{ disabled: isRejecting }}
              >
                <Text style={styles.confirmRejectText}>
                  {isRejecting ? 'Rejecting...' : 'Confirm Rejection'}
                </Text>
              </TouchableOpacity>
            </View>
          </View>
        </View>
      </Modal>
    </ScrollView>
  );
}

function formatStatus(status: string): string {
  const statusMap: Record<string, string> = {
    pending_device_verification: 'Awaiting Device Verification',
    pending_steward_vouch: 'Awaiting Steward Vouch',
    ready_for_completion: 'Ready for Completion',
    rejected: 'Rejected',
  };
  return statusMap[status] || status;
}

function formatDate(dateString: string): string {
  return new Date(dateString).toLocaleString();
}

function isExpiringSoon(expiresAt: string): boolean {
  const expiry = new Date(expiresAt);
  const now = new Date();
  const hoursUntilExpiry = (expiry.getTime() - now.getTime()) / (1000 * 60 * 60);
  return hoursUntilExpiry < 24;
}

function getLevelStyle(level: number, theme: Theme) {
  switch (level) {
    case 0:
      return { backgroundColor: theme.colors.errorBackground };
    case 1:
      return { backgroundColor: theme.colors.warningBackground };
    case 2:
      return { backgroundColor: theme.colors.successBackground };
    default:
      return { backgroundColor: theme.colors.border };
  }
}

function getLevelDescription(level: number): string {
  switch (level) {
    case 0:
      return 'This enrollment is new and awaiting device verification. The user needs to complete device proof before you can vouch for them.';
    case 1:
      return 'Device verification complete. This user has proven control of their device. You can now vouch for their identity if you have verified them in person or through a trusted process.';
    case 2:
      return 'This enrollment is ready for completion. All verification steps have been completed.';
    default:
      return 'Unknown verification level.';
  }
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
  loadingText: {
    marginTop: 12,
    fontSize: 14,
    color: theme.colors.textSecondary,
  },
  errorContainer: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    padding: 24,
    backgroundColor: theme.colors.background,
  },
  errorText: {
    fontSize: 14,
    color: theme.colors.error,
    textAlign: 'center',
    marginBottom: 16,
  },
  retryButton: {
    backgroundColor: theme.colors.primary,
    paddingHorizontal: 24,
    paddingVertical: 12,
    borderRadius: 8,
  },
  retryText: {
    color: theme.colors.primaryText,
    fontSize: 14,
    fontWeight: '600',
  },
  rejectedBanner: {
    backgroundColor: theme.colors.errorBackground,
    padding: 16,
    marginBottom: 16,
  },
  rejectedText: {
    fontSize: 16,
    fontWeight: '600',
    color: theme.colors.errorDark,
  },
  rejectionReason: {
    fontSize: 14,
    color: theme.colors.errorDark,
    marginTop: 4,
  },
  section: {
    padding: 16,
    paddingBottom: 0,
  },
  sectionTitle: {
    fontSize: 14,
    fontWeight: '600',
    color: theme.colors.textSecondary,
    marginBottom: 8,
    textTransform: 'uppercase',
    letterSpacing: 0.5,
  },
  infoCard: {
    backgroundColor: theme.colors.card,
    borderRadius: 12,
    padding: 16,
    shadowColor: theme.mode === 'dark' ? '#000' : '#000',
    shadowOffset: { width: 0, height: 1 },
    shadowOpacity: theme.mode === 'dark' ? 0.3 : 0.05,
    shadowRadius: 4,
    elevation: 2,
  },
  infoRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    paddingVertical: 8,
    borderBottomWidth: 1,
    borderBottomColor: theme.colors.borderLight,
  },
  infoLabel: {
    fontSize: 14,
    color: theme.colors.textSecondary,
  },
  infoValue: {
    fontSize: 14,
    color: theme.colors.text,
    fontWeight: '500',
  },
  monospace: {
    fontFamily: 'monospace',
    fontSize: 12,
  },
  levelBadge: {
    paddingHorizontal: 10,
    paddingVertical: 4,
    borderRadius: 12,
  },
  levelText: {
    fontSize: 12,
    fontWeight: '600',
    color: theme.colors.text,
  },
  statusText: {
    fontSize: 14,
    color: theme.colors.primary,
    fontWeight: '500',
  },
  verified: {
    color: theme.colors.success,
    fontWeight: '600',
  },
  pending: {
    color: theme.colors.textSecondary,
  },
  warning: {
    color: theme.colors.warning,
  },
  descriptionCard: {
    backgroundColor: theme.colors.infoBackground,
    borderRadius: 12,
    padding: 16,
  },
  descriptionText: {
    fontSize: 14,
    color: theme.colors.text,
    lineHeight: 20,
  },
  actions: {
    padding: 16,
    gap: 12,
    marginTop: 8,
    marginBottom: 32,
  },
  vouchButton: {
    backgroundColor: theme.colors.success,
    borderRadius: 12,
    padding: 16,
    alignItems: 'center',
  },
  vouchButtonText: {
    color: theme.colors.primaryText,
    fontSize: 16,
    fontWeight: '600',
  },
  rejectButton: {
    backgroundColor: theme.colors.card,
    borderRadius: 12,
    padding: 16,
    alignItems: 'center',
    borderWidth: 2,
    borderColor: theme.colors.error,
  },
  rejectButtonText: {
    color: theme.colors.error,
    fontSize: 16,
    fontWeight: '600',
  },
  disabledButton: {
    opacity: 0.5,
  },
  modalOverlay: {
    flex: 1,
    backgroundColor: 'rgba(0, 0, 0, 0.5)',
    justifyContent: 'flex-end',
  },
  modalContent: {
    backgroundColor: theme.colors.surface,
    borderTopLeftRadius: 20,
    borderTopRightRadius: 20,
    padding: 24,
  },
  modalTitle: {
    fontSize: 20,
    fontWeight: '600',
    color: theme.colors.text,
    marginBottom: 8,
  },
  modalSubtitle: {
    fontSize: 14,
    color: theme.colors.textSecondary,
    marginBottom: 16,
  },
  reasonInput: {
    backgroundColor: theme.colors.background,
    borderRadius: 12,
    padding: 16,
    fontSize: 14,
    minHeight: 100,
    marginBottom: 16,
    color: theme.colors.text,
  },
  modalActions: {
    flexDirection: 'row',
    gap: 12,
  },
  cancelButton: {
    flex: 1,
    backgroundColor: theme.colors.background,
    borderRadius: 12,
    padding: 16,
    alignItems: 'center',
  },
  cancelButtonText: {
    color: theme.colors.textSecondary,
    fontSize: 16,
    fontWeight: '600',
  },
  confirmRejectButton: {
    flex: 1,
    backgroundColor: theme.colors.error,
    borderRadius: 12,
    padding: 16,
    alignItems: 'center',
  },
  confirmRejectText: {
    color: theme.colors.primaryText,
    fontSize: 16,
    fontWeight: '600',
  },
});
