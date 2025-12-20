import React, { useState, useEffect, useContext } from 'react';
import { View, StyleSheet, FlatList } from 'react-native';
import { Card, Title, Text, FAB } from 'react-native-paper';
import { AuthContext } from '../contexts/AuthContext';

export default function LedgerScreen() {
  const { client } = useContext(AuthContext);
  const [transactions, setTransactions] = useState<any[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    loadTransactions();
  }, []);

  const loadTransactions = async () => {
    if (!client) return;
    
    try {
      const { transactions: data } = await client.getHistory(50, 0);
      setTransactions(data);
    } catch (error) {
      console.error('Failed to load transactions:', error);
    } finally {
      setLoading(false);
    }
  };

  const renderTransaction = ({ item }: any) => (
    <Card style={styles.card}>
      <Card.Content>
        <View style={styles.transactionRow}>
          <View style={styles.transactionInfo}>
            <Text style={styles.transactionAmount}>
              {item.amount > 0 ? '+' : ''}{item.amount}
            </Text>
            <Text style={styles.transactionDesc}>{item.description}</Text>
            <Text style={styles.transactionTime}>
              {new Date(item.timestamp * 1000).toLocaleDateString()}
            </Text>
          </View>
        </View>
      </Card.Content>
    </Card>
  );

  return (
    <View style={styles.container}>
      <FlatList
        data={transactions}
        renderItem={renderTransaction}
        keyExtractor={(item) => item.id}
        refreshing={loading}
        onRefresh={loadTransactions}
        ListEmptyComponent={
          <Text style={styles.emptyText}>No transactions yet</Text>
        }
      />
      <FAB
        style={styles.fab}
        icon="plus"
        label="New Payment"
        onPress={() => {/* Navigate to payment screen */}}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#f1f5f9',
  },
  card: {
    margin: 8,
    marginHorizontal: 16,
  },
  transactionRow: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  transactionInfo: {
    flex: 1,
  },
  transactionAmount: {
    fontSize: 20,
    fontWeight: 'bold',
    color: '#1e293b',
  },
  transactionDesc: {
    color: '#64748b',
    marginTop: 4,
  },
  transactionTime: {
    fontSize: 12,
    color: '#94a3b8',
    marginTop: 4,
  },
  emptyText: {
    textAlign: 'center',
    marginTop: 40,
    color: '#94a3b8',
  },
  fab: {
    position: 'absolute',
    right: 16,
    bottom: 16,
  },
});
