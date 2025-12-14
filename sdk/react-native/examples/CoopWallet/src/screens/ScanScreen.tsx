/**
 * Scan Screen
 *
 * Universal QR code scanner for payments, contacts, and coop joining.
 */

import React, { useState, useEffect } from 'react';
import {
  View,
  Text,
  StyleSheet,
  Alert,
  TouchableOpacity,
  Platform,
  ActivityIndicator,
} from 'react-native';
// Dynamic import to prevent crash if expo-camera has issues
let CameraView: any = null;
let requestCameraPermissionsAsync: (() => Promise<{ status: string }>) | null = null;

try {
  const expoCamera = require('expo-camera');
  CameraView = expoCamera.CameraView;
  requestCameraPermissionsAsync = expoCamera.requestCameraPermissionsAsync;
  console.log('ScanScreen: expo-camera loaded successfully');
  console.log('ScanScreen: CameraView:', typeof CameraView);
  console.log('ScanScreen: requestCameraPermissionsAsync:', typeof requestCameraPermissionsAsync);
} catch (e) {
  console.error('ScanScreen: Failed to load expo-camera:', e);
}

type BarcodeScanningResult = { data: string };
import { NativeStackNavigationProp } from '@react-navigation/native-stack';
import { RouteProp } from '@react-navigation/native';
import { parseQR } from '@icn/react-native';
import { RootStackParamList } from '../../App';
import { useTheme, Theme } from '../contexts/ThemeContext';
import { saveContact } from '../storage';

type Props = {
  navigation: NativeStackNavigationProp<RootStackParamList, 'Scan'>;
  route: RouteProp<RootStackParamList, 'Scan'>;
};

