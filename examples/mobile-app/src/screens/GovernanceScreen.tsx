import React from 'react';
import { View, StyleSheet, ScrollView } from 'react-native';
import { Text } from 'react-native-paper';
import VotingScreen from '../../VotingScreen';

export default function GovernanceScreen() {
  return (
    <View style={styles.container}>
      <VotingScreen />
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#f1f5f9',
  },
});
