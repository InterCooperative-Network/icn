# Internationalization (i18n) Migration Guide

This guide explains how to add internationalization to ICN components across Rust, React Native, and web platforms.

## Overview

ICN uses different i18n libraries optimized for each platform:

| Platform | Library | Locale Format | Location |
|----------|---------|---------------|----------|
| Rust (CLI, Gateway) | `rust-i18n` | YAML | `<crate>/locales/` |
| React Native | `react-i18next` | JSON | `src/i18n/locales/` |
| Web (Vanilla JS) | Custom module | JSON | `locales/` |

### Translation Key Naming Convention

Use dot-notation with consistent hierarchy:

```
{component}.{section}.{message}
```

Examples:
- `cli.status.connected` - CLI status message
- `nav.overview` - Navigation item
- `common.loading` - Shared UI string
- `auth.signIn` - Authentication action

### Variable Interpolation

| Platform | Syntax | Example |
|----------|--------|---------|
| Rust | `{variable}` | `t!("greeting", name = "Alice")` |
| React/JS | `{{variable}}` | `t('greeting', { name: 'Alice' })` |

---

## Rust Components

### Adding rust-i18n to a Crate

**1. Add dependency to `Cargo.toml`:**

```toml
[dependencies]
rust-i18n = "3"
```

**2. Initialize in `main.rs` or `lib.rs`:**

```rust
use rust_i18n::t;

// Load translations from ./locales directory, fallback to English
rust_i18n::i18n!("locales", fallback = "en");
```

**3. Create locale file at `locales/en.yaml`:**

```yaml
# ICN Component Translations - English
# Format: {component}.{section}.{message}
# Variables: {var_name} for interpolation

cli:
  common:
    no_identity: "No identity found. Run 'icnctl id init' to create one."
    error: "Error"
    success: "Success"

  status:
    checking: "Checking daemon status..."
    connected: "Connected to ICN daemon"
    not_running: "Daemon is not running"
```

### Using the `t!()` Macro

**Basic translation:**

```rust
println!("{}", t!("cli.status.connected"));
```

**With variable interpolation:**

```rust
// Locale file: greeting: "Welcome, {name}!"
println!("{}", t!("greeting", name = user_name));
```

**Setting locale at runtime:**

```rust
// Get locale from environment or config
let locale = std::env::var("ICN_LOCALE").unwrap_or_else(|_| "en".into());
rust_i18n::set_locale(&locale);
```

### Example: icnctl CLI

See `icn/bins/icnctl/` for a complete example:

```rust
// icn/bins/icnctl/src/main.rs
use rust_i18n::t;

rust_i18n::i18n!("locales", fallback = "en");

fn main() {
    // Set locale from environment
    if let Ok(locale) = std::env::var("ICN_LOCALE") {
        rust_i18n::set_locale(&locale);
    }

    println!("{}\n", t!("cli.id.init.starting"));
    // ... rest of implementation
}
```

---

## React Native

### Setting Up react-i18next

**1. Install dependencies:**

```bash
npm install i18next react-i18next expo-localization
```

**2. Create `src/i18n/index.ts`:**

```typescript
import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import * as Localization from 'expo-localization';

// Import locale files
import en from './locales/en.json';

const resources = {
  en: { translation: en },
  // Add more locales here:
  // es: { translation: es },
};

// Get device locale
const getDeviceLocale = (): string => {
  try {
    const deviceLocale = Localization.locale?.split('-')[0] || 'en';
    return deviceLocale in resources ? deviceLocale : 'en';
  } catch {
    return 'en';
  }
};

i18n.use(initReactI18next).init({
  resources,
  lng: getDeviceLocale(),
  fallbackLng: 'en',
  interpolation: {
    escapeValue: false, // React already escapes
  },
  react: {
    useSuspense: false,
  },
});

export default i18n;
```

**3. Create locale file at `src/i18n/locales/en.json`:**

```json
{
  "common": {
    "loading": "Loading...",
    "error": "An error occurred",
    "retry": "Retry",
    "cancel": "Cancel"
  },

  "nav": {
    "home": "Home",
    "payments": "Payments",
    "settings": "Settings"
  },

  "home": {
    "title": "Home",
    "welcome": "Welcome back",
    "balance": {
      "label": "Available Balance",
      "currency": "hours"
    }
  }
}
```

**4. Initialize in app entry point:**

```typescript
// App.tsx or index.ts
import './i18n';
```

### Using the useTranslation Hook

```tsx
import { useTranslation } from 'react-i18next';

function HomeScreen() {
  const { t } = useTranslation();

  return (
    <View>
      <Text>{t('home.title')}</Text>
      <Text>{t('home.welcome')}</Text>
      <Text>{t('home.balance.label')}: 42 {t('home.balance.currency')}</Text>
    </View>
  );
}
```

**With interpolation:**

```tsx
// Locale: "greeting": "Hello, {{name}}!"
<Text>{t('greeting', { name: 'Alice' })}</Text>
```

