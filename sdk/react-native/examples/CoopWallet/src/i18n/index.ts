/**
 * ICN CoopWallet Internationalization Configuration
 *
 * This module sets up i18next for the mobile app with:
 * - Automatic device locale detection via expo-localization
 * - Fallback to English for missing translations
 * - React integration via react-i18next hooks
 *
 * Usage in components:
 * ```tsx
 * import { useTranslation } from 'react-i18next';
 *
 * function MyComponent() {
 *   const { t } = useTranslation();
 *   return <Text>{t('home.title')}</Text>;
 * }
 * ```
 */

import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import * as Localization from 'expo-localization';

// Import locale files
import en from './locales/en.json';

// Available locales - add more as translations become available
const resources = {
  en: { translation: en },
  // Future locales:
  // es: { translation: es },
  // fr: { translation: fr },
};

// Get the device's preferred language, falling back to English
const getDeviceLocale = (): string => {
  try {
    // Get locale like "en-US" and extract language code "en"
    const deviceLocale = Localization.locale?.split('-')[0] || 'en';

    // Check if we have translations for this language
    if (deviceLocale in resources) {
      return deviceLocale;
    }

    // Fallback to English
    return 'en';
  } catch {
    return 'en';
  }
};

i18n.use(initReactI18next).init({
  resources,
  lng: getDeviceLocale(),
  fallbackLng: 'en',

  interpolation: {
    // React already escapes values, so we don't need i18next to do it
    escapeValue: false,
  },

  // React-specific configuration
  react: {
    // Suspend while loading translations (useful for async loading)
    useSuspense: false,
  },
});

/**
 * Change the app's language at runtime
 * @param languageCode - ISO 639-1 language code (e.g., 'en', 'es', 'fr')
 */
export const changeLanguage = async (languageCode: string): Promise<void> => {
  await i18n.changeLanguage(languageCode);
};

/**
 * Get the current language
 */
export const getCurrentLanguage = (): string => {
  return i18n.language;
};

/**
 * Get list of available languages
 */
export const getAvailableLanguages = (): string[] => {
  return Object.keys(resources);
};

export default i18n;
