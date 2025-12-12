/**
 * Identity Screen (Wallet Mode)
 *
 * Displays the user's SDIS identity and allows generating ephemeral proofs.
 */

import React, { useState } from 'react';
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  ScrollView,
  ActivityIndicator,
  Modal,
  Pressable,
} from 'react-native';
import QRCode from 'react-native-qrcode-svg';
import { NativeStackScreenProps } from '@react-navigation/native-stack';
import {
  useSdisProof,
  useSdisHealth,
  ProofType,
  PROOF_PRESETS,
  formatProofType,
} from '@icn/react-native';
import { client } from '../client';
import { RootStackParamList } from '../../App';

type Props = NativeStackScreenProps<RootStackParamList, 'Identity'>;

const PROOF_OPTIONS: { label: string; type: ProofType }[] = [
  { label: 'Age 18+', type: PROOF_PRESETS.AGE_18 },
  { label: 'Age 21+', type: PROOF_PRESETS.AGE_21 },
  { label: 'Age 25+', type: PROOF_PRESETS.AGE_25 },
  { label: 'Age 65+', type: PROOF_PRESETS.AGE_65 },
  { label: 'US Citizen', type: PROOF_PRESETS.CITIZENSHIP_US },
  { label: 'Not Revoked', type: PROOF_PRESETS.NON_REVOCATION },
];

const VALIDITY_OPTIONS = [
  { label: '15 minutes', secs: 900 },
  { label: '1 hour', secs: 3600 },
  { label: '4 hours', secs: 14400 },
  { label: '24 hours', secs: 86400 },
];

