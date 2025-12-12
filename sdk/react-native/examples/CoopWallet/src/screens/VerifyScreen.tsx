/**
 * Verify Screen (Scanner Mode)
 *
 * Scans and verifies SDIS identity proofs from QR codes.
 */

import React, { useState, useCallback } from 'react';
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  ActivityIndicator,
  Dimensions,
} from 'react-native';
import { Camera, useCameraDevice, useCodeScanner } from 'react-native-vision-camera';
import { NativeStackScreenProps } from '@react-navigation/native-stack';
import {
  useSdisVerifierWithHistory,
  parseSdisQR,
  isSdisQR,
  VerificationLevel,
  formatVerificationResult,
} from '@icn/react-native';
import { client } from '../client';
import { RootStackParamList } from '../../App';

type Props = NativeStackScreenProps<RootStackParamList, 'Verify'>;

const { width: SCREEN_WIDTH } = Dimensions.get('window');
const SCANNER_SIZE = SCREEN_WIDTH * 0.75;

export function VerifyScreen({ navigation }: Props) {
  const { verify, result, isVerifying, error, reset, history, clearHistory } =
    useSdisVerifierWithHistory(client!);

  const [verificationLevel, setVerificationLevel] = useState<VerificationLevel>(1);
  const [hasPermission, setHasPermission] = useState<boolean | null>(null);
  const [isScanning, setIsScanning] = useState(true);
  const [lastScannedData, setLastScannedData] = useState<string | null>(null);

  const device = useCameraDevice('back');

  // Request camera permission
  React.useEffect(() => {
    Camera.requestCameraPermission().then((status) => {
      setHasPermission(status === 'granted');
    });
  }, []);

  // Code scanner callback
  const codeScanner = useCodeScanner({
    codeTypes: ['qr'],
    onCodeScanned: useCallback(
      (codes) => {
        if (!isScanning || isVerifying || codes.length === 0) return;

        const qrData = codes[0].value;
        if (!qrData) return;

        // Prevent duplicate scans
        if (qrData === lastScannedData) return;
        setLastScannedData(qrData);

        // Check if it's an SDIS QR code
        if (!isSdisQR(qrData)) {
          return;
        }

        // Parse and verify
        const parsed = parseSdisQR(qrData);
        if (parsed) {
          setIsScanning(false);
          verify(parsed.raw, verificationLevel, parsed.proofInfo?.proofTypeLabel);
        }
      },
      [isScanning, isVerifying, lastScannedData, verificationLevel, verify],
    ),
  });

  const handleRescan = () => {
    reset();
    setLastScannedData(null);
    setIsScanning(true);
  };

  const handleViewHistory = () => {
    navigation.navigate('VerificationHistory');
  };

  // Permission states
  if (hasPermission === null) {
    return (
      <View style={styles.centered}>
        <ActivityIndicator size="large" color="#4A90A4" />
        <Text style={styles.message}>Requesting camera permission...</Text>
      </View>
    );
  }

  if (hasPermission === false) {
    return (
      <View style={styles.centered}>
        <Text style={styles.icon}>&#128247;</Text>
        <Text style={styles.message}>Camera permission is required to scan QR codes.</Text>
        <TouchableOpacity
          style={styles.primaryButton}
          onPress={() => Camera.requestCameraPermission()}
        >
          <Text style={styles.primaryButtonText}>Grant Permission</Text>
        </TouchableOpacity>
      </View>
    );
  }

  if (!device) {
    return (
      <View style={styles.centered}>
        <Text style={styles.message}>No camera device found.</Text>
      </View>
    );
  }

  return (
    <View style={styles.container}>
      {/* Camera View */}
      <View style={styles.cameraContainer}>
        {isScanning ? (
          <>
            <Camera
              style={StyleSheet.absoluteFill}
              device={device}
              isActive={isScanning}
              codeScanner={codeScanner}
            />
            {/* Scanner Overlay */}
            <View style={styles.overlay}>
              <View style={styles.scannerFrame}>
                <View style={[styles.corner, styles.topLeft]} />
                <View style={[styles.corner, styles.topRight]} />
                <View style={[styles.corner, styles.bottomLeft]} />
                <View style={[styles.corner, styles.bottomRight]} />
              </View>
              <Text style={styles.scanHint}>Position QR code within frame</Text>
            </View>
          </>
        ) : (
          /* Result Display */
          <View style={styles.resultContainer}>
            {isVerifying ? (
              <>
                <ActivityIndicator size="large" color="#4A90A4" />
                <Text style={styles.resultText}>Verifying...</Text>
              </>
            ) : result ? (
              <>
                <View
                  style={[
                    styles.resultIcon,
                    { backgroundColor: result.valid ? '#4CAF50' : '#F44336' },
                  ]}
                >
                  <Text style={styles.resultIconText}>{result.valid ? '\u2713' : '\u2717'}</Text>
                </View>
                <Text
                  style={[styles.resultTitle, { color: result.valid ? '#4CAF50' : '#F44336' }]}
                >
                  {result.valid ? 'VALID' : 'INVALID'}
                </Text>
                <Text style={styles.resultDetails}>
                  {formatVerificationResult(result.valid, result.level, result.proof_type)}
                </Text>
                {result.warnings.length > 0 && (
                  <View style={styles.warningsContainer}>
                    {result.warnings.map((warning, i) => (
                      <Text key={i} style={styles.warningText}>
                        {warning}
                      </Text>
                    ))}
                  </View>
                )}
              </>
            ) : error ? (
              <>
                <View style={[styles.resultIcon, { backgroundColor: '#FF9800' }]}>
                  <Text style={styles.resultIconText}>!</Text>
                </View>
                <Text style={styles.resultTitle}>Error</Text>
                <Text style={styles.errorText}>{error}</Text>
              </>
            ) : null}
          </View>
        )}
      </View>

      {/* Controls */}
      <View style={styles.controls}>
        {/* Verification Level Selector */}
        <View style={styles.levelSelector}>
          <Text style={styles.levelLabel}>Verification Level</Text>
          <View style={styles.levelButtons}>
            <TouchableOpacity
              style={[styles.levelButton, verificationLevel === 1 && styles.levelButtonActive]}
              onPress={() => setVerificationLevel(1)}
            >
              <Text
                style={[
                  styles.levelButtonText,
                  verificationLevel === 1 && styles.levelButtonTextActive,
                ]}
              >
                Level 1
              </Text>
              <Text style={styles.levelButtonDesc}>QR Only</Text>
            </TouchableOpacity>
            <TouchableOpacity
              style={[styles.levelButton, verificationLevel === 2 && styles.levelButtonActive]}
              onPress={() => setVerificationLevel(2)}
            >
              <Text
                style={[
                  styles.levelButtonText,
                  verificationLevel === 2 && styles.levelButtonTextActive,
                ]}
              >
                Level 2
              </Text>
              <Text style={styles.levelButtonDesc}>+ Binding</Text>
            </TouchableOpacity>
          </View>
        </View>

        {/* Action Buttons */}
        <View style={styles.actionButtons}>
          {!isScanning && (
            <TouchableOpacity style={styles.primaryButton} onPress={handleRescan}>
              <Text style={styles.primaryButtonText}>Scan Again</Text>
            </TouchableOpacity>
          )}
          <TouchableOpacity style={styles.secondaryButton} onPress={handleViewHistory}>
            <Text style={styles.secondaryButtonText}>View History ({history.length})</Text>
          </TouchableOpacity>
        </View>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#000',
  },
  centered: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    backgroundColor: '#f5f5f5',
    padding: 32,
  },
  icon: {
    fontSize: 64,
    marginBottom: 16,
  },
  message: {
    fontSize: 16,
    color: '#666',
    textAlign: 'center',
    marginTop: 16,
  },
  cameraContainer: {
    flex: 1,
    position: 'relative',
  },
  overlay: {
    ...StyleSheet.absoluteFillObject,
    justifyContent: 'center',
    alignItems: 'center',
  },
  scannerFrame: {
    width: SCANNER_SIZE,
    height: SCANNER_SIZE,
    position: 'relative',
  },
  corner: {
    position: 'absolute',
    width: 40,
    height: 40,
    borderColor: '#4A90A4',
  },
  topLeft: {
    top: 0,
    left: 0,
    borderTopWidth: 4,
    borderLeftWidth: 4,
  },
  topRight: {
    top: 0,
    right: 0,
    borderTopWidth: 4,
    borderRightWidth: 4,
  },
  bottomLeft: {
    bottom: 0,
    left: 0,
    borderBottomWidth: 4,
    borderLeftWidth: 4,
  },
  bottomRight: {
    bottom: 0,
    right: 0,
    borderBottomWidth: 4,
    borderRightWidth: 4,
  },
  scanHint: {
    color: 'white',
    fontSize: 14,
    marginTop: 24,
    textShadowColor: 'rgba(0,0,0,0.5)',
    textShadowOffset: { width: 0, height: 1 },
    textShadowRadius: 2,
  },
  resultContainer: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    backgroundColor: '#f5f5f5',
    padding: 32,
  },
  resultIcon: {
    width: 80,
    height: 80,
    borderRadius: 40,
    justifyContent: 'center',
    alignItems: 'center',
    marginBottom: 16,
  },
  resultIconText: {
    fontSize: 40,
    color: 'white',
    fontWeight: 'bold',
  },
  resultTitle: {
    fontSize: 28,
    fontWeight: 'bold',
    marginBottom: 8,
  },
  resultText: {
    fontSize: 16,
    color: '#666',
    marginTop: 16,
  },
  resultDetails: {
    fontSize: 16,
    color: '#666',
    textAlign: 'center',
  },
  warningsContainer: {
    marginTop: 16,
    padding: 12,
    backgroundColor: '#FFF3E0',
    borderRadius: 8,
  },
  warningText: {
    fontSize: 14,
    color: '#E65100',
  },
  errorText: {
    fontSize: 14,
    color: '#c62828',
    textAlign: 'center',
  },
  controls: {
    backgroundColor: '#f5f5f5',
    padding: 16,
  },
  levelSelector: {
    marginBottom: 16,
  },
  levelLabel: {
    fontSize: 12,
    color: '#666',
    marginBottom: 8,
  },
  levelButtons: {
    flexDirection: 'row',
    gap: 12,
  },
  levelButton: {
    flex: 1,
    backgroundColor: 'white',
    padding: 12,
    borderRadius: 8,
    alignItems: 'center',
    borderWidth: 2,
    borderColor: '#ddd',
  },
  levelButtonActive: {
    borderColor: '#4A90A4',
    backgroundColor: '#E3F2FD',
  },
  levelButtonText: {
    fontSize: 14,
    fontWeight: '600',
    color: '#333',
  },
  levelButtonTextActive: {
    color: '#4A90A4',
  },
  levelButtonDesc: {
    fontSize: 11,
    color: '#999',
    marginTop: 2,
  },
  actionButtons: {
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
});
