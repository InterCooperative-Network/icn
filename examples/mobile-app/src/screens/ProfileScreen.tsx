import React, { useContext } from 'react';
import { View, StyleSheet, ScrollView } from 'react-native';
import { Card, Title, List, Button, Divider, Text } from 'react-native-paper';
import { useNavigation } from '@react-navigation/native';
import { AuthContext } from '../contexts/AuthContext';

export default function ProfileScreen() {
  const { logout } = useContext(AuthContext);
  const navigation = useNavigation<any>();

  return (
    <ScrollView style={styles.container}>
      {/* Web Login Card - Prominent placement */}
      <Card style={[styles.card, styles.webLoginCard]}>
        <Card.Content style={styles.webLoginContent}>
          <View style={styles.webLoginHeader}>
            <Text style={styles.webLoginIcon}>📱</Text>
            <View style={styles.webLoginText}>
              <Title style={styles.webLoginTitle}>Approve Web Login</Title>
              <Text style={styles.webLoginDescription}>
                Scan QR code to log in to the web app
              </Text>
            </View>
          </View>
          <Button
            mode="contained"
            onPress={() => navigation.navigate('QRScanner')}
            icon="qrcode-scan"
            style={styles.scanButton}
          >
            Scan QR Code
          </Button>
        </Card.Content>
      </Card>

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
  // Web login card styles
  webLoginCard: {
    backgroundColor: '#2563eb',
    marginTop: 16,
  },
  webLoginContent: {
    paddingVertical: 8,
  },
  webLoginHeader: {
    flexDirection: 'row',
    alignItems: 'center',
    marginBottom: 16,
  },
  webLoginIcon: {
    fontSize: 40,
    marginRight: 16,
  },
  webLoginText: {
    flex: 1,
  },
  webLoginTitle: {
    color: '#fff',
    fontSize: 18,
    marginBottom: 4,
  },
  webLoginDescription: {
    color: 'rgba(255, 255, 255, 0.8)',
    fontSize: 14,
  },
  scanButton: {
    backgroundColor: '#fff',
  },
  logoutButton: {
    margin: 16,
  },
});
