/**
 * Trust Badge Component
 * 
 * Displays a color-coded trust score badge with classification label.
 */

import React from 'react';
import { View, Text, StyleSheet } from 'react-native';

interface TrustBadgeProps {
  trustScore: number;
  trustClass?: 'Isolated' | 'Known' | 'Partner' | 'Federated';
  size?: 'small' | 'medium' | 'large';
}

/**
 * Get color for trust score
 */
function getTrustColor(score: number): string {
  if (score < 0.1) return '#EF4444'; // Red - Isolated
  if (score < 0.4) return '#F59E0B'; // Amber - Known
  if (score < 0.7) return '#10B981'; // Green - Partner
  return '#3B82F6'; // Blue - Federated
}

/**
 * Get label for trust score
 */
function getTrustLabel(score: number, trustClass?: string): string {
  if (trustClass) return trustClass;
  
  if (score < 0.1) return 'Isolated';
  if (score < 0.4) return 'Known';
  if (score < 0.7) return 'Partner';
  return 'Federated';
}

/**
 * Trust Badge Component
 * 
 * Shows a color-coded badge with trust score and classification.
 * 
 * @example
 * ```tsx
 * <TrustBadge trustScore={0.85} trustClass="Federated" />
 * <TrustBadge trustScore={0.45} size="small" />
 * ```
 */
export function TrustBadge({ trustScore, trustClass, size = 'medium' }: TrustBadgeProps) {
  const color = getTrustColor(trustScore);
  const label = getTrustLabel(trustScore, trustClass);

  const sizeStyles = {
    small: {
      container: styles.containerSmall,
      score: styles.scoreSmall,
      label: styles.labelSmall,
    },
    medium: {
      container: styles.containerMedium,
      score: styles.scoreMedium,
      label: styles.labelMedium,
    },
    large: {
      container: styles.containerLarge,
      score: styles.scoreLarge,
      label: styles.labelLarge,
    },
  };

  const currentSize = sizeStyles[size];

  return (
    <View style={[styles.container, currentSize.container, { backgroundColor: `${color}15` }]}>
      <View style={[styles.scoreBadge, { backgroundColor: color }]}>
        <Text style={[styles.score, currentSize.score]}>
          {trustScore.toFixed(2)}
        </Text>
      </View>
      <Text style={[styles.label, currentSize.label, { color }]}>
        {label}
      </Text>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flexDirection: 'row',
    alignItems: 'center',
    borderRadius: 8,
    paddingVertical: 4,
    paddingHorizontal: 8,
  },
  containerSmall: {
    paddingVertical: 2,
    paddingHorizontal: 6,
  },
  containerMedium: {
    paddingVertical: 4,
    paddingHorizontal: 8,
  },
  containerLarge: {
    paddingVertical: 6,
    paddingHorizontal: 12,
  },
  scoreBadge: {
    paddingVertical: 2,
    paddingHorizontal: 6,
    borderRadius: 4,
    marginRight: 6,
  },
  score: {
    color: '#FFFFFF',
    fontWeight: '700',
  },
  scoreSmall: {
    fontSize: 10,
  },
  scoreMedium: {
    fontSize: 12,
  },
  scoreLarge: {
    fontSize: 14,
  },
  label: {
    fontWeight: '600',
  },
  labelSmall: {
    fontSize: 10,
  },
  labelMedium: {
    fontSize: 12,
  },
  labelLarge: {
    fontSize: 14,
  },
});

/**
 * Get emoji indicator for trust level
 */
export function getTrustEmoji(score: number): string {
  if (score < 0.1) return '🔴'; // Red circle - Isolated
  if (score < 0.4) return '🟡'; // Yellow circle - Known
  if (score < 0.7) return '🟢'; // Green circle - Partner
  return '🔵'; // Blue circle - Federated
}

/**
 * Simple inline trust indicator (just emoji + score)
 */
export function TrustIndicator({ score }: { score: number }) {
  return (
    <Text style={{ fontSize: 12 }}>
      {getTrustEmoji(score)} {score.toFixed(2)}
    </Text>
  );
}
