/**
 * Identicon Component
 *
 * Generates unique visual avatars based on DID strings.
 * Uses a simple hash-based color scheme with geometric patterns.
 */

import React, { useMemo } from 'react';
import { View, Text, StyleSheet, ViewStyle } from 'react-native';

interface IdenticonProps {
  did: string;
  size?: number;
  style?: ViewStyle;
}

// Simple hash function for consistent colors
const hashCode = (str: string): number => {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    const char = str.charCodeAt(i);
    hash = ((hash << 5) - hash) + char;
    hash = hash & hash; // Convert to 32bit integer
  }
  return Math.abs(hash);
};

// Generate HSL color from hash
const hashToColor = (hash: number, saturation = 70, lightness = 50): string => {
  const hue = hash % 360;
  return `hsl(${hue}, ${saturation}%, ${lightness}%)`;
};

// Generate background gradient colors
const getColors = (did: string) => {
  const hash1 = hashCode(did);
  const hash2 = hashCode(did + 'secondary');

  return {
    primary: hashToColor(hash1, 65, 55),
    secondary: hashToColor(hash2, 55, 45),
    accent: hashToColor(hash1 + hash2, 70, 70),
  };
};

// Get initials from DID
const getInitials = (did: string): string => {
  if (!did) return '?';

  // Extract the unique part after did:icn:
  const parts = did.split(':');
  const identifier = parts[parts.length - 1] || '';

  if (identifier.length >= 2) {
    return identifier.slice(0, 2).toUpperCase();
  }
  return identifier.toUpperCase() || '?';
};

// Generate a simple pattern based on hash
const getPattern = (did: string): number => {
  const hash = hashCode(did);
  return hash % 4; // 4 different patterns
};

export function Identicon({ did, size = 40, style }: IdenticonProps) {
  const { primary, secondary, accent } = useMemo(() => getColors(did), [did]);
  const initials = useMemo(() => getInitials(did), [did]);
  const pattern = useMemo(() => getPattern(did), [did]);

  const fontSize = size * 0.4;
  const borderRadius = size / 2;

  // Generate pattern elements
  const renderPattern = () => {
    const patternSize = size * 0.3;

    switch (pattern) {
      case 0: // Circle in top-right
        return (
          <View
            style={[
              styles.patternElement,
              {
                width: patternSize,
                height: patternSize,
                borderRadius: patternSize / 2,
                backgroundColor: accent,
                top: 2,
                right: 2,
                opacity: 0.6,
              },
            ]}
          />
        );
      case 1: // Two dots
        return (
          <>
            <View
              style={[
                styles.patternElement,
                {
                  width: patternSize * 0.5,
                  height: patternSize * 0.5,
                  borderRadius: patternSize / 4,
                  backgroundColor: accent,
                  top: 4,
                  left: 4,
                  opacity: 0.5,
                },
              ]}
            />
            <View
              style={[
                styles.patternElement,
                {
                  width: patternSize * 0.5,
                  height: patternSize * 0.5,
                  borderRadius: patternSize / 4,
                  backgroundColor: accent,
                  bottom: 4,
                  right: 4,
                  opacity: 0.5,
                },
              ]}
            />
          </>
        );
      case 2: // Ring
        return (
          <View
            style={[
              styles.patternElement,
              {
                width: size - 6,
                height: size - 6,
                borderRadius: (size - 6) / 2,
                borderWidth: 2,
                borderColor: accent,
                top: 3,
                left: 3,
                opacity: 0.4,
              },
            ]}
          />
        );
      case 3: // Triangle indicator
        return (
          <View
            style={[
              styles.patternElement,
              {
                width: 0,
                height: 0,
                borderLeftWidth: patternSize / 2,
                borderRightWidth: patternSize / 2,
                borderBottomWidth: patternSize,
                borderLeftColor: 'transparent',
                borderRightColor: 'transparent',
                borderBottomColor: accent,
                top: 2,
                right: 2,
                opacity: 0.5,
              },
            ]}
          />
        );
      default:
        return null;
    }
  };

  return (
    <View
      style={[
        styles.container,
        {
          width: size,
          height: size,
          borderRadius,
          backgroundColor: primary,
        },
        style,
      ]}
    >
      {/* Gradient overlay effect */}
      <View
        style={[
          styles.gradientOverlay,
          {
            borderRadius,
            backgroundColor: secondary,
          },
        ]}
      />

      {/* Pattern elements */}
      {renderPattern()}

      {/* Initials */}
      <Text
        style={[
          styles.initials,
          {
            fontSize,
            lineHeight: fontSize * 1.2,
          },
        ]}
      >
        {initials}
      </Text>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    justifyContent: 'center',
    alignItems: 'center',
    overflow: 'hidden',
  },
  gradientOverlay: {
    position: 'absolute',
    top: 0,
    left: 0,
    right: 0,
    bottom: '50%',
    opacity: 0.3,
  },
  patternElement: {
    position: 'absolute',
  },
  initials: {
    color: '#fff',
    fontWeight: '700',
    textShadowColor: 'rgba(0, 0, 0, 0.3)',
    textShadowOffset: { width: 0, height: 1 },
    textShadowRadius: 2,
  },
});

export default Identicon;
