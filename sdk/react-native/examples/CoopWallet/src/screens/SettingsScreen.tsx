/**
 * Settings Screen
 *
 * App settings including security, notifications, and account management.
 */

import React, { useState, useEffect } from 'react';
import {
  View,
  Text,
  StyleSheet,
  ScrollView,
  TouchableOpacity,
  Switch,
  Alert,
  Linking,
} from 'react-native';
import * as LocalAuthentication from 'expo-local-authentication';
import * as SecureStore from 'expo-secure-store';
import { ICNMobileClient } from '@icn/react-native';
import { useTheme, ThemeMode, Theme } from '../contexts/ThemeContext';

interface SettingsScreenProps {
  client: ICNMobileClient;
  userDid: string;
  onLogout: () => void;
}

const BIOMETRICS_ENABLED_KEY = 'icn_biometrics_enabled';

export function SettingsScreen({ client, userDid, onLogout }: SettingsScreenProps) {
  const { theme, themeMode, setThemeMode, isDark } = useTheme();
  const styles = createStyles(theme);
  const [biometricsAvailable, setBiometricsAvailable] = useState(false);
  const [biometricsEnabled, setBiometricsEnabled] = useState(false);
  const [biometricType, setBiometricType] = useState<string>('Biometrics');

  const themeModeLabel = {
    light: 'Light',
    dark: 'Dark',
    system: 'System',
  };

  const cycleThemeMode = () => {
    const modes: ThemeMode[] = ['system', 'light', 'dark'];
    const currentIndex = modes.indexOf(themeMode);
    const nextIndex = (currentIndex + 1) % modes.length;
    setThemeMode(modes[nextIndex]);
  };

  useEffect(() => {
    checkBiometrics();
    loadBiometricsPreference();
  }, []);

  const checkBiometrics = async () => {
    try {
      const compatible = await LocalAuthentication.hasHardwareAsync();
      const enrolled = await LocalAuthentication.isEnrolledAsync();
      setBiometricsAvailable(compatible && enrolled);

      if (compatible) {
        const types = await LocalAuthentication.supportedAuthenticationTypesAsync();
        if (types.includes(LocalAuthentication.AuthenticationType.FACIAL_RECOGNITION)) {
          setBiometricType('Face ID');
        } else if (types.includes(LocalAuthentication.AuthenticationType.FINGERPRINT)) {
          setBiometricType('Fingerprint');
        }
      }
    } catch (error) {
      console.warn('Biometrics check failed:', error);
    }
  };

  const loadBiometricsPreference = async () => {
    try {
      const enabled = await SecureStore.getItemAsync(BIOMETRICS_ENABLED_KEY);
      setBiometricsEnabled(enabled === 'true');
    } catch (error) {
      console.warn('Failed to load biometrics preference:', error);
    }
  };

  const toggleBiometrics = async (enabled: boolean) => {
    if (enabled) {
      // Verify biometrics before enabling
      const result = await LocalAuthentication.authenticateAsync({
        promptMessage: `Enable ${biometricType} for Coop Wallet`,
        cancelLabel: 'Cancel',
        disableDeviceFallback: false,
      });

      if (!result.success) {
        Alert.alert('Authentication Failed', 'Could not verify your identity.');
        return;
      }
    }

    try {
      await SecureStore.setItemAsync(BIOMETRICS_ENABLED_KEY, enabled ? 'true' : 'false');
      setBiometricsEnabled(enabled);
    } catch (error) {
      Alert.alert('Error', 'Failed to save preference.');
    }
  };

  const handleLogout = () => {
    Alert.alert(
      'Log Out',
      'Are you sure you want to log out? Your keys will remain on this device.',
      [
        { text: 'Cancel', style: 'cancel' },
        {
          text: 'Log Out',
          style: 'destructive',
          onPress: async () => {
            await client.logout();
            onLogout();
          },
        },
      ]
    );
  };

  const handleDeleteWallet = () => {
    Alert.alert(
      'Delete Wallet',
      'This will permanently delete your keys from this device. Make sure you have a backup! This cannot be undone.',
      [
        { text: 'Cancel', style: 'cancel' },
        {
          text: 'Delete',
          style: 'destructive',
          onPress: () => {
            Alert.alert(
              'Confirm Deletion',
              'Type DELETE to confirm wallet deletion.',
              [
                { text: 'Cancel', style: 'cancel' },
                {
                  text: 'I Understand',
                  style: 'destructive',
                  onPress: async () => {
                    // This would delete the wallet keys
                    Alert.alert('Not Implemented', 'Wallet deletion is disabled for safety.');
                  },
                },
              ]
            );
          },
        },
      ]
    );
  };

  const handleExportKeys = () => {
    Alert.alert(
      'Export Keys',
      'This will display your private key. Only do this in a secure location.',
      [
        { text: 'Cancel', style: 'cancel' },
        {
          text: 'Export',
          onPress: () => {
            Alert.alert('Not Implemented', 'Key export will be available in a future update.');
          },
        },
      ]
    );
  };

  const openPrivacyPolicy = () => {
    Linking.openURL('https://icn.zone/privacy');
  };

  const openTerms = () => {
    Linking.openURL('https://icn.zone/terms');
  };

  const openSupport = () => {
    Linking.openURL('https://github.com/InterCooperative-Network/icn/issues');
  };

  const renderSection = (title: string, children: React.ReactNode) => (
    <View style={styles.section}>
      <Text style={styles.sectionTitle}>{title}</Text>
      <View style={styles.sectionContent}>{children}</View>
    </View>
  );

  const renderSettingRow = (
    label: string,
    description?: string,
    right?: React.ReactNode,
    onPress?: () => void
  ) => (
    <TouchableOpacity
      style={styles.settingRow}
      onPress={onPress}
      disabled={!onPress && !right}
      activeOpacity={onPress ? 0.7 : 1}
    >
      <View style={styles.settingInfo}>
        <Text style={styles.settingLabel}>{label}</Text>
        {description && <Text style={styles.settingDescription}>{description}</Text>}
      </View>
      {right}
    </TouchableOpacity>
  );

  return (
    <ScrollView style={styles.container} contentContainerStyle={styles.content}>
      {/* Account Section */}
      {renderSection(
        'Account',
        <>
          {renderSettingRow(
            'Identity',
            userDid ? `${userDid.slice(0, 20)}...` : 'Not logged in',
            <Text style={styles.chevron}>›</Text>
          )}
          {renderSettingRow(
            'Cooperative',
            client.authState.coopId || 'Not connected',
            <Text style={styles.chevron}>›</Text>
          )}
        </>
      )}

      {/* Security Section */}
      {renderSection(
        'Security',
        <>
          {biometricsAvailable &&
            renderSettingRow(
              `Use ${biometricType}`,
              'Require biometric authentication to open the app',
              <Switch
                value={biometricsEnabled}
                onValueChange={toggleBiometrics}
                trackColor={{ false: theme.colors.border, true: theme.colors.primary }}
              />
            )}
          {renderSettingRow(
            'Export Keys',
            'Backup your private key',
            <Text style={styles.chevron}>›</Text>,
            handleExportKeys
          )}
          {renderSettingRow(
            'Change PIN',
            'Update your app PIN',
            <Text style={styles.chevron}>›</Text>,
            () => Alert.alert('Not Implemented', 'PIN management coming soon.')
          )}
        </>
      )}

      {/* Appearance Section */}
      {renderSection(
        'Appearance',
        <>
          {renderSettingRow(
            'Theme',
            `Current: ${themeModeLabel[themeMode]}${themeMode === 'system' ? ` (${isDark ? 'Dark' : 'Light'})` : ''}`,
            <View style={styles.themeButtons}>
              {(['system', 'light', 'dark'] as ThemeMode[]).map((mode) => (
                <TouchableOpacity
                  key={mode}
                  style={[
                    styles.themeButton,
                    themeMode === mode && styles.themeButtonActive,
                  ]}
                  onPress={() => setThemeMode(mode)}
                >
                  <Text
                    style={[
                      styles.themeButtonText,
                      themeMode === mode && styles.themeButtonTextActive,
                    ]}
                  >
                    {themeModeLabel[mode]}
                  </Text>
                </TouchableOpacity>
              ))}
            </View>
          )}
        </>
      )}

      {/* Notifications Section */}
      {renderSection(
        'Notifications',
        <>
          {renderSettingRow(
            'Push Notifications',
            'Receive alerts for payments and proposals',
            <Switch
              value={true}
              onValueChange={() => Alert.alert('Not Implemented', 'Notification settings coming soon.')}
              trackColor={{ false: theme.colors.border, true: theme.colors.primary }}
            />
          )}
          {renderSettingRow(
            'Payment Alerts',
            'Notify when you receive a payment',
            <Switch
              value={true}
              onValueChange={() => {}}
              trackColor={{ false: theme.colors.border, true: theme.colors.primary }}
            />
          )}
        </>
      )}

      {/* About Section */}
      {renderSection(
        'About',
        <>
          {renderSettingRow(
            'Version',
            undefined,
            <Text style={styles.versionText}>0.1.0</Text>
          )}
          {renderSettingRow(
            'Privacy Policy',
            undefined,
            <Text style={styles.chevron}>›</Text>,
            openPrivacyPolicy
          )}
          {renderSettingRow(
            'Terms of Service',
            undefined,
            <Text style={styles.chevron}>›</Text>,
            openTerms
          )}
          {renderSettingRow(
            'Support',
            undefined,
            <Text style={styles.chevron}>›</Text>,
            openSupport
          )}
        </>
      )}

      {/* Danger Zone */}
      {renderSection(
        'Danger Zone',
        <>
          <TouchableOpacity style={styles.dangerButton} onPress={handleLogout}>
            <Text style={styles.dangerButtonText}>Log Out</Text>
          </TouchableOpacity>
          <TouchableOpacity style={[styles.dangerButton, styles.deleteButton]} onPress={handleDeleteWallet}>
            <Text style={styles.deleteButtonText}>Delete Wallet</Text>
          </TouchableOpacity>
        </>
      )}

      {/* Footer */}
      <View style={styles.footer}>
        <Text style={styles.footerText}>InterCooperative Network</Text>
        <Text style={styles.footerSubtext}>Building the cooperative internet</Text>
      </View>
    </ScrollView>
  );
}

