import React, { useState, useEffect, useContext } from 'react';
import { View, StyleSheet, ScrollView, RefreshControl } from 'react-native';
import { Card, Title, Text, Button } from 'react-native-paper';
import { AuthContext } from '../contexts/AuthContext';

export default function HomeScreen() {
  const { client } = useContext(AuthContext);
  const [balance, setBalance] = useState<any>(null);
  const [stats, setStats] = useState<any>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    loadData();
  }, []);

  const loadData = async () => {
    if (!client) return;
    
    setLoading(true);
    try {
      const balanceData = await client.getBalance();
      setBalance(balanceData);
      
      // Load additional stats
      const [coops, notifications] = await Promise.all([
        client.listCooperatives(),
        client.getNotifications(),
      ]);
      
      setStats({
        coopCount: coops.length,
        unreadNotifications: notifications.filter((n: any) => !n.read).length,
      });
    } catch (error) {
      console.error('Failed to load data:', error);
    } finally {
      setLoading(false);
    }
  };

  return (
    <ScrollView
      style={styles.container}
      refreshControl={
        <RefreshControl refreshing={loading} onRefresh={loadData} />
      }
    >
      <View style={styles.header}>
        <Title>Welcome to ICN</Title>
        <Text>Cooperative Economy</Text>
      </View>

      <Card style={styles.card}>
        <Card.Content>
          <Title>Your Balance</Title>
          <Text style={styles.balanceAmount}>
            {balance ? balance.balance : '--'} credits
          </Text>
          <Text style={styles.creditLimit}>
            Credit limit: {balance ? balance.credit_limit : '--'}
          </Text>
        </Card.Content>
      </Card>

      <Card style={styles.card}>
        <Card.Content>
          <Title>Quick Stats</Title>
          <View style={styles.statsRow}>
            <View style={styles.stat}>
              <Text style={styles.statValue}>{stats?.coopCount || 0}</Text>
              <Text style={styles.statLabel}>Cooperatives</Text>
            </View>
            <View style={styles.stat}>
              <Text style={styles.statValue}>{stats?.unreadNotifications || 0}</Text>
              <Text style={styles.statLabel}>Notifications</Text>
            </View>
          </View>
        </Card.Content>
      </Card>

      <Card style={styles.card}>
        <Card.Content>
          <Title>Quick Actions</Title>
          <Button mode="contained" style={styles.actionButton}>
            Send Payment
          </Button>
          <Button mode="outlined" style={styles.actionButton}>
            View History
          </Button>
        </Card.Content>
      </Card>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#f1f5f9',
  },
  header: {
    padding: 20,
    backgroundColor: '#2563eb',
  },
  card: {
    margin: 16,
    elevation: 2,
  },
  balanceAmount: {
    fontSize: 32,
    fontWeight: 'bold',
    color: '#2563eb',
    marginVertical: 8,
  },
  creditLimit: {
    color: '#64748b',
  },
  statsRow: {
    flexDirection: 'row',
    justifyContent: 'space-around',
    marginTop: 12,
  },
  stat: {
    alignItems: 'center',
  },
  statValue: {
    fontSize: 24,
    fontWeight: 'bold',
    color: '#1e293b',
  },
  statLabel: {
    fontSize: 12,
    color: '#64748b',
    marginTop: 4,
  },
  actionButton: {
    marginTop: 12,
  },
});
