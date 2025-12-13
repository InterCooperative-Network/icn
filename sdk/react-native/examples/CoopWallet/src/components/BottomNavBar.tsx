/**
 * Bottom Navigation Bar Component
 *
 * Quick access navigation bar for key screens.
 */

import React from 'react';
import { View, Text, TouchableOpacity, StyleSheet } from 'react-native';
import { NativeStackNavigationProp } from '@react-navigation/native-stack';
import { RootStackParamList } from '../../App';

interface Props {
  navigation: NativeStackNavigationProp<RootStackParamList, any>;
  activeTab?: string;
  pendingEnrollments?: number;
}

interface NavItem {
  key: string;
  icon: string;
  label: string;
  screen: keyof RootStackParamList;
  params?: any;
  badge?: number;
}

export function BottomNavBar({ navigation, activeTab = 'Home', pendingEnrollments = 0 }: Props) {
  const navItems: NavItem[] = [
    { key: 'Home', icon: '🏠', label: 'Home', screen: 'Home' },
    { key: 'Governance', icon: '🗳️', label: 'Vote', screen: 'Governance' },
    { key: 'Identity', icon: '🪪', label: 'Identity', screen: 'Identity' },
    { key: 'Contacts', icon: '👥', label: 'Contacts', screen: 'Contacts' },
    { key: 'Settings', icon: '⚙️', label: 'Settings', screen: 'Settings' },
  ];

  // Add badge for steward pending enrollments on Identity tab
  if (pendingEnrollments > 0) {
    const identityItem = navItems.find(item => item.key === 'Identity');
    if (identityItem) {
      identityItem.badge = pendingEnrollments;
    }
  }

  const handlePress = (item: NavItem) => {
    if (item.key !== activeTab) {
      navigation.navigate(item.screen as any, item.params);
    }
  };

  return (
    <View style={styles.container}>
      {navItems.map(item => (
        <TouchableOpacity
          key={item.key}
          style={styles.navItem}
          onPress={() => handlePress(item)}
        >
          <View style={styles.iconContainer}>
            <Text style={[
              styles.icon,
              activeTab === item.key && styles.iconActive
            ]}>
              {item.icon}
            </Text>
            {item.badge && item.badge > 0 && (
              <View style={styles.badge}>
                <Text style={styles.badgeText}>
                  {item.badge > 99 ? '99+' : item.badge}
                </Text>
              </View>
            )}
          </View>
          <Text style={[
            styles.label,
            activeTab === item.key && styles.labelActive
          ]}>
            {item.label}
          </Text>
        </TouchableOpacity>
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flexDirection: 'row',
    backgroundColor: '#fff',
    borderTopWidth: 1,
    borderTopColor: '#e0e0e0',
    paddingVertical: 8,
    paddingHorizontal: 4,
    position: 'absolute',
    bottom: 0,
    left: 0,
    right: 0,
  },
  navItem: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    paddingVertical: 4,
  },
  iconContainer: {
    position: 'relative',
  },
  icon: {
    fontSize: 22,
    marginBottom: 2,
  },
  iconActive: {
    transform: [{ scale: 1.15 }],
  },
  label: {
    fontSize: 10,
    color: '#666',
    fontWeight: '500',
  },
  labelActive: {
    color: '#4a90d9',
    fontWeight: '600',
  },
  badge: {
    position: 'absolute',
    top: -4,
    right: -10,
    backgroundColor: '#f44336',
    borderRadius: 10,
    minWidth: 16,
    height: 16,
    justifyContent: 'center',
    alignItems: 'center',
    paddingHorizontal: 4,
  },
  badgeText: {
    color: '#fff',
    fontSize: 9,
    fontWeight: 'bold',
  },
});
