import React, { useContext } from 'react';
import { View, StyleSheet, ScrollView } from 'react-native';
import { Card, Title, List, Button, Divider } from 'react-native-paper';
import { AuthContext } from '../contexts/AuthContext';

export default function ProfileScreen() {
  const { logout } = useContext(AuthContext);

  return (
    <ScrollView style={styles.container}>
      <Card style={styles.card}>
        <Card.Content>
          <Title>Profile</Title>
          <List.Item
            title="DID"
            description="Your decentralized identifier"
            left={props => <List.Icon {...props} icon="account" />}
          />
          <Divider />
          <List.Item
            title="Settings"
            description="App preferences"
            left={props => <List.Icon {...props} icon="cog" />}
            onPress={() => {}}
          />
          <Divider />
          <List.Item
            title="Security"
            description="Keys and authentication"
            left={props => <List.Icon {...props} icon="shield" />}
            onPress={() => {}}
          />
        </Card.Content>
      </Card>

      <Card style={styles.card}>
        <Card.Content>
          <Title>Budgets & Payments</Title>
          <List.Item
            title="Budgets"
            description="Manage spending budgets"
            left={props => <List.Icon {...props} icon="cash-multiple" />}
            onPress={() => {}}
          />
          <Divider />
          <List.Item
            title="Recurring Payments"
            description="Automated payments"
            left={props => <List.Icon {...props} icon="calendar-sync" />}
            onPress={() => {}}
          />
        </Card.Content>
      </Card>

      <Button
        mode="contained"
        onPress={logout}
        style={styles.logoutButton}
        buttonColor="#ef4444"
      >
        Sign Out
      </Button>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#f1f5f9',
  },
  card: {
    margin: 16,
    elevation: 2,
  },
  logoutButton: {
    margin: 16,
  },
});
