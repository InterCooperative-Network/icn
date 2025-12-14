/**
 * Login Screen
 *
 * Handles cooperative selection and authentication.
 * Supports QR code scanning for easy coop joining.
 */

import React, { useState, useEffect } from 'react';
import {
  View,
  Text,
  TextInput,
  TouchableOpacity,
  StyleSheet,
  ActivityIndicator,
  Alert,
  KeyboardAvoidingView,
  Platform,
} from 'react-native';
import { NativeStackNavigationProp } from '@react-navigation/native-stack';
import { RouteProp } from '@react-navigation/native';
import { client } from '../client';
import { useTheme, Theme } from '../contexts/ThemeContext';
import { RootStackParamList } from '../../App';

type Props = {
  navigation: NativeStackNavigationProp<RootStackParamList, 'Login'>;
  route: RouteProp<RootStackParamList, 'Login'>;
};

export function LoginScreen({ navigation, route }: Props) {
  const { theme } = useTheme();
  const styles = createStyles(theme);
  const [coopId, setCoopId] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Handle pre-filled data from QR scan
  useEffect(() => {
    if (route.params?.coopId) {
      setCoopId(route.params.coopId);
      // If we have gateway info from QR, show a message
      if (route.params?.coopName) {
        Alert.alert(
          'Join Cooperative',
          `Joining "${route.params.coopName}"`,
          [{ text: 'OK' }]
        );
      }
    }
  }, [route.params]);

  const handleLogin = async () => {
    if (!coopId.trim()) {
      if (Platform.OS === 'web') {
        alert('Please enter your cooperative ID');
      } else {
        Alert.alert('Error', 'Please enter your cooperative ID');
      }
      return;
    }

    if (!client) {
      setError('Client not initialized');
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      await client.login(coopId.trim());
    } catch (err) {
      const message = (err as Error).message;
      setError(message);
      if (Platform.OS === 'web') {
        alert('Login Failed: ' + message);
      } else {
        Alert.alert('Login Failed', message);
      }
    } finally {
      setIsLoading(false);
    }
  };

  const handleScanToJoin = () => {
    navigation.navigate('Scan', { mode: 'join' });
  };

  return (
    <KeyboardAvoidingView
      style={styles.container}
      behavior={Platform.OS === 'ios' ? 'padding' : 'height'}
    >
      <View style={styles.content}>
        <Text style={styles.title}>Coop Wallet</Text>
        <Text style={styles.subtitle}>InterCooperative Network</Text>

        <View style={styles.form}>
          <Text style={styles.label}>Cooperative ID</Text>
          <TextInput
            style={styles.input}
            placeholder="e.g., my-timebank"
            placeholderTextColor={theme.colors.textSecondary}
            value={coopId}
            onChangeText={setCoopId}
            autoCapitalize="none"
            autoCorrect={false}
          />

          {error && <Text style={styles.error}>{error}</Text>}

          <TouchableOpacity
            style={[styles.button, isLoading && styles.buttonDisabled]}
            onPress={handleLogin}
            disabled={isLoading}
          >
            {isLoading ? (
              <ActivityIndicator color="#fff" />
            ) : (
              <Text style={styles.buttonText}>Login</Text>
            )}
          </TouchableOpacity>

          <View style={styles.divider}>
            <View style={styles.dividerLine} />
            <Text style={styles.dividerText}>or</Text>
            <View style={styles.dividerLine} />
          </View>

          <TouchableOpacity
            style={styles.scanButton}
            onPress={handleScanToJoin}
          >
            <Text style={styles.scanIcon}>📷</Text>
            <Text style={styles.scanButtonText}>Scan QR to Join</Text>
          </TouchableOpacity>
        </View>

        <Text style={styles.footer}>
          Your keys are stored securely on your device.
        </Text>
      </View>
    </KeyboardAvoidingView>
  );
}

const createStyles = (theme: Theme) => StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: theme.colors.primary,
  },
  content: {
    flex: 1,
    justifyContent: 'center',
    padding: 24,
  },
  title: {
    fontSize: 36,
    fontWeight: 'bold',
    color: '#fff',
    textAlign: 'center',
    marginBottom: 8,
  },
  subtitle: {
    fontSize: 16,
    color: 'rgba(255,255,255,0.8)',
    textAlign: 'center',
    marginBottom: 48,
  },
  form: {
    backgroundColor: theme.colors.card,
    borderRadius: 16,
    padding: 24,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 2 },
    shadowOpacity: 0.1,
    shadowRadius: 8,
    elevation: 4,
  },
  label: {
    fontSize: 14,
    fontWeight: '600',
    color: theme.colors.text,
    marginBottom: 8,
  },
  input: {
    backgroundColor: theme.colors.borderLight,
    borderRadius: 8,
    padding: 16,
    fontSize: 16,
    marginBottom: 16,
    color: theme.colors.text,
  },
  button: {
    backgroundColor: theme.colors.primary,
    borderRadius: 8,
    padding: 16,
    alignItems: 'center',
  },
  buttonDisabled: {
    opacity: 0.7,
  },
  buttonText: {
    color: '#fff',
    fontSize: 18,
    fontWeight: '600',
  },
  divider: {
    flexDirection: 'row',
    alignItems: 'center',
    marginVertical: 20,
  },
  dividerLine: {
    flex: 1,
    height: 1,
    backgroundColor: theme.colors.border,
  },
  dividerText: {
    color: theme.colors.textSecondary,
    paddingHorizontal: 12,
    fontSize: 14,
  },
  scanButton: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: theme.colors.borderLight,
    borderRadius: 8,
    padding: 16,
    borderWidth: 2,
    borderColor: theme.colors.primary,
    borderStyle: 'dashed',
  },
  scanIcon: {
    fontSize: 20,
    marginRight: 8,
  },
  scanButtonText: {
    color: theme.colors.primary,
    fontSize: 16,
    fontWeight: '600',
  },
  error: {
    color: theme.colors.error,
    fontSize: 14,
    marginBottom: 16,
    textAlign: 'center',
  },
  footer: {
    color: 'rgba(255,255,255,0.7)',
    fontSize: 12,
    textAlign: 'center',
    marginTop: 24,
  },
});
