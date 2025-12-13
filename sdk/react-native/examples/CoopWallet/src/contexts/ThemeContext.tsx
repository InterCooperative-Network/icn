/**
 * Theme Context
 *
 * Provides light/dark theme support with system preference detection.
 */

import React, { createContext, useContext, useState, useEffect, ReactNode } from 'react';
import { useColorScheme } from 'react-native';
import * as SecureStore from 'expo-secure-store';

// Theme color definitions
export const lightTheme = {
  mode: 'light' as const,
  colors: {
    // Backgrounds
    background: '#f5f5f5',
    surface: '#ffffff',
    card: '#ffffff',

    // Text
    text: '#333333',
    textSecondary: '#666666',
    textMuted: '#999999',

    // Primary actions
    primary: '#4A90A4',
    primaryText: '#ffffff',

    // Accents
    success: '#4caf50',
    error: '#f44336',
    warning: '#ff9800',

    // Borders
    border: '#e0e0e0',
    borderLight: '#f0f0f0',

    // Special
    statusBar: 'dark',
  },
};

export const darkTheme = {
  mode: 'dark' as const,
  colors: {
    // Backgrounds
    background: '#121212',
    surface: '#1e1e1e',
    card: '#2d2d2d',

    // Text
    text: '#e0e0e0',
    textSecondary: '#b0b0b0',
    textMuted: '#808080',

    // Primary actions
    primary: '#5ba3b5',
    primaryText: '#ffffff',

    // Accents
    success: '#66bb6a',
    error: '#ef5350',
    warning: '#ffb74d',

    // Borders
    border: '#404040',
    borderLight: '#333333',

    // Special
    statusBar: 'light',
  },
};

export type Theme = typeof lightTheme | typeof darkTheme;
export type ThemeMode = 'light' | 'dark' | 'system';

interface ThemeContextValue {
  theme: Theme;
  themeMode: ThemeMode;
  setThemeMode: (mode: ThemeMode) => void;
  isDark: boolean;
}

const ThemeContext = createContext<ThemeContextValue | undefined>(undefined);

const THEME_STORAGE_KEY = 'icn_theme_mode';

export function ThemeProvider({ children }: { children: ReactNode }) {
  const systemColorScheme = useColorScheme();
  const [themeMode, setThemeModeState] = useState<ThemeMode>('system');
  const [isLoaded, setIsLoaded] = useState(false);

  // Load saved theme preference
  useEffect(() => {
    const loadTheme = async () => {
      try {
        const saved = await SecureStore.getItemAsync(THEME_STORAGE_KEY);
        if (saved && ['light', 'dark', 'system'].includes(saved)) {
          setThemeModeState(saved as ThemeMode);
        }
      } catch (e) {
        console.warn('Failed to load theme preference:', e);
      }
      setIsLoaded(true);
    };
    loadTheme();
  }, []);

  // Save theme preference
  const setThemeMode = async (mode: ThemeMode) => {
    setThemeModeState(mode);
    try {
      await SecureStore.setItemAsync(THEME_STORAGE_KEY, mode);
    } catch (e) {
      console.warn('Failed to save theme preference:', e);
    }
  };

  // Determine actual theme based on mode and system preference
  const isDark = themeMode === 'system'
    ? systemColorScheme === 'dark'
    : themeMode === 'dark';

  const theme = isDark ? darkTheme : lightTheme;

  // Don't render until theme is loaded to prevent flash
  if (!isLoaded) {
    return null;
  }

  return (
    <ThemeContext.Provider value={{ theme, themeMode, setThemeMode, isDark }}>
      {children}
    </ThemeContext.Provider>
  );
}

export function useTheme() {
  const context = useContext(ThemeContext);
  if (!context) {
    throw new Error('useTheme must be used within a ThemeProvider');
  }
  return context;
}

// Helper hook for styled components
export function useThemedStyles<T>(stylesFn: (theme: Theme) => T): T {
  const { theme } = useTheme();
  return stylesFn(theme);
}

export default ThemeContext;
