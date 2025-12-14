/**
 * Verify Screen (Scanner Mode)
 *
 * Scans and verifies SDIS identity proofs from QR codes.
 */

import React, { useState, useCallback, useEffect } from 'react';
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  ActivityIndicator,
  Dimensions,
  Platform,
} from 'react-native';
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
import { useTheme, Theme } from '../contexts/ThemeContext';

type Props = NativeStackScreenProps<RootStackParamList, 'Verify'>;

const { width: SCREEN_WIDTH } = Dimensions.get('window');
const SCANNER_SIZE = SCREEN_WIDTH * 0.75;

export function VerifyScreen({ navigation }: Props) {
  const { theme } = useTheme();
  const styles = createStyles(theme);
  const { verify, result, isVerifying, error, reset, history } =
    useSdisVerifierWithHistory(client!);

  const [verificationLevel, setVerificationLevel] = useState<VerificationLevel>(1);
  const [hasPermission, setHasPermission] = useState<boolean | null>(null);
  const [isScanning, setIsScanning] = useState(true);
  const [lastScannedData, setLastScannedData] = useState<string | null>(null);
  const [CameraView, setCameraView] = useState<any>(null);

  // Load expo-camera dynamically
  useEffect(() => {
    if (Platform.OS !== 'web') {
      import('expo-camera').then(async (module: any) => {
        setCameraView(() => module.CameraView);
        try {
          const requestPermission = module.Camera?.requestCameraPermissionsAsync || module.requestCameraPermissionsAsync;
          if (requestPermission) {
            const { status } = await requestPermission();
            setHasPermission(status === 'granted');
          }
        } catch (err) {
          console.error('VerifyScreen: Permission error:', err);
          setHasPermission(false);
        }
      }).catch((err) => {
        console.error('Failed to load expo-camera:', err);
        setHasPermission(false);
      });
    }
  }, []);

  // Handle barcode scan
  const handleBarCodeScanned = useCallback(
    ({ data }: { data: string }) => {
      if (!isScanning || isVerifying) return;

      // Prevent duplicate scans
      if (data === lastScannedData) return;
      setLastScannedData(data);

      // Check if it's an SDIS QR code
      if (!isSdisQR(data)) {
        return;
      }

      // Parse and verify
      const parsed = parseSdisQR(data);
      if (parsed) {
        setIsScanning(false);
        verify(parsed.raw, verificationLevel, parsed.proofInfo?.proofTypeLabel);
      }
    },
    [isScanning, isVerifying, lastScannedData, verificationLevel, verify],
  );

  const handleRescan = () => {
    reset();
    setLastScannedData(null);
    setIsScanning(true);
  };

  const handleViewHistory = () => {
    navigation.navigate('VerificationHistory');
  };

  // Permission states
  if (hasPermission === null || !CameraView) {
    return (
      <View style={styles.centered}>
        <ActivityIndicator size="large" color={theme.colors.primary} />
        <Text style={styles.message}>Requesting camera permission...</Text>
      </View>
    );
  }

  if (hasPermission === false) {
    return (
      <View style={styles.centered}>
        <Text style={styles.icon}>📷</Text>
        <Text style={styles.message}>Camera permission is required to scan QR codes.</Text>
        <TouchableOpacity
          style={styles.primaryButton}
          onPress={() => {
            import('expo-camera').then(async (module: any) => {
              const requestPermission = module.Camera?.requestCameraPermissionsAsync || module.requestCameraPermissionsAsync;
              if (requestPermission) {
                const { status } = await requestPermission();
                setHasPermission(status === 'granted');
              }
            });
          }}
        >
          <Text style={styles.primaryButtonText}>Grant Permission</Text>
        </TouchableOpacity>
      </View>
    );
  }

  return (
    <View style={styles.container}>
      {/* Camera View */}
      <View style={styles.cameraContainer}>
        {isScanning ? (
          <>
            <CameraView
              style={StyleSheet.absoluteFill}
              facing="back"
              onBarcodeScanned={handleBarCodeScanned}
              barcodeScannerSettings={{
                barcodeTypes: ['qr'],
              }}
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
                <ActivityIndicator size="large" color={theme.colors.primary} />
                <Text style={styles.resultText}>Verifying...</Text>
              </>
            ) : result ? (
              <>
                <View
                  style={[
                    styles.resultIcon,
                    { backgroundColor: result.valid ? theme.colors.success : theme.colors.error },
                  ]}
                >
                  <Text style={styles.resultIconText}>{result.valid ? '\u2713' : '\u2717'}</Text>
                </View>
                <Text
                  style={[styles.resultTitle, { color: result.valid ? theme.colors.success : theme.colors.error }]}
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

const createStyles = (theme: Theme) => StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#000',
  },
  centered: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    backgroundColor: theme.colors.background,
    padding: 32,
  },
  icon: {
    fontSize: 64,
    marginBottom: 16,
  },
  message: {
    fontSize: 16,
    color: theme.colors.textSecondary,
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
    borderColor: theme.colors.primary,
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
    backgroundColor: theme.colors.background,
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
    color: theme.colors.textSecondary,
    marginTop: 16,
  },
  resultDetails: {
    fontSize: 16,
    color: theme.colors.textSecondary,
    textAlign: 'center',
  },
  warningsContainer: {
    marginTop: 16,
    padding: 12,
    backgroundColor: theme.mode === 'dark' ? 'rgba(255, 152, 0, 0.2)' : '#FFF3E0',
    borderRadius: 8,
  },
  warningText: {
    fontSize: 14,
    color: theme.colors.warning,
  },
  errorText: {
    fontSize: 14,
    color: theme.colors.error,
    textAlign: 'center',
  },
  controls: {
    backgroundColor: theme.colors.background,
    padding: 16,
  },
  levelSelector: {
    marginBottom: 16,
  },
  levelLabel: {
    fontSize: 12,
    color: theme.colors.textSecondary,
    marginBottom: 8,
  },
  levelButtons: {
    flexDirection: 'row',
    gap: 12,
  },
  levelButton: {
    flex: 1,
    backgroundColor: theme.colors.card,
    padding: 12,
    borderRadius: 8,
    alignItems: 'center',
    borderWidth: 2,
    borderColor: theme.colors.border,
  },
  levelButtonActive: {
    borderColor: theme.colors.primary,
    backgroundColor: theme.mode === 'dark' ? 'rgba(74, 144, 164, 0.2)' : '#E3F2FD',
  },
  levelButtonText: {
    fontSize: 14,
    fontWeight: '600',
    color: theme.colors.text,
  },
  levelButtonTextActive: {
    color: theme.colors.primary,
  },
  levelButtonDesc: {
    fontSize: 11,
    color: theme.colors.textMuted,
    marginTop: 2,
  },
  actionButtons: {
    gap: 12,
  },
  primaryButton: {
    backgroundColor: theme.colors.primary,
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
    backgroundColor: theme.colors.card,
    padding: 16,
    borderRadius: 8,
    alignItems: 'center',
    borderWidth: 1,
    borderColor: theme.colors.border,
  },
  secondaryButtonText: {
    color: theme.colors.textSecondary,
    fontSize: 16,
  },
});