export function IdentityScreen({ navigation }: Props) {
  const { proof, qrData, proofType, isGenerating, error, timeRemaining, isExpired, generate, clear } =
    useSdisProof(client!);
  const { isAvailable, isChecking, check } = useSdisHealth(client!);

  const [showProofPicker, setShowProofPicker] = useState(false);
  const [showValidityPicker, setShowValidityPicker] = useState(false);
  const [selectedProofType, setSelectedProofType] = useState<ProofType>(PROOF_PRESETS.AGE_21);
  const [selectedValidity, setSelectedValidity] = useState(3600);

  const handleGenerate = async () => {
    await generate(selectedProofType, selectedValidity);
  };

  const handleSelectProofType = (type: ProofType) => {
    setSelectedProofType(type);
    setShowProofPicker(false);
  };

  const handleSelectValidity = (secs: number) => {
    setSelectedValidity(secs);
    setShowValidityPicker(false);
  };

  const getValidityLabel = () => {
    const option = VALIDITY_OPTIONS.find((o) => o.secs === selectedValidity);
    return option?.label || '1 hour';
  };

  const getProofTypeLabel = () => {
    const option = PROOF_OPTIONS.find(
      (o) =>
        o.type.type === selectedProofType.type &&
        o.type.threshold === selectedProofType.threshold &&
        o.type.country_code === selectedProofType.country_code,
    );
    return option?.label || formatProofType(selectedProofType);
  };

  return (
    <ScrollView style={styles.container}>
      {/* Status Bar */}
      <View style={styles.statusBar}>
        <View style={styles.statusItem}>
          <View style={[styles.statusDot, { backgroundColor: isAvailable ? '#4CAF50' : '#F44336' }]} />
          <Text style={styles.statusText}>SDIS {isAvailable ? 'Online' : 'Offline'}</Text>
        </View>
        <TouchableOpacity onPress={check} disabled={isChecking}>
          <Text style={styles.refreshLink}>{isChecking ? '...' : 'Refresh'}</Text>
        </TouchableOpacity>
      </View>

      {/* QR Code Display */}
      <View style={styles.qrContainer}>
        {isGenerating ? (
          <View style={styles.qrPlaceholder}>
            <ActivityIndicator size="large" color="#4A90A4" />
            <Text style={styles.qrPlaceholderText}>Generating proof...</Text>
          </View>
        ) : qrData && !isExpired ? (
          <View style={styles.qrWrapper}>
            <QRCode value={qrData} size={220} backgroundColor="white" color="black" />
            <View style={styles.expiryBadge}>
              <Text style={styles.expiryText}>{timeRemaining}</Text>
            </View>
          </View>
        ) : (
          <View style={styles.qrPlaceholder}>
            <Text style={styles.qrPlaceholderIcon}>ID</Text>
            <Text style={styles.qrPlaceholderText}>
              {isExpired ? 'Proof Expired' : 'Generate a proof to display QR'}
            </Text>
          </View>
        )}
      </View>

      {/* Proof Info */}
      {proofType && !isExpired && (
        <View style={styles.proofInfo}>
          <Text style={styles.proofInfoLabel}>Proving</Text>
          <Text style={styles.proofInfoValue}>{formatProofType(proofType)}</Text>
        </View>
      )}

      {/* Error Display */}
      {error && (
        <View style={styles.errorContainer}>
          <Text style={styles.errorText}>{error}</Text>
        </View>
      )}

      {/* Proof Type Selector */}
      <Text style={styles.sectionLabel}>Proof Type</Text>
      <TouchableOpacity style={styles.selector} onPress={() => setShowProofPicker(true)}>
        <Text style={styles.selectorText}>{getProofTypeLabel()}</Text>
        <Text style={styles.selectorArrow}>▼</Text>
      </TouchableOpacity>

      {/* Validity Selector */}
      <Text style={styles.sectionLabel}>Valid For</Text>
      <TouchableOpacity style={styles.selector} onPress={() => setShowValidityPicker(true)}>
        <Text style={styles.selectorText}>{getValidityLabel()}</Text>
        <Text style={styles.selectorArrow}>▼</Text>
      </TouchableOpacity>

      {/* Action Buttons */}
      <View style={styles.buttonContainer}>
        <TouchableOpacity
          style={[styles.primaryButton, (!isAvailable || isGenerating) && styles.disabledButton]}
          onPress={handleGenerate}
          disabled={!isAvailable || isGenerating}
        >
          <Text style={styles.primaryButtonText}>
            {isGenerating ? 'Generating...' : qrData ? 'Regenerate Proof' : 'Generate Proof'}
          </Text>
        </TouchableOpacity>

        {qrData && (
          <TouchableOpacity style={styles.secondaryButton} onPress={clear}>
            <Text style={styles.secondaryButtonText}>Clear</Text>
          </TouchableOpacity>
        )}
      </View>

      {/* Proof Type Picker Modal */}
      <Modal visible={showProofPicker} transparent animationType="slide">
        <View style={styles.modalOverlay}>
          <View style={styles.modalContent}>
            <Text style={styles.modalTitle}>Select Proof Type</Text>
            {PROOF_OPTIONS.map((option) => (
              <Pressable
                key={option.label}
                style={styles.modalOption}
                onPress={() => handleSelectProofType(option.type)}
              >
                <Text style={styles.modalOptionText}>{option.label}</Text>
              </Pressable>
            ))}
            <TouchableOpacity style={styles.modalCancel} onPress={() => setShowProofPicker(false)}>
              <Text style={styles.modalCancelText}>Cancel</Text>
            </TouchableOpacity>
          </View>
        </View>
      </Modal>

      {/* Validity Picker Modal */}
      <Modal visible={showValidityPicker} transparent animationType="slide">
        <View style={styles.modalOverlay}>
          <View style={styles.modalContent}>
            <Text style={styles.modalTitle}>Valid For</Text>
            {VALIDITY_OPTIONS.map((option) => (
              <Pressable
                key={option.secs}
                style={styles.modalOption}
                onPress={() => handleSelectValidity(option.secs)}
              >
                <Text style={styles.modalOptionText}>{option.label}</Text>
              </Pressable>
            ))}
            <TouchableOpacity style={styles.modalCancel} onPress={() => setShowValidityPicker(false)}>
              <Text style={styles.modalCancelText}>Cancel</Text>
            </TouchableOpacity>
          </View>
        </View>
      </Modal>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#f5f5f5',
    padding: 16,
  },
  statusBar: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 16,
    padding: 12,
    backgroundColor: 'white',
    borderRadius: 8,
  },
  statusItem: {
    flexDirection: 'row',
    alignItems: 'center',
  },
  statusDot: {
    width: 10,
    height: 10,
    borderRadius: 5,
    marginRight: 8,
  },
  statusText: {
    fontSize: 14,
    color: '#333',
  },
  refreshLink: {
    color: '#4A90A4',
    fontSize: 14,
  },
  qrContainer: {
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: 'white',
    borderRadius: 16,
    padding: 24,
    marginBottom: 16,
    minHeight: 280,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 2 },
    shadowOpacity: 0.1,
    shadowRadius: 4,
    elevation: 3,
  },
  qrWrapper: {
    alignItems: 'center',
  },
  qrPlaceholder: {
    alignItems: 'center',
    justifyContent: 'center',
  },
  qrPlaceholderIcon: {
    fontSize: 48,
    color: '#ccc',
    marginBottom: 12,
    fontWeight: 'bold',
  },
  qrPlaceholderText: {
    fontSize: 14,
    color: '#999',
    textAlign: 'center',
  },
  expiryBadge: {
    marginTop: 16,
    paddingHorizontal: 16,
    paddingVertical: 8,
    backgroundColor: '#4A90A4',
    borderRadius: 20,
  },
  expiryText: {
    color: 'white',
    fontSize: 16,
    fontWeight: '600',
  },
  proofInfo: {
    backgroundColor: 'white',
    padding: 16,
    borderRadius: 8,
    marginBottom: 16,
    alignItems: 'center',
  },
  proofInfoLabel: {
    fontSize: 12,
    color: '#666',
    marginBottom: 4,
  },
  proofInfoValue: {
    fontSize: 18,
    fontWeight: '600',
    color: '#333',
  },
  errorContainer: {
    backgroundColor: '#ffebee',
    padding: 12,
    borderRadius: 8,
    marginBottom: 16,
  },
  errorText: {
    color: '#c62828',
    fontSize: 14,
  },
  sectionLabel: {
    fontSize: 12,
    color: '#666',
    marginBottom: 8,
    marginLeft: 4,
  },
  selector: {
    backgroundColor: 'white',
    padding: 16,
    borderRadius: 8,
    marginBottom: 16,
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  selectorText: {
    fontSize: 16,
    color: '#333',
  },
  selectorArrow: {
    fontSize: 12,
    color: '#999',
  },
  buttonContainer: {
    marginTop: 8,
    gap: 12,
  },
  primaryButton: {
    backgroundColor: '#4A90A4',
    padding: 16,
    borderRadius: 8,
    alignItems: 'center',
  },
  primaryButtonText: {
    color: 'white',
    fontSize: 16,
    fontWeight: '600',
  },
  secondaryButton: {
    backgroundColor: 'white',
    padding: 16,
    borderRadius: 8,
    alignItems: 'center',
    borderWidth: 1,
    borderColor: '#ddd',
  },
  secondaryButtonText: {
    color: '#666',
    fontSize: 16,
  },
  disabledButton: {
    backgroundColor: '#ccc',
  },
  modalOverlay: {
    flex: 1,
    backgroundColor: 'rgba(0,0,0,0.5)',
    justifyContent: 'flex-end',
  },
  modalContent: {
    backgroundColor: 'white',
    borderTopLeftRadius: 16,
    borderTopRightRadius: 16,
    padding: 16,
  },
  modalTitle: {
    fontSize: 18,
    fontWeight: '600',
    marginBottom: 16,
    textAlign: 'center',
  },
  modalOption: {
    padding: 16,
    borderBottomWidth: 1,
    borderBottomColor: '#eee',
  },
  modalOptionText: {
    fontSize: 16,
    color: '#333',
  },
  modalCancel: {
    padding: 16,
    marginTop: 8,
    alignItems: 'center',
  },
  modalCancelText: {
    fontSize: 16,
    color: '#666',
  },
});
