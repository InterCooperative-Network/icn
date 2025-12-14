/**
 * LoadingState Component
 *
 * Consistent loading indicator with optional message.
 * Uses theme colors for proper dark mode support.
 */

import React from 'react';
import { View, Text, ActivityIndicator, StyleSheet } from 'react-native';
import { useTheme } from '../contexts/ThemeContext';

interface LoadingStateProps {
  /** Optional message to display below the spinner */
  message?: string;
  /** Size of the spinner - defaults to 'large' */
  size?: 'small' | 'large';
  /** Whether to show in full-screen centered mode */
  fullScreen?: boolean;
  /** Use inverted colors (for dark backgrounds like buttons) */
  inverted?: boolean;
}

export function LoadingState({
  message,
  size = 'large',
  fullScreen = false,
  inverted = false,
}: LoadingStateProps) {
  const { theme } = useTheme();

  const containerStyle = fullScreen
    ? [styles.container, styles.fullScreen, { backgroundColor: theme.colors.background }]
    : styles.container;

  const spinnerColor = inverted ? theme.colors.primaryText : theme.colors.primary;
  const textColor = inverted ? theme.colors.primaryText : theme.colors.textSecondary;

  return (
    <View style={containerStyle}>
      <ActivityIndicator size={size} color={spinnerColor} />
      {message && (
        <Text style={[styles.message, { color: textColor }]}>{message}</Text>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    padding: 40,
    alignItems: 'center',
    justifyContent: 'center',
  },
  fullScreen: {
    flex: 1,
  },
  message: {
    marginTop: 12,
    fontSize: 14,
    textAlign: 'center',
  },
});

export default LoadingState;
