/**
 * Coop Wallet - ICN React Native App
 *
 * Features:
 * - Authentication with secure wallet
 * - Balance display and payment
 * - QR code scan-to-pay
 * - Governance voting
 * - SDIS Identity verification
 */

import React, { useEffect, useState, useCallback } from 'react';
import { NavigationContainer } from '@react-navigation/native';
import { createNativeStackNavigator } from '@react-navigation/native-stack';
import {
  ActivityIndicator,
  View,
  StyleSheet,
  Platform,
  Text,
  TextInput,
  TouchableOpacity,
  ScrollView,
  RefreshControl,
  KeyboardAvoidingView,
} from 'react-native';
import { StatusBar } from 'expo-status-bar';

// Import client with error handling
let client: any = null;
let initializeClient: () => Promise<void> = async () => {};

try {
  const clientModule = require('./src/client');
  client = clientModule.client;
  initializeClient = clientModule.initializeClient;
} catch (e) {
  console.error('Failed to import client:', e);
}

// ============================================================================
// Inline Screens (Web-safe, no external hook dependencies)
// ============================================================================

function LoginScreen({ onLogin }: { onLogin: (coopId: string) => void }) {
  const [coopId, setCoopId] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleLogin = async () => {
    if (!coopId.trim()) {
      setError('Please enter your cooperative ID');
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      if (client) {
        await client.login(coopId.trim());
      }
      onLogin(coopId.trim());
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <KeyboardAvoidingView
      style={styles.loginContainer}
      behavior={Platform.OS === 'ios' ? 'padding' : 'height'}
    >
      <View style={styles.loginContent}>
        <Text style={styles.loginTitle}>Coop Wallet</Text>
        <Text style={styles.loginSubtitle}>InterCooperative Network</Text>

        <View style={styles.loginForm}>
          <Text style={styles.label}>Cooperative ID</Text>
          <TextInput
            style={styles.input}
            placeholder="e.g., my-timebank"
            value={coopId}
            onChangeText={setCoopId}
            autoCapitalize="none"
            autoCorrect={false}
          />

          {error && <Text style={styles.errorText}>{error}</Text>}

          <TouchableOpacity
            style={[styles.primaryButton, isLoading && styles.buttonDisabled]}
            onPress={handleLogin}
            disabled={isLoading}
          >
            {isLoading ? (
              <ActivityIndicator color="#fff" />
            ) : (
              <Text style={styles.buttonText}>Login</Text>
            )}
          </TouchableOpacity>
        </View>

        <Text style={styles.footerText}>
          Your keys are stored securely on your device.
        </Text>
      </View>
    </KeyboardAvoidingView>
  );
}

function HomeScreen({
  navigation,
  coopId,
  onLogout
}: {
  navigation: any;
  coopId: string;
  onLogout: () => void;
}) {
  const [balance, setBalance] = useState(0);
  const [isLoading, setIsLoading] = useState(false);
  const [did, setDid] = useState<string | null>(null);

  useEffect(() => {
    if (client?.authState?.did) {
      setDid(client.authState.did);
    }
    // Simulate fetching balance
    setBalance(Math.floor(Math.random() * 100));
  }, []);

  const refresh = useCallback(() => {
    setIsLoading(true);
    setTimeout(() => {
      setBalance(Math.floor(Math.random() * 100));
      setIsLoading(false);
    }, 1000);
  }, []);

  const handleLogout = async () => {
    if (client) {
      try {
        await client.logout();
      } catch (e) {
        console.error('Logout error:', e);
      }
    }
    onLogout();
  };

  const formatDid = (did: string) => {
    if (did && did.length > 20) {
      return `${did.slice(0, 12)}...${did.slice(-6)}`;
    }
    return did || 'Unknown';
  };

  return (
    <ScrollView
      style={styles.container}
      refreshControl={
        <RefreshControl refreshing={isLoading} onRefresh={refresh} />
      }
    >
      {/* Balance Card */}
      <View style={styles.balanceCard}>
        <Text style={styles.balanceLabel}>Available Balance</Text>
        <Text style={styles.balanceAmount}>
          {balance}
          <Text style={styles.balanceCurrency}> hours</Text>
        </Text>
        <Text style={styles.coopName}>{coopId}</Text>
        <Text style={styles.didText}>{formatDid(did || '')}</Text>
      </View>

      {/* Quick Actions */}
      <View style={styles.actionsGrid}>
        <TouchableOpacity
          style={styles.actionButton}
          onPress={() => navigation.navigate('Payment')}
        >
          <Text style={styles.actionIcon}>💸</Text>
          <Text style={styles.actionLabel}>Send</Text>
        </TouchableOpacity>

        <TouchableOpacity
          style={styles.actionButton}
          onPress={() => navigation.navigate('Receive')}
        >
          <Text style={styles.actionIcon}>📥</Text>
          <Text style={styles.actionLabel}>Receive</Text>
        </TouchableOpacity>

        <TouchableOpacity
          style={styles.actionButton}
          onPress={() => navigation.navigate('Scan')}
        >
          <Text style={styles.actionIcon}>📷</Text>
          <Text style={styles.actionLabel}>Scan</Text>
        </TouchableOpacity>

        <TouchableOpacity
          style={styles.actionButton}
          onPress={() => navigation.navigate('Governance')}
        >
          <Text style={styles.actionIcon}>🗳️</Text>
          <Text style={styles.actionLabel}>Vote</Text>
        </TouchableOpacity>
      </View>

      {/* SDIS Section */}
      <View style={styles.section}>
        <Text style={styles.sectionTitle}>Identity & Verification</Text>

        <TouchableOpacity
          style={styles.menuItem}
          onPress={() => navigation.navigate('Identity')}
        >
          <Text style={styles.menuIcon}>🪪</Text>
          <View style={styles.menuContent}>
            <Text style={styles.menuLabel}>My Identity</Text>
            <Text style={styles.menuDescription}>View and share your identity</Text>
          </View>
          <Text style={styles.menuArrow}>›</Text>
        </TouchableOpacity>

        <TouchableOpacity
          style={styles.menuItem}
          onPress={() => navigation.navigate('Verify')}
        >
          <Text style={styles.menuIcon}>✅</Text>
          <View style={styles.menuContent}>
            <Text style={styles.menuLabel}>Verify Someone</Text>
            <Text style={styles.menuDescription}>Scan and verify another member</Text>
          </View>
          <Text style={styles.menuArrow}>›</Text>
        </TouchableOpacity>

        <TouchableOpacity
          style={styles.menuItem}
          onPress={() => navigation.navigate('VerificationHistory')}
        >
          <Text style={styles.menuIcon}>📋</Text>
          <View style={styles.menuContent}>
            <Text style={styles.menuLabel}>Verification History</Text>
            <Text style={styles.menuDescription}>View past verifications</Text>
          </View>
          <Text style={styles.menuArrow}>›</Text>
        </TouchableOpacity>
      </View>

      {/* Logout Button */}
      <TouchableOpacity style={styles.logoutButton} onPress={handleLogout}>
        <Text style={styles.logoutText}>Logout</Text>
      </TouchableOpacity>
    </ScrollView>
  );
}

// Placeholder screens
function PaymentScreen() {
  return (
    <View style={styles.placeholder}>
      <Text style={styles.placeholderIcon}>💸</Text>
      <Text style={styles.placeholderTitle}>Send Payment</Text>
      <Text style={styles.placeholderText}>Enter recipient and amount</Text>
    </View>
  );
}

function ReceiveScreen() {
  return (
    <View style={styles.placeholder}>
      <Text style={styles.placeholderIcon}>📥</Text>
      <Text style={styles.placeholderTitle}>Receive Payment</Text>
      <Text style={styles.placeholderText}>Show QR code to receive</Text>
    </View>
  );
}

function ScanScreen() {
  return (
    <View style={styles.placeholder}>
      <Text style={styles.placeholderIcon}>📷</Text>
      <Text style={styles.placeholderTitle}>Scan QR Code</Text>
      <Text style={styles.placeholderText}>
        {Platform.OS === 'web'
          ? 'Camera scanning not available on web'
          : 'Point camera at QR code'}
      </Text>
    </View>
  );
}

function GovernanceScreen() {
  return (
    <View style={styles.placeholder}>
      <Text style={styles.placeholderIcon}>🗳️</Text>
      <Text style={styles.placeholderTitle}>Governance</Text>
      <Text style={styles.placeholderText}>View and vote on proposals</Text>
    </View>
  );
}

function IdentityScreen() {
  return (
    <View style={styles.placeholder}>
      <Text style={styles.placeholderIcon}>🪪</Text>
      <Text style={styles.placeholderTitle}>My Identity</Text>
      <Text style={styles.placeholderText}>Your SDIS identity details</Text>
    </View>
  );
}

function VerifyScreen() {
  return (
    <View style={styles.placeholder}>
      <Text style={styles.placeholderIcon}>✅</Text>
      <Text style={styles.placeholderTitle}>Verify Identity</Text>
      <Text style={styles.placeholderText}>Scan to verify someone</Text>
    </View>
  );
}

function VerificationHistoryScreen() {
  return (
    <View style={styles.placeholder}>
      <Text style={styles.placeholderIcon}>📋</Text>
      <Text style={styles.placeholderTitle}>Verification History</Text>
      <Text style={styles.placeholderText}>Your past verifications</Text>
    </View>
  );
}

// ============================================================================
// Navigation Setup
// ============================================================================

export type RootStackParamList = {
  Login: undefined;
  Home: undefined;
  Payment: undefined;
  Scan: undefined;
  Receive: undefined;
  Governance: undefined;
  Identity: undefined;
  Verify: undefined;
  VerificationHistory: undefined;
};

const Stack = createNativeStackNavigator<RootStackParamList>();

export default function App() {
  const [isReady, setIsReady] = useState(false);
  const [isAuthenticated, setIsAuthenticated] = useState(false);
  const [coopId, setCoopId] = useState<string>('');

  useEffect(() => {
    async function init() {
      try {
        if (client) {
          await initializeClient();
          if (client.authState?.isAuthenticated) {
            setIsAuthenticated(true);
            setCoopId(client.authState.coopId || '');
          }
        }
      } catch (error) {
        console.error('Init error:', error);
      } finally {
        setIsReady(true);
      }
    }
    init();
  }, []);

  const handleLogin = (coop: string) => {
    setCoopId(coop);
    setIsAuthenticated(true);
  };

  const handleLogout = () => {
    setIsAuthenticated(false);
    setCoopId('');
  };

  if (!isReady) {
    return (
      <View style={styles.loading}>
        <ActivityIndicator size="large" color="#4A90A4" />
        <Text style={styles.loadingText}>Loading...</Text>
      </View>
    );
  }

  return (
    <NavigationContainer>
      <StatusBar style="auto" />
      <Stack.Navigator
        screenOptions={{
          headerStyle: { backgroundColor: '#4A90A4' },
          headerTintColor: '#fff',
          headerTitleStyle: { fontWeight: 'bold' },
        }}
      >
        {!isAuthenticated ? (
          <Stack.Screen name="Login" options={{ headerShown: false }}>
            {() => <LoginScreen onLogin={handleLogin} />}
          </Stack.Screen>
        ) : (
          <>
            <Stack.Screen name="Home" options={{ title: 'Coop Wallet' }}>
              {({ navigation }) => (
                <HomeScreen
                  navigation={navigation}
                  coopId={coopId}
                  onLogout={handleLogout}
                />
              )}
            </Stack.Screen>
            <Stack.Screen name="Payment" component={PaymentScreen} options={{ title: 'Send Payment' }} />
            <Stack.Screen name="Scan" component={ScanScreen} options={{ title: 'Scan QR Code' }} />
            <Stack.Screen name="Receive" component={ReceiveScreen} options={{ title: 'Receive Payment' }} />
            <Stack.Screen name="Governance" component={GovernanceScreen} options={{ title: 'Governance' }} />
            <Stack.Screen name="Identity" component={IdentityScreen} options={{ title: 'My Identity' }} />
            <Stack.Screen name="Verify" component={VerifyScreen} options={{ title: 'Verify Identity' }} />
            <Stack.Screen name="VerificationHistory" component={VerificationHistoryScreen} options={{ title: 'History' }} />
          </>
        )}
      </Stack.Navigator>
    </NavigationContainer>
  );
}

// ============================================================================
// Styles
// ============================================================================

const styles = StyleSheet.create({
  // Loading
  loading: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    backgroundColor: '#f5f5f5',
  },
  loadingText: {
    marginTop: 10,
    color: '#666',
  },

  // Login
  loginContainer: {
    flex: 1,
    backgroundColor: '#4A90A4',
  },
  loginContent: {
    flex: 1,
    justifyContent: 'center',
    padding: 24,
  },
  loginTitle: {
    fontSize: 36,
    fontWeight: 'bold',
    color: '#fff',
    textAlign: 'center',
    marginBottom: 8,
  },
  loginSubtitle: {
    fontSize: 16,
    color: 'rgba(255,255,255,0.8)',
    textAlign: 'center',
    marginBottom: 48,
  },
  loginForm: {
    backgroundColor: '#fff',
    borderRadius: 16,
    padding: 24,
  },
  label: {
    fontSize: 14,
    fontWeight: '600',
    color: '#333',
    marginBottom: 8,
  },
  input: {
    backgroundColor: '#f5f5f5',
    borderRadius: 8,
    padding: 16,
    fontSize: 16,
    marginBottom: 16,
  },
  primaryButton: {
    backgroundColor: '#4A90A4',
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
  errorText: {
    color: '#e53935',
    fontSize: 14,
    marginBottom: 16,
    textAlign: 'center',
  },
  footerText: {
    color: 'rgba(255,255,255,0.7)',
    fontSize: 12,
    textAlign: 'center',
    marginTop: 24,
  },

  // Home
  container: {
    flex: 1,
    backgroundColor: '#f5f5f5',
  },
  balanceCard: {
    backgroundColor: '#4A90A4',
    margin: 16,
    padding: 24,
    borderRadius: 16,
    alignItems: 'center',
  },
  balanceLabel: {
    color: 'rgba(255,255,255,0.8)',
    fontSize: 14,
  },
  balanceAmount: {
    color: '#fff',
    fontSize: 48,
    fontWeight: 'bold',
    marginVertical: 8,
  },
  balanceCurrency: {
    fontSize: 24,
    fontWeight: 'normal',
  },
  coopName: {
    color: 'rgba(255,255,255,0.9)',
    fontSize: 16,
    marginTop: 8,
  },
  didText: {
    color: 'rgba(255,255,255,0.6)',
    fontSize: 12,
    marginTop: 4,
  },

  // Actions Grid
  actionsGrid: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    padding: 8,
    justifyContent: 'center',
  },
  actionButton: {
    width: '45%',
    backgroundColor: '#fff',
    margin: 8,
    padding: 20,
    borderRadius: 12,
    alignItems: 'center',
  },
  actionIcon: {
    fontSize: 32,
    marginBottom: 8,
  },
  actionLabel: {
    fontSize: 14,
    fontWeight: '600',
    color: '#333',
  },

  // Sections
  section: {
    margin: 16,
  },
  sectionTitle: {
    fontSize: 18,
    fontWeight: 'bold',
    color: '#333',
    marginBottom: 12,
  },
  menuItem: {
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: '#fff',
    padding: 16,
    borderRadius: 12,
    marginBottom: 8,
  },
  menuIcon: {
    fontSize: 24,
    marginRight: 16,
  },
  menuContent: {
    flex: 1,
  },
  menuLabel: {
    fontSize: 16,
    fontWeight: '600',
    color: '#333',
  },
  menuDescription: {
    fontSize: 12,
    color: '#666',
    marginTop: 2,
  },
  menuArrow: {
    fontSize: 24,
    color: '#ccc',
  },

  // Logout
  logoutButton: {
    margin: 16,
    padding: 16,
    alignItems: 'center',
  },
  logoutText: {
    color: '#e53935',
    fontSize: 16,
  },

  // Placeholder screens
  placeholder: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    backgroundColor: '#f5f5f5',
    padding: 24,
  },
  placeholderIcon: {
    fontSize: 64,
    marginBottom: 16,
  },
  placeholderTitle: {
    fontSize: 24,
    fontWeight: 'bold',
    color: '#333',
    marginBottom: 8,
  },
  placeholderText: {
    fontSize: 16,
    color: '#666',
    textAlign: 'center',
  },
});
