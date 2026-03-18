/**
 * ErrorState Component
 *
 * Consistent error display with optional retry action.
 * Uses theme colors for proper dark mode support.
 */

import React from 'react';
import { View, Text, TouchableOpacity, StyleSheet } from 'react-native';
import { useTheme, Theme } from '../contexts/ThemeContext';

interface ErrorStateProps {
  /** Error message to display */
  message: string;
  /** Optional retry callback */
  onRetry?: () => void;
  /** Text for retry button - defaults to 'Retry' */
  retryText?: string;
  /** Whether to show in full-screen centered mode */
  fullScreen?: boolean;
}

export function ErrorState({
  message,
  onRetry,
  retryText = 'Retry',
  fullScreen = false,
}: ErrorStateProps) {
  const { theme } = useTheme();
  const styles = createStyles(theme);

  const containerStyle = fullScreen
    ? [styles.container, styles.fullScreen]
    : styles.container;

  return (
    <View style={containerStyle}>
      <Text style={styles.errorText} accessibilityRole="alert">
        {message}
      </Text>
      {onRetry && (
        <TouchableOpacity
          style={styles.retryButton}
          onPress={onRetry}
          accessibilityRole="button"
          accessibilityLabel={`${retryText}: ${message}`}
        >
          <Text style={styles.retryText}>{retryText}</Text>
        </TouchableOpacity>
      )}
    </View>
  );
}

const createStyles = (theme: Theme) =>
  StyleSheet.create({
    container: {
      padding: 40,
      alignItems: 'center',
      justifyContent: 'center',
    },
    fullScreen: {
      flex: 1,
      backgroundColor: theme.colors.background,
    },
    errorText: {
      fontSize: 14,
      color: theme.colors.error,
      textAlign: 'center',
      marginBottom: 16,
    },
    retryButton: {
      backgroundColor: theme.colors.primary,
      paddingHorizontal: 24,
      paddingVertical: 12,
      borderRadius: 8,
    },
    retryText: {
      color: theme.colors.primaryText,
      fontSize: 14,
      fontWeight: '600',
    },
  });

export default ErrorState;
