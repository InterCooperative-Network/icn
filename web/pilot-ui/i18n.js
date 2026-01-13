/**
 * ICN Pilot UI Internationalization Module
 *
 * A simple, framework-agnostic i18n implementation for vanilla JavaScript apps.
 * Supports:
 * - Automatic browser locale detection
 * - Dynamic locale loading
 * - Variable interpolation with {{variable}} syntax
 * - Fallback to English for missing translations
 *
 * Usage:
 * ```javascript
 * import { initI18n, t, setLocale } from './i18n.js';
 *
 * // Initialize (call once at app startup)
 * await initI18n();
 *
 * // Use translations
 * const text = t('nav.overview'); // "Overview"
 * const greeting = t('common.welcome', { name: 'Alice' }); // "Welcome, Alice"
 *
 * // Change language at runtime
 * await setLocale('es');
 * ```
 */

// Translation storage
const translations = {
  en: null,
};

// Current locale
let currentLocale = 'en';

// Whether i18n has been initialized
let initialized = false;

/**
 * Initialize internationalization
 * Detects browser locale and loads appropriate translations
 */
export async function initI18n() {
  if (initialized) return;

  // Get browser locale (e.g., "en-US" -> "en")
  const browserLocale = getBrowserLocale();

  // Load the locale
  await loadLocale(browserLocale);

  initialized = true;
}

/**
 * Get the browser's preferred language
 * @returns {string} Two-letter language code
 */
function getBrowserLocale() {
  try {
    const locale = navigator.language || navigator.userLanguage || 'en';
    return locale.split('-')[0].toLowerCase();
  } catch {
    return 'en';
  }
}

/**
 * Load translations for a specific locale
 * @param {string} locale - Two-letter language code (e.g., 'en', 'es')
 */
export async function loadLocale(locale) {
  try {
    // Try to load the requested locale
    const response = await fetch(`locales/${locale}.json`);

    if (response.ok) {
      translations[locale] = await response.json();
      currentLocale = locale;
      return;
    }

    // If locale not found and not already English, fall back to English
    if (locale !== 'en' && !translations.en) {
      const enResponse = await fetch('locales/en.json');
      if (enResponse.ok) {
        translations.en = await enResponse.json();
      }
    }

    currentLocale = 'en';
  } catch (error) {
    console.warn(`Failed to load locale '${locale}':`, error);

    // Ensure English is loaded as fallback
    if (!translations.en) {
      try {
        const enResponse = await fetch('locales/en.json');
        if (enResponse.ok) {
          translations.en = await enResponse.json();
        }
      } catch (enError) {
        console.error('Failed to load English locale:', enError);
      }
    }

    currentLocale = 'en';
  }
}

/**
 * Get a translated string
 *
 * SECURITY NOTE: This function returns raw strings from locale files.
 * When using the result in DOM manipulation:
 * - Use textContent instead of innerHTML when possible
 * - If innerHTML is required, sanitize the output first
 * - Locale files are trusted, but interpolated params could be user input
 *
 * @param {string} key - Dot-notation key (e.g., 'nav.overview', 'common.loading')
 * @param {Object} params - Optional parameters for interpolation
 * @returns {string} Translated string, or key if not found
 */
export function t(key, params = {}) {
  const keys = key.split('.');
  let value = translations[currentLocale] || translations.en;

  // Navigate to the nested value
  for (const k of keys) {
    if (value && typeof value === 'object') {
      value = value[k];
    } else {
      // Key not found, return the key itself
      return key;
    }
  }

  // If value is not a string, return the key
  if (typeof value !== 'string') {
    return key;
  }

  // Perform variable interpolation: {{variable}} -> value
  // Note: Params are inserted as-is. If using innerHTML, sanitize params externally.
  return value.replace(/\{\{(\w+)\}\}/g, (match, name) => {
    return params[name] !== undefined ? String(params[name]) : match;
  });
}

/**
 * Change the application locale at runtime
 * @param {string} locale - Two-letter language code
 */
export async function setLocale(locale) {
  await loadLocale(locale);

  // Dispatch event for components to update
  window.dispatchEvent(
    new CustomEvent('localechange', { detail: { locale: currentLocale } })
  );
}

/**
 * Get the current locale
 * @returns {string} Current two-letter language code
 */
export function getLocale() {
  return currentLocale;
}

/**
 * Get list of available locales
 * @returns {string[]} Array of available locale codes
 */
export function getAvailableLocales() {
  return Object.keys(translations).filter((locale) => translations[locale] !== null);
}

/**
 * Check if a locale is available
 * @param {string} locale - Two-letter language code
 * @returns {boolean} True if locale is available
 */
export function isLocaleAvailable(locale) {
  return translations[locale] !== null;
}

// Export default for module compatibility
export default {
  initI18n,
  loadLocale,
  t,
  setLocale,
  getLocale,
  getAvailableLocales,
  isLocaleAvailable,
};