const createStyles = (theme: Theme) => StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: theme.colors.background,
  },
  content: {
    paddingBottom: 32,
  },
  section: {
    marginTop: 24,
  },
  sectionTitle: {
    fontSize: 12,
    fontWeight: '600',
    color: theme.colors.textMuted,
    textTransform: 'uppercase',
    letterSpacing: 1,
    marginHorizontal: 16,
    marginBottom: 8,
  },
  sectionContent: {
    backgroundColor: theme.colors.card,
    borderTopWidth: 1,
    borderBottomWidth: 1,
    borderColor: theme.colors.border,
  },
  settingRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    paddingVertical: 14,
    paddingHorizontal: 16,
    borderBottomWidth: StyleSheet.hairlineWidth,
    borderBottomColor: theme.colors.border,
  },
  settingInfo: {
    flex: 1,
    marginRight: 12,
  },
  settingLabel: {
    fontSize: 16,
    color: theme.colors.text,
  },
  settingDescription: {
    fontSize: 12,
    color: theme.colors.textMuted,
    marginTop: 2,
  },
  chevron: {
    fontSize: 20,
    color: theme.colors.textMuted,
  },
  themeButtons: {
    flexDirection: 'row',
    gap: 8,
  },
  themeButton: {
    paddingHorizontal: 12,
    paddingVertical: 6,
    borderRadius: 16,
    backgroundColor: theme.colors.borderLight,
    borderWidth: 1,
    borderColor: 'transparent',
  },
  themeButtonActive: {
    backgroundColor: theme.colors.primary,
    borderColor: theme.colors.primary,
  },
  themeButtonText: {
    fontSize: 12,
    fontWeight: '600',
    color: theme.colors.textSecondary,
  },
  themeButtonTextActive: {
    color: '#fff',
  },
  versionText: {
    fontSize: 14,
    color: theme.colors.textMuted,
  },
  dangerButton: {
    margin: 16,
    marginBottom: 8,
    padding: 16,
    backgroundColor: theme.colors.card,
    borderRadius: 8,
    borderWidth: 1,
    borderColor: theme.colors.error,
    alignItems: 'center',
  },
  dangerButtonText: {
    fontSize: 16,
    fontWeight: '600',
    color: theme.colors.error,
  },
  deleteButton: {
    backgroundColor: theme.colors.error,
    borderColor: theme.colors.error,
    marginTop: 0,
  },
  deleteButtonText: {
    fontSize: 16,
    fontWeight: '600',
    color: '#fff',
  },
  footer: {
    marginTop: 32,
    alignItems: 'center',
    paddingBottom: 16,
  },
  footerText: {
    fontSize: 14,
    fontWeight: '600',
    color: theme.colors.textMuted,
  },
  footerSubtext: {
    fontSize: 12,
    color: theme.colors.textMuted,
    marginTop: 4,
  },
});

export default SettingsScreen;