**Changing language at runtime:**

```typescript
import i18n from './i18n';

const changeLanguage = async (lang: string) => {
  await i18n.changeLanguage(lang);
};
```

---

## Web (Vanilla JavaScript)

### Using the i18n Module

ICN includes a lightweight, framework-agnostic i18n module for vanilla JavaScript.

**1. Import and initialize:**

```javascript
import { initI18n, t, setLocale } from './i18n.js';

// Initialize (call once at app startup)
await initI18n();
```

**2. Create locale file at `locales/en.json`:**

```json
{
  "app": {
    "title": "ICN Timebank",
    "subtitle": "Cooperative Time Banking"
  },

  "nav": {
    "overview": "Overview",
    "network": "Network",
    "ledger": "Ledger"
  },

  "status": {
    "online": "Online",
    "offline": "Offline",
    "connecting": "Connecting..."
  }
}
```

### Using Translations

**Basic translation:**

```javascript
const title = t('app.title'); // "ICN Timebank"
```

**With interpolation:**

```javascript
// Locale: "welcome": "Welcome, {{name}}!"
const greeting = t('common.welcome', { name: 'Alice' }); // "Welcome, Alice!"
```

**Changing locale at runtime:**

```javascript
await setLocale('es');
```

### XSS Prevention

When rendering translations to the DOM, always use safe methods:

```javascript
// SAFE: Use textContent for plain text (recommended)
element.textContent = t('app.title');

// SAFE: Use DOM methods for structured content
const span = document.createElement('span');
span.textContent = t('greeting', { name: userName });
container.appendChild(span);
```

The i18n module returns raw strings. Locale files are trusted, but interpolated params could contain user input.

### Listening for Locale Changes

```javascript
window.addEventListener('localechange', (event) => {
  console.log(`Locale changed to: ${event.detail.locale}`);
  // Update UI components
});
```

---

## Adding New Languages

### Checklist

For each platform, create the corresponding locale file:

- [ ] **Rust**: `<crate>/locales/<lang>.yaml`
- [ ] **React Native**: `src/i18n/locales/<lang>.json`
- [ ] **Web**: `locales/<lang>.json`

Then register the new locale:

- [ ] **Rust**: No registration needed (auto-loaded)
- [ ] **React Native**: Import and add to `resources` in `src/i18n/index.ts`
- [ ] **Web**: Auto-loaded by `loadLocale()` function

### Translation Process

1. **Copy the English locale file** as a starting point
2. **Translate all strings** while preserving:
   - Key structure (don't rename keys)
   - Variable placeholders (`{var}` or `{{var}}`)
   - HTML entities if present
3. **Request review** from native speakers in the community
4. **Test the translations** in the actual UI

### Testing Translations

**Rust:**

```bash
ICN_LOCALE=es cargo run -- status
```

**React Native:**

```typescript
import i18n from './i18n';
i18n.changeLanguage('es');
```

**Web:**

```javascript
await setLocale('es');
```

### Language Codes

Use [ISO 639-1](https://en.wikipedia.org/wiki/List_of_ISO_639-1_codes) two-letter codes:

| Language | Code |
|----------|------|
| English | `en` |
| Spanish | `es` |
| French | `fr` |
| German | `de` |
| Portuguese | `pt` |
| Chinese | `zh` |

---

## Best Practices

### 1. Avoid Concatenating Translated Strings

Word order varies by language.

```javascript
// BAD: "Hello" + " " + name
t('hello') + ' ' + name;

// GOOD: Use interpolation
t('greeting', { name });
```

### 2. Keep Translations Context-Aware

Include context in key names:

```yaml
# BAD: ambiguous
button: "Submit"

# GOOD: clear context
payment.submitButton: "Send Payment"
proposal.submitButton: "Submit Proposal"
```

### 3. Handle Pluralization

Some languages have complex plural rules.

```javascript
// Use count-based keys
{
  "items": {
    "one": "{{count}} item",
    "other": "{{count}} items"
  }
}
```

### 4. Test with Long Strings

German and Finnish often produce longer strings than English. Test UI layout with these languages.

### 5. Keep Locale Files Organized

Group related strings:

```yaml
# Group by feature/screen
payments:
  title: "Payments"
  send: "Send Payment"
  receive: "Receive"
  history: "Transaction History"

governance:
  title: "Governance"
  proposals: "Proposals"
  vote: "Vote"
```

---

## Related Documentation

- [Contributing Guide](../CONTRIBUTING.md) - General contribution guidelines
- [icnctl locales](../icn/bins/icnctl/locales/) - CLI translation files
- [Gateway locales](../icn/crates/icn-gateway/locales/) - Gateway API messages
- [Web UI locales](../web/pilot-ui/locales/) - Web interface translations
- [CoopWallet i18n](../sdk/react-native/examples/CoopWallet/src/i18n/) - Mobile app example