export function ScanScreen({ navigation, route }: Props) {
  console.log('ScanScreen: Component rendering');

  const { theme } = useTheme();
  const styles = createStyles(theme);
  const [hasPermission, setHasPermission] = useState<boolean | null>(null);
  const [scanned, setScanned] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Optional: scan mode can be passed as param to filter QR types
  const scanMode = route.params?.mode || 'all';

  // Request camera permission on mount
  useEffect(() => {
    console.log('ScanScreen: useEffect running');
    let timeoutId: NodeJS.Timeout;

    const getPermission = async () => {
      try {
        if (!requestCameraPermissionsAsync) {
          console.error('ScanScreen: requestCameraPermissionsAsync not available');
          setError('Camera API not available');
          setHasPermission(false);
          return;
        }

        // Set a timeout in case permission request hangs
        const timeoutPromise = new Promise<never>((_, reject) => {
          timeoutId = setTimeout(() => {
            reject(new Error('Permission request timed out. Please check app permissions in device settings.'));
          }, 10000);
        });

        console.log('ScanScreen: Requesting camera permission...');
        const { status } = await Promise.race([
          requestCameraPermissionsAsync(),
          timeoutPromise,
        ]);
        clearTimeout(timeoutId);

        console.log('ScanScreen: Camera permission status:', status);
        setHasPermission(status === 'granted');
      } catch (err) {
        console.error('ScanScreen: Camera permission error:', err);
        setError(String(err));
        setHasPermission(false);
      }
    };
    getPermission();

    return () => {
      if (timeoutId) clearTimeout(timeoutId);
    };
  }, []);

  const handleBarCodeScanned = ({ data }: BarcodeScanningResult) => {
    if (scanned) return;
    setScanned(true);

    // Parse the QR code
    const result = parseQR(data);

    switch (result.type) {
      case 'payment':
        if (scanMode !== 'all' && scanMode !== 'payment') {
          Alert.alert('Wrong QR Type', 'This is a payment QR code. Please scan the correct type.', [
            { text: 'Try Again', onPress: () => setScanned(false) },
          ]);
          return;
        }
        navigation.navigate('Payment', {
          recipient: result.data.to,
          amount: result.data.amount?.toString(),
          memo: result.data.memo,
        });
        break;

      case 'contact':
        if (scanMode !== 'all' && scanMode !== 'contact') {
          Alert.alert('Wrong QR Type', 'This is a contact QR code. Please scan the correct type.', [
            { text: 'Try Again', onPress: () => setScanned(false) },
          ]);
          return;
        }
        handleAddContact(result.data);
        break;

      case 'join':
        if (scanMode !== 'all' && scanMode !== 'join') {
          Alert.alert('Wrong QR Type', 'This is a coop join QR code. Please scan the correct type.', [
            { text: 'Try Again', onPress: () => setScanned(false) },
          ]);
          return;
        }
        handleJoinCoop(result.data);
        break;

      default:
        Alert.alert('Invalid QR Code', 'This QR code is not recognized as an ICN code.', [
          { text: 'Try Again', onPress: () => setScanned(false) },
          { text: 'Cancel', onPress: () => navigation.goBack() },
        ]);
    }
  };

  const handleAddContact = async (contactData: { did: string; name?: string; coopId?: string }) => {
    try {
      await saveContact({
        did: contactData.did,
        name: contactData.name || `Member ${contactData.did.slice(-8)}`,
        coopId: contactData.coopId,
      });

      Alert.alert(
        'Contact Added',
        `${contactData.name || contactData.did.slice(0, 20)}... has been added to your contacts.`,
        [
          {
            text: 'View Profile',
            onPress: () => navigation.replace('MemberProfile', { memberDid: contactData.did }),
          },
          {
            text: 'Done',
            onPress: () => navigation.goBack(),
          },
        ]
      );
    } catch (error) {
      Alert.alert('Error', 'Failed to add contact. Please try again.', [
        { text: 'Try Again', onPress: () => setScanned(false) },
      ]);
    }
  };

  const handleJoinCoop = (joinData: { coopId: string; gateway: string; name?: string }) => {
    Alert.alert(
      'Join Cooperative',
      `Would you like to join "${joinData.name || joinData.coopId}"?`,
      [
        {
          text: 'Cancel',
          style: 'cancel',
          onPress: () => setScanned(false),
        },
        {
          text: 'Join',
          onPress: () => {
            navigation.reset({
              index: 0,
              routes: [
                {
                  name: 'Login',
                  params: {
                    coopId: joinData.coopId,
                    gateway: joinData.gateway,
                    coopName: joinData.name,
                  },
                },
              ],
            });
          },
        },
      ]
    );
  };

  const getHintText = () => {
    switch (scanMode) {
      case 'payment':
        return 'Point camera at payment QR code';
      case 'contact':
        return 'Point camera at contact QR code';
      case 'join':
        return 'Point camera at coop QR code';
      default:
        return 'Point camera at any ICN QR code';
    }
  };

  // Camera not available (failed to load)
  if (!requestCameraPermissionsAsync || !CameraView) {
    return (
      <View style={styles.container}>
        <Text style={styles.title}>📷</Text>
        <Text style={styles.message}>Camera is not available.</Text>
        <Text style={[styles.message, { fontSize: 14, opacity: 0.8 }]}>
          expo-camera may not be properly installed or configured.
        </Text>
        <TouchableOpacity style={styles.button} onPress={() => navigation.goBack()}>
          <Text style={styles.buttonText}>Go Back</Text>
        </TouchableOpacity>
      </View>
    );
  }

  // Web platform doesn't support camera
  if (Platform.OS === 'web') {
    return (
      <View style={styles.container}>
        <Text style={styles.message}>Camera scanning is not available on web.</Text>
        <Text style={[styles.message, { marginTop: 8, fontSize: 14 }]}>
          Please use the mobile app to scan QR codes.
        </Text>
        <TouchableOpacity style={styles.button} onPress={() => navigation.goBack()}>
          <Text style={styles.buttonText}>Go Back</Text>
        </TouchableOpacity>
      </View>
    );
  }

  // Loading state - permission is null while loading
  if (hasPermission === null) {
    return (
      <View style={styles.container}>
        <ActivityIndicator size="large" color="#fff" />
        <Text style={styles.message}>Requesting camera access...</Text>
        <TouchableOpacity
          style={[styles.button, { marginTop: 20 }]}
          onPress={() => navigation.goBack()}
        >
          <Text style={styles.buttonText}>Cancel</Text>
        </TouchableOpacity>
      </View>
    );
  }

  // Permission denied or error
  if (hasPermission === false) {
    return (
      <View style={styles.container}>
        <Text style={styles.title}>📷</Text>
        <Text style={styles.message}>
          {error || 'Camera access is required to scan QR codes.'}
        </Text>
        <Text style={[styles.message, { fontSize: 14, opacity: 0.8 }]}>
          Please enable camera access in your device settings.
        </Text>
        <TouchableOpacity
          style={styles.button}
          onPress={() => navigation.goBack()}
        >
          <Text style={styles.buttonText}>Go Back</Text>
        </TouchableOpacity>
      </View>
    );
  }

  // Camera view
  return (
    <View style={styles.container}>
      <CameraView
        style={styles.camera}
        facing="back"
        onBarcodeScanned={scanned ? undefined : handleBarCodeScanned}
        barcodeScannerSettings={{
          barcodeTypes: ['qr'],
        }}
      >
        <View style={styles.overlay}>
          <View style={styles.scanArea}>
            <View style={[styles.corner, styles.topLeft]} />
            <View style={[styles.corner, styles.topRight]} />
            <View style={[styles.corner, styles.bottomLeft]} />
            <View style={[styles.corner, styles.bottomRight]} />
          </View>
          <Text style={styles.hint}>{getHintText()}</Text>

          {scanMode === 'all' && (
            <Text style={styles.subHint}>Payments • Contacts • Join Coop</Text>
          )}

          <TouchableOpacity
            style={styles.cancelButton}
            onPress={() => navigation.goBack()}
          >
            <Text style={styles.cancelButtonText}>Cancel</Text>
          </TouchableOpacity>
        </View>
      </CameraView>
    </View>
  );
}

