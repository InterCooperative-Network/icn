/**
 * Skeleton Loading Component
 *
 * Animated placeholder for loading states.
 */

import React, { useEffect, useRef } from 'react';
import { View, StyleSheet, Animated, ViewStyle } from 'react-native';

interface SkeletonProps {
  width?: number | `${number}%`;
  height?: number;
  borderRadius?: number;
  style?: ViewStyle;
}

export function Skeleton({ width = '100%', height = 20, borderRadius = 4, style }: SkeletonProps) {
  const opacity = useRef(new Animated.Value(0.3)).current;

  useEffect(() => {
    const animation = Animated.loop(
      Animated.sequence([
        Animated.timing(opacity, {
          toValue: 0.7,
          duration: 800,
          useNativeDriver: true,
        }),
        Animated.timing(opacity, {
          toValue: 0.3,
          duration: 800,
          useNativeDriver: true,
        }),
      ])
    );
    animation.start();
    return () => animation.stop();
  }, [opacity]);

  return (
    <Animated.View
      style={[
        styles.skeleton,
        {
          width,
          height,
          borderRadius,
          opacity,
        },
        style,
      ]}
    />
  );
}

// Pre-built skeleton layouts
export function SkeletonTransaction() {
  return (
    <View style={styles.transactionSkeleton}>
      <Skeleton width={40} height={40} borderRadius={20} />
      <View style={styles.transactionContent}>
        <Skeleton width="60%" height={16} style={styles.mb8} />
        <Skeleton width="40%" height={12} />
      </View>
      <View style={styles.transactionAmount}>
        <Skeleton width={60} height={20} style={styles.mb4} />
        <Skeleton width={40} height={12} />
      </View>
    </View>
  );
}

export function SkeletonCard() {
  return (
    <View style={styles.cardSkeleton}>
      <Skeleton width="40%" height={14} style={styles.mb12} />
      <Skeleton width="70%" height={32} style={styles.mb8} />
      <Skeleton width="50%" height={14} />
    </View>
  );
}

export function SkeletonList({ count = 3 }: { count?: number }) {
  return (
    <View>
      {Array.from({ length: count }).map((_, i) => (
        <SkeletonTransaction key={i} />
      ))}
    </View>
  );
}

export function SkeletonBalance() {
  return (
    <View style={styles.balanceSkeleton}>
      <Skeleton width={100} height={14} style={styles.mb8} />
      <Skeleton width={150} height={48} style={styles.mb8} />
      <Skeleton width={80} height={14} />
    </View>
  );
}

const styles = StyleSheet.create({
  skeleton: {
    backgroundColor: '#e0e0e0',
  },
  mb4: {
    marginBottom: 4,
  },
  mb8: {
    marginBottom: 8,
  },
  mb12: {
    marginBottom: 12,
  },
  transactionSkeleton: {
    flexDirection: 'row',
    alignItems: 'center',
    padding: 16,
    backgroundColor: '#fff',
    marginBottom: 8,
    borderRadius: 8,
  },
  transactionContent: {
    flex: 1,
    marginLeft: 12,
  },
  transactionAmount: {
    alignItems: 'flex-end',
  },
  cardSkeleton: {
    backgroundColor: '#fff',
    padding: 20,
    borderRadius: 12,
    marginBottom: 16,
  },
  balanceSkeleton: {
    alignItems: 'center',
    padding: 24,
    backgroundColor: '#fff',
    borderRadius: 16,
    marginBottom: 16,
  },
});

export default Skeleton;
