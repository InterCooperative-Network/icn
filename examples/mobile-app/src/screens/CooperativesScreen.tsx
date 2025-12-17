import React from 'react';
import { View, StyleSheet } from 'react-native';
import CooperativeManager from '../../CooperativeManager';

export default function CooperativesScreen() {
  return (
    <View style={styles.container}>
      <CooperativeManager />
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#f1f5f9',
  },
});
