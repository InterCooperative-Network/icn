import React, { useState, useEffect, useContext } from 'react';
import { View, StyleSheet, Alert, Dimensions } from 'react-native';
import { BarCodeScanner } from 'expo-barcode-scanner';
import { Button, Text, ActivityIndicator, Surface } from 'react-native-paper';
import { SecureStorage } from '../services/SecureStorage';

interface QRData {
  session_id: string;
  gateway_url: string;
  coop_id: string;
  expires_at: number;
}

interface Props {
  navigation: any;
}

export default function QRScannerScreen({ navigation }: Props) {
  const [hasPermission, setHasPermission] = useState<boolean | null>(null);
  const [scanned, setScanned] = useState(false);
  const [loading, setLoading] = useState(false);
  const [statusMessage, setStatusMessage] = useState('');

  useEffect(() => {
    requestCameraPermission();
  }, []);

  const requestCameraPermission = async () => {
    const { status } = await BarCodeScanner.requestPermissionsAsync();
    setHasPermission(status === 'granted');
  };

  const handleBarCodeScanned = async ({ type, data }: { type: string; data: string }) => {
    if (scanned || loading) return;
    setScanned(true);
    setLoading(true);
    setStatusMessage('Processing...');

    try {
      // Parse QR data
      let qrData: QRData;
      try {
        qrData = JSON.parse(data);
      } catch (e) {
        throw new Error('Invalid QR code format. Please scan a valid ICN login code.');
      }

      // Validate required fields
      if (!qrData.session_id || !qrData.gateway_url || !qrData.coop_id) {
        throw new Error('Invalid QR code. Missing required fields.');
      }

      // Validate expiration
      const now = Date.now();
      if (now > qrData.expires_at * 1000) {
        throw new Error('This login session has expired. Please generate a new QR code on the web app.');
      }

      setStatusMessage('Checking authentication...');

      // Get stored token from secure storage
      const token = await SecureStorage.getAuthToken();
      if (!token) {
        Alert.alert(
          'Not Logged In',
          'You need to be logged in to approve web logins. Please log in first.',
          [{ text: 'OK', onPress: () => navigation.goBack() }]
        );
        return;
      }

      setStatusMessage('Approving web login...');

      // Approve the session
      const response = await fetch(
        `${qrData.gateway_url}/v1/sessions/${qrData.session_id}/approve`,
        {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            'Authorization': `Bearer ${token}`,
          },
          body: JSON.stringify({}),
        }
      );

      if (!response.ok) {
        const errorData = await response.json().catch(() => ({}));

        if (response.status === 401) {
          throw new Error('Your session has expired. Please log in again.');
        } else if (response.status === 404) {
          throw new Error('Session not found. It may have expired or already been used.');
        } else if (response.status === 400) {
          throw new Error(errorData.error || 'Session cannot be approved. It may have already been used.');
        } else {
          throw new Error(errorData.error || 'Failed to approve login session.');
        }
      }

      // Success!
      Alert.alert(
        'Login Approved!',
        `Web login for ${qrData.coop_id} has been approved. You can now use the web app.`,
        [{ text: 'Done', onPress: () => navigation.goBack() }]
      );

    } catch (error: any) {
      console.error('QR scan error:', error);
      Alert.alert(
        'Error',
        error.message || 'Failed to process QR code. Please try again.',
        [
          { text: 'Try Again', onPress: () => setScanned(false) },
          { text: 'Cancel', onPress: () => navigation.goBack() },
        ]
      );
    } finally {
      setLoading(false);
      setStatusMessage('');
    }
  };

  if (hasPermission === null) {
    return (
      <View style={styles.container}>
        <ActivityIndicator size="large" color="#2563eb" />
        <Text style={styles.statusText}>Requesting camera permission...</Text>
      </View>
    );
  }

  if (hasPermission === false) {
    return (
      <View style={styles.container}>
        <Surface style={styles.permissionCard}>
          <Text style={styles.permissionTitle}>Camera Permission Required</Text>
          <Text style={styles.permissionText}>
            To scan QR codes for web login, please grant camera access.
          </Text>
          <Button
            mode="contained"
            onPress={requestCameraPermission}
            style={styles.permissionButton}
          >
            Grant Permission
          </Button>
          <Button
            mode="outlined"
            onPress={() => navigation.goBack()}
            style={styles.cancelButton}
          >
            Cancel
          </Button>
        </Surface>
      </View>
    );
  }

  return (
    <View style={styles.container}>
      <BarCodeScanner
        onBarCodeScanned={scanned ? undefined : handleBarCodeScanned}
        style={StyleSheet.absoluteFillObject}
      />

      {/* Scan overlay */}
      <View style={styles.overlay}>
        <View style={styles.overlayTop} />
        <View style={styles.overlayMiddle}>
          <View style={styles.overlaySide} />
          <View style={styles.scanArea}>
            <View style={[styles.corner, styles.cornerTopLeft]} />
            <View style={[styles.corner, styles.cornerTopRight]} />
            <View style={[styles.corner, styles.cornerBottomLeft]} />
            <View style={[styles.corner, styles.cornerBottomRight]} />
          </View>
          <View style={styles.overlaySide} />
        </View>
        <View style={styles.overlayBottom} />
      </View>

      {/* Instructions */}
      <View style={styles.instructions}>
        <Surface style={styles.instructionCard}>
          <Text style={styles.instructionTitle}>Scan Web Login QR Code</Text>
          <Text style={styles.instructionText}>
            Point your camera at the QR code displayed on the web login screen.
          </Text>
        </Surface>
      </View>

      {/* Loading overlay */}
      {loading && (
        <View style={styles.loadingOverlay}>
          <ActivityIndicator size="large" color="#ffffff" />
          <Text style={styles.loadingText}>{statusMessage}</Text>
        </View>
      )}

      {/* Footer buttons */}
      <View style={styles.footer}>
        <Button
          mode="outlined"
          onPress={() => navigation.goBack()}
          style={styles.footerButton}
          labelStyle={styles.footerButtonLabel}
        >
          Cancel
        </Button>
        {scanned && !loading && (
          <Button
            mode="contained"
            onPress={() => setScanned(false)}
            style={styles.footerButton}
          >
            Scan Again
          </Button>
        )}
      </View>
    </View>
  );
}

