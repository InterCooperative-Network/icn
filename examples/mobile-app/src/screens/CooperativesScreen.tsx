import React from 'react';
import { View, StyleSheet, ScrollView } from 'react-native';
import { Text, Card } from 'react-native-paper';
// CooperativeManager disabled - requires @icn/sdk which is not yet implemented
// import CooperativeManager from '../../CooperativeManager';

export default function CooperativesScreen() {
  return (
    <View style={styles.container}>
      <ScrollView contentContainerStyle={styles.content}>
        <Card style={styles.card}>
          <Card.Content>
            <Text variant="titleLarge">Cooperatives</Text>
            <Text variant="bodyMedium" style={styles.placeholder}>
              Cooperative management features coming soon.
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
