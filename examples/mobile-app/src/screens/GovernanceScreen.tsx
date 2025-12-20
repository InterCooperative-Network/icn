import React from 'react';
import { View, StyleSheet, ScrollView } from 'react-native';
import { Text, Card } from 'react-native-paper';
// VotingScreen disabled - requires @icn/react-native which is not yet implemented
// import VotingScreen from '../../VotingScreen';

export default function GovernanceScreen() {
  return (
    <View style={styles.container}>
      <ScrollView contentContainerStyle={styles.content}>
        <Card style={styles.card}>
          <Card.Content>
            <Text variant="titleLarge">Governance</Text>
            <Text variant="bodyMedium" style={styles.placeholder}>
              Voting and governance features coming soon.
            </Text>
          </Card.Content>
        </Card>
      </ScrollView>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#f1f5f9',
  },
  content: {
    padding: 16,
  },
  card: {
    marginBottom: 16,
  },
  placeholder: {
    marginTop: 8,
    color: '#666',
  },
});