const { width: screenWidth } = Dimensions.get('window');
const scanAreaSize = screenWidth * 0.7;

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#000',
    justifyContent: 'center',
    alignItems: 'center',
  },
  statusText: {
    color: '#fff',
    marginTop: 16,
    fontSize: 16,
  },

  // Permission denied styles
  permissionCard: {
    padding: 24,
    margin: 24,
    borderRadius: 12,
    alignItems: 'center',
  },
  permissionTitle: {
    fontSize: 20,
    fontWeight: 'bold',
    marginBottom: 12,
    textAlign: 'center',
  },
  permissionText: {
    fontSize: 16,
    color: '#64748b',
    textAlign: 'center',
    marginBottom: 24,
  },
  permissionButton: {
    width: '100%',
    marginBottom: 12,
  },
  cancelButton: {
    width: '100%',
  },

  // Overlay styles
  overlay: {
    ...StyleSheet.absoluteFillObject,
  },
  overlayTop: {
    flex: 1,
    backgroundColor: 'rgba(0, 0, 0, 0.6)',
  },
  overlayMiddle: {
    flexDirection: 'row',
    height: scanAreaSize,
  },
  overlaySide: {
    flex: 1,
    backgroundColor: 'rgba(0, 0, 0, 0.6)',
  },
  overlayBottom: {
    flex: 1,
    backgroundColor: 'rgba(0, 0, 0, 0.6)',
  },
  scanArea: {
    width: scanAreaSize,
    height: scanAreaSize,
    backgroundColor: 'transparent',
  },
  corner: {
    position: 'absolute',
    width: 24,
    height: 24,
    borderColor: '#2563eb',
  },
  cornerTopLeft: {
    top: 0,
    left: 0,
    borderTopWidth: 4,
    borderLeftWidth: 4,
    borderTopLeftRadius: 8,
  },
  cornerTopRight: {
    top: 0,
    right: 0,
    borderTopWidth: 4,
    borderRightWidth: 4,
    borderTopRightRadius: 8,
  },
  cornerBottomLeft: {
    bottom: 0,
    left: 0,
    borderBottomWidth: 4,
    borderLeftWidth: 4,
    borderBottomLeftRadius: 8,
  },
  cornerBottomRight: {
    bottom: 0,
    right: 0,
    borderBottomWidth: 4,
    borderRightWidth: 4,
    borderBottomRightRadius: 8,
  },

  // Instructions
  instructions: {
    position: 'absolute',
    top: 60,
    left: 20,
    right: 20,
  },
  instructionCard: {
    padding: 16,
    borderRadius: 12,
    backgroundColor: 'rgba(255, 255, 255, 0.95)',
  },
  instructionTitle: {
    fontSize: 18,
    fontWeight: 'bold',
    marginBottom: 8,
    textAlign: 'center',
  },
  instructionText: {
    fontSize: 14,
    color: '#64748b',
    textAlign: 'center',
  },

  // Loading overlay
  loadingOverlay: {
    ...StyleSheet.absoluteFillObject,
    backgroundColor: 'rgba(0, 0, 0, 0.8)',
    justifyContent: 'center',
    alignItems: 'center',
  },
  loadingText: {
    color: '#fff',
    marginTop: 16,
    fontSize: 16,
  },

  // Footer
  footer: {
    position: 'absolute',
    bottom: 40,
    left: 20,
    right: 20,
    flexDirection: 'row',
    justifyContent: 'center',
    gap: 16,
  },
  footerButton: {
    minWidth: 120,
    borderColor: '#fff',
  },
  footerButtonLabel: {
    color: '#fff',
  },
});