const createStyles = (theme: Theme) =>
  StyleSheet.create({
    container: {
      flex: 1,
      backgroundColor: '#000',
      justifyContent: 'center',
      alignItems: 'center',
    },
    camera: {
      flex: 1,
      width: '100%',
    },
    overlay: {
      flex: 1,
      justifyContent: 'center',
      alignItems: 'center',
    },
    scanArea: {
      width: 250,
      height: 250,
      position: 'relative',
    },
    corner: {
      position: 'absolute',
      width: 40,
      height: 40,
      borderColor: '#fff',
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
    title: {
      fontSize: 48,
      marginBottom: 16,
    },
    hint: {
      color: '#fff',
      fontSize: 16,
      marginTop: 24,
      textAlign: 'center',
    },
    subHint: {
      color: 'rgba(255,255,255,0.7)',
      fontSize: 12,
      marginTop: 8,
      textAlign: 'center',
    },
    message: {
      color: '#fff',
      fontSize: 16,
      textAlign: 'center',
      marginTop: 16,
      marginBottom: 24,
      paddingHorizontal: 32,
    },
    button: {
      backgroundColor: theme.colors.primary,
      paddingHorizontal: 24,
      paddingVertical: 14,
      borderRadius: 8,
      minWidth: 200,
      alignItems: 'center',
    },
    secondaryButton: {
      marginTop: 12,
      backgroundColor: '#666',
    },
    buttonText: {
      color: '#fff',
      fontSize: 16,
      fontWeight: '600',
    },
    cancelButton: {
      marginTop: 40,
      paddingHorizontal: 32,
      paddingVertical: 12,
    },
    cancelButtonText: {
      color: '#fff',
      fontSize: 16,
      opacity: 0.8,
    },
  });
