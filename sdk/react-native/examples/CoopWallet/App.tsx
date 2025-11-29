/**
 * Coop Wallet - Example ICN React Native App
 *
 * Demonstrates core cooperative features:
 * - Authentication with secure wallet
 * - Balance display and payment
 * - QR code scan-to-pay
 * - Governance voting
 */

import React, { useEffect, useState } from 'react';
import { NavigationContainer } from '@react-navigation/native';
import { createNativeStackNavigator } from '@react-navigation/native-stack';
import { ActivityIndicator, View, StyleSheet } from 'react-native';
import { StatusBar } from 'expo-status-bar';

import { client, initializeClient } from './src/client';

// Screens
import { LoginScreen } from './src/screens/LoginScreen';
import { HomeScreen } from './src/screens/HomeScreen';
import { PaymentScreen } from './src/screens/PaymentScreen';
import { ScanScreen } from './src/screens/ScanScreen';
import { ReceiveScreen } from './src/screens/ReceiveScreen';
import { GovernanceScreen } from './src/screens/GovernanceScreen';
import { ProposalScreen } from './src/screens/ProposalScreen';

export type RootStackParamList = {
  Login: undefined;
  Home: undefined;
  Payment: { to?: string; amount?: number; memo?: string };
  Scan: undefined;
  Receive: undefined;
  Governance: undefined;
  Proposal: { proposalId: string };
};

const Stack = createNativeStackNavigator<RootStackParamList>();

export default function App() {
  const [isReady, setIsReady] = useState(false);
  const [isAuthenticated, setIsAuthenticated] = useState(false);

  useEffect(() => {
    async function init() {
      try {
        await initializeClient();
        setIsAuthenticated(client.authState.isAuthenticated);
      } catch (error) {
        console.error('Failed to initialize client:', error);
      } finally {
        setIsReady(true);
      }
    }
    init();

    // Listen for auth changes
    const unsubscribe = client.onAuthStateChange((state) => {
      setIsAuthenticated(state.isAuthenticated);
    });

    return unsubscribe;
  }, []);

  if (!isReady) {
    return (
      <View style={styles.loading}>
        <ActivityIndicator size="large" color="#4A90A4" />
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
          <Stack.Screen
            name="Login"
            component={LoginScreen}
            options={{ headerShown: false }}
          />
        ) : (
          <>
            <Stack.Screen
              name="Home"
              component={HomeScreen}
              options={{ title: 'Coop Wallet' }}
            />
            <Stack.Screen
              name="Payment"
              component={PaymentScreen}
              options={{ title: 'Send Payment' }}
            />
            <Stack.Screen
              name="Scan"
              component={ScanScreen}
              options={{ title: 'Scan QR Code' }}
            />
            <Stack.Screen
              name="Receive"
              component={ReceiveScreen}
              options={{ title: 'Receive Payment' }}
            />
            <Stack.Screen
              name="Governance"
              component={GovernanceScreen}
              options={{ title: 'Governance' }}
            />
            <Stack.Screen
              name="Proposal"
              component={ProposalScreen}
              options={{ title: 'Proposal Details' }}
            />
          </>
        )}
      </Stack.Navigator>
    </NavigationContainer>
  );
}

const styles = StyleSheet.create({
  loading: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    backgroundColor: '#f5f5f5',
  },
});
