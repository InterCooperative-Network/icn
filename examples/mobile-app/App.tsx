import React, { useState, useEffect } from 'react';
import { NavigationContainer } from '@react-navigation/native';
import { createBottomTabNavigator } from '@react-navigation/bottom-tabs';
import { createStackNavigator } from '@react-navigation/stack';
import { Provider as PaperProvider, DefaultTheme } from 'react-native-paper';
import { StatusBar } from 'expo-status-bar';

// Secure storage for credentials
import { SecureStorage } from './src/services/SecureStorage';

// Import screens
import HomeScreen from './src/screens/HomeScreen';
import LedgerScreen from './src/screens/LedgerScreen';
import GovernanceScreen from './src/screens/GovernanceScreen';
import CooperativesScreen from './src/screens/CooperativesScreen';
import ProfileScreen from './src/screens/ProfileScreen';
import LoginScreen from './src/screens/LoginScreen';
import QRScannerScreen from './src/screens/QRScannerScreen';

// Import components
// NotificationCenter disabled - requires @icn/react-native which is not yet implemented
// import NotificationCenter from './NotificationCenter';

// Import services
import { ICNClient } from './src/services/ICNClient';
import { AuthContext } from './src/contexts/AuthContext';

const Tab = createBottomTabNavigator();
const Stack = createStackNavigator();

// Custom theme
const theme = {
  ...DefaultTheme,
  colors: {
    ...DefaultTheme.colors,
    primary: '#2563eb',
    accent: '#10b981',
    background: '#f1f5f9',
    surface: '#ffffff',
    text: '#1e293b',
  },
};

// Main tab navigator
function MainTabs() {
  return (
    <Tab.Navigator
      screenOptions={{
        tabBarActiveTintColor: theme.colors.primary,
        tabBarInactiveTintColor: '#94a3b8',
        tabBarStyle: {
          backgroundColor: theme.colors.surface,
          borderTopWidth: 1,
          borderTopColor: '#e2e8f0',
        },
      }}
    >
      <Tab.Screen 
        name="Home" 
        component={HomeScreen}
        options={{
          tabBarLabel: 'Home',
          tabBarIcon: () => '🏠',
        }}
      />
      <Tab.Screen 
        name="Ledger" 
        component={LedgerScreen}
        options={{
          tabBarLabel: 'Ledger',
          tabBarIcon: () => '💰',
        }}
      />
      <Tab.Screen 
        name="Governance" 
        component={GovernanceScreen}
        options={{
          tabBarLabel: 'Governance',
          tabBarIcon: () => '🗳️',
        }}
      />
      <Tab.Screen 
        name="Cooperatives" 
        component={CooperativesScreen}
        options={{
          tabBarLabel: 'Co-ops',
          tabBarIcon: () => '🤝',
        }}
      />
      <Tab.Screen 
        name="Profile" 
        component={ProfileScreen}
        options={{
          tabBarLabel: 'Profile',
          tabBarIcon: () => '👤',
        }}
      />
    </Tab.Navigator>
  );
}

export default function App() {
  const [isAuthenticated, setIsAuthenticated] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [client, setClient] = useState<ICNClient | null>(null);

  useEffect(() => {
    checkAuthentication();
  }, []);

  const checkAuthentication = async () => {
    try {
      // Migrate old credentials from AsyncStorage to SecureStore (one-time)
      await SecureStorage.migrateFromAsyncStorage();

      // Get credentials from secure storage
      const credentials = await SecureStorage.getCredentials();

      if (credentials.token && credentials.apiUrl) {
        const icnClient = new ICNClient(credentials.apiUrl, credentials.token);
        setClient(icnClient);
        setIsAuthenticated(true);
      }
    } catch (error) {
      console.error('Auth check failed:', error);
    } finally {
      setIsLoading(false);
    }
  };

  const login = async (token: string, apiUrl: string, did?: string, coopId?: string) => {
    // Store credentials securely (encrypted in device keychain/keystore)
    await SecureStorage.setCredentials({
      token,
      apiUrl,
      did,
      coopId,
    });

    const icnClient = new ICNClient(apiUrl, token);
    setClient(icnClient);
    setIsAuthenticated(true);
  };

  const logout = async () => {
    // Clear all secure credentials
    await SecureStorage.clearCredentials();
    setClient(null);
    setIsAuthenticated(false);
  };

  if (isLoading) {
    return null; // Or a loading screen
  }

  return (
    <AuthContext.Provider value={{ isAuthenticated, client, login, logout }}>
      <PaperProvider theme={theme}>
        <NavigationContainer>
          <StatusBar style="auto" />
          {isAuthenticated ? (
            <Stack.Navigator screenOptions={{ headerShown: false }}>
              <Stack.Screen name="Main" component={MainTabs} />
              <Stack.Screen
                name="QRScanner"
                component={QRScannerScreen}
                options={{
                  headerShown: true,
                  headerTitle: 'Approve Web Login',
                  headerTransparent: true,
                  headerTintColor: '#fff',
                  headerStyle: { backgroundColor: 'transparent' },
                  presentation: 'modal',
                }}
              />
            </Stack.Navigator>
          ) : (
            <Stack.Navigator>
              <Stack.Screen 
                name="Login" 
                component={LoginScreen}
                options={{ headerShown: false }}
              />
            </Stack.Navigator>
          )}
        </NavigationContainer>
        {/* <NotificationCenter /> */}
      </PaperProvider>
    </AuthContext.Provider>
  );
}
