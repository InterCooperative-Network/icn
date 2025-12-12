/**
 * Transactions Screen
 *
 * Full transaction history with filtering and search.
 */

import React, { useState, useCallback } from 'react';
import {
  View,
  Text,
  StyleSheet,
  FlatList,
  TouchableOpacity,
  ActivityIndicator,
  RefreshControl,
  TextInput,
} from 'react-native';
import { useTransactions, Transaction } from '@icn/react-native';
import { ICNMobileClient } from '@icn/react-native';

interface TransactionsScreenProps {
  client: ICNMobileClient;
  coopId: string;
  userDid: string;
}

type FilterType = 'all' | 'sent' | 'received';

export function TransactionsScreen({ client, coopId, userDid }: TransactionsScreenProps) {
  const [filter, setFilter] = useState<FilterType>('all');
  const [searchQuery, setSearchQuery] = useState('');

  const { transactions, isLoading, error, refresh, loadMore, hasMore } = useTransactions(
    client,
    coopId,
    50
  );

  const filteredTransactions = transactions.filter((tx) => {
    // Apply type filter
    if (filter === 'sent' && tx.from !== userDid) return false;
    if (filter === 'received' && tx.to !== userDid) return false;

    // Apply search filter
    if (searchQuery) {
      const query = searchQuery.toLowerCase();
      const matchesMemo = tx.memo?.toLowerCase().includes(query);
      const matchesFrom = tx.from.toLowerCase().includes(query);
      const matchesTo = tx.to.toLowerCase().includes(query);
      if (!matchesMemo && !matchesFrom && !matchesTo) return false;
    }

    return true;
  });

  const formatDate = (timestamp: number) => {
    const date = new Date(timestamp);
    return date.toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
      year: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  };

  const formatDid = (did: string) => {
    if (did.length <= 20) return did;
    return `${did.slice(0, 12)}...${did.slice(-6)}`;
  };

  const renderTransaction = useCallback(
    ({ item: tx }: { item: Transaction }) => {
      const isSent = tx.from === userDid;
      const otherParty = isSent ? tx.to : tx.from;

      return (
        <View style={styles.transactionCard}>
          <View style={styles.transactionHeader}>
            <View style={[styles.typeIndicator, isSent ? styles.sentIndicator : styles.receivedIndicator]}>
              <Text style={styles.typeText}>{isSent ? 'SENT' : 'RECEIVED'}</Text>
            </View>
            <Text style={styles.transactionDate}>{formatDate(tx.timestamp)}</Text>
          </View>

          <View style={styles.transactionBody}>
            <View style={styles.amountContainer}>
              <Text style={[styles.amount, isSent ? styles.sentAmount : styles.receivedAmount]}>
                {isSent ? '-' : '+'}{tx.amount}
              </Text>
              <Text style={styles.currency}>{tx.currency || 'hours'}</Text>
            </View>

            <View style={styles.partyInfo}>
              <Text style={styles.partyLabel}>{isSent ? 'To:' : 'From:'}</Text>
              <Text style={styles.partyDid}>{formatDid(otherParty)}</Text>
            </View>

            {tx.memo && (
              <View style={styles.memoContainer}>
                <Text style={styles.memoLabel}>Memo:</Text>
                <Text style={styles.memoText}>{tx.memo}</Text>
              </View>
            )}
          </View>

          <View style={styles.transactionFooter}>
            <Text style={styles.hashText}>#{tx.id.slice(0, 8)}...</Text>
            <Text style={[styles.statusBadge, styles.confirmed]}>
              confirmed
            </Text>
          </View>
        </View>
      );
    },
    [userDid]
  );

  const renderHeader = () => (
    <View style={styles.headerContainer}>
      {/* Search Bar */}
      <View style={styles.searchContainer}>
        <TextInput
          style={styles.searchInput}
          placeholder="Search transactions..."
          placeholderTextColor="#999"
          value={searchQuery}
          onChangeText={setSearchQuery}
        />
      </View>

      {/* Filter Buttons */}
      <View style={styles.filterContainer}>
        {(['all', 'sent', 'received'] as FilterType[]).map((type) => (
          <TouchableOpacity
            key={type}
            style={[styles.filterButton, filter === type && styles.filterButtonActive]}
            onPress={() => setFilter(type)}
          >
            <Text style={[styles.filterText, filter === type && styles.filterTextActive]}>
              {type.charAt(0).toUpperCase() + type.slice(1)}
            </Text>
          </TouchableOpacity>
        ))}
      </View>

      {/* Summary */}
      <View style={styles.summaryContainer}>
        <Text style={styles.summaryText}>
          {filteredTransactions.length} transaction{filteredTransactions.length !== 1 ? 's' : ''}
        </Text>
      </View>
    </View>
  );

  const renderFooter = () => {
    if (!hasMore) return null;
    return (
      <TouchableOpacity style={styles.loadMoreButton} onPress={loadMore}>
        <Text style={styles.loadMoreText}>Load More</Text>
      </TouchableOpacity>
    );
  };

  const renderEmpty = () => {
    if (isLoading) return null;
    return (
      <View style={styles.emptyContainer}>
        <Text style={styles.emptyTitle}>No Transactions</Text>
        <Text style={styles.emptyText}>
          {filter !== 'all'
            ? `No ${filter} transactions found.`
            : searchQuery
            ? 'No transactions match your search.'
            : 'Your transaction history will appear here.'}
        </Text>
      </View>
    );
  };

  if (error) {
    return (
      <View style={styles.errorContainer}>
        <Text style={styles.errorText}>Failed to load transactions</Text>
        <Text style={styles.errorDetail}>{error}</Text>
        <TouchableOpacity style={styles.retryButton} onPress={refresh}>
          <Text style={styles.retryText}>Retry</Text>
        </TouchableOpacity>
      </View>
    );
  }

  return (
    <View style={styles.container}>
      <FlatList
        data={filteredTransactions}
        keyExtractor={(item) => item.id}
        renderItem={renderTransaction}
        ListHeaderComponent={renderHeader}
        ListFooterComponent={renderFooter}
        ListEmptyComponent={renderEmpty}
        refreshControl={
          <RefreshControl refreshing={isLoading && transactions.length === 0} onRefresh={refresh} />
        }
        contentContainerStyle={filteredTransactions.length === 0 ? styles.emptyList : undefined}
        showsVerticalScrollIndicator={false}
      />

      {isLoading && transactions.length > 0 && (
        <View style={styles.loadingOverlay}>
          <ActivityIndicator color="#4A90A4" />
        </View>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#f5f5f5',
  },
  headerContainer: {
    padding: 16,
    backgroundColor: '#fff',
    borderBottomWidth: 1,
    borderBottomColor: '#e0e0e0',
  },
  searchContainer: {
    marginBottom: 12,
  },
  searchInput: {
    backgroundColor: '#f0f0f0',
    borderRadius: 8,
    paddingHorizontal: 16,
    paddingVertical: 12,
    fontSize: 16,
    color: '#333',
  },
  filterContainer: {
    flexDirection: 'row',
    gap: 8,
    marginBottom: 12,
  },
  filterButton: {
    flex: 1,
    paddingVertical: 10,
    paddingHorizontal: 16,
    borderRadius: 8,
    backgroundColor: '#f0f0f0',
    alignItems: 'center',
  },
  filterButtonActive: {
    backgroundColor: '#4A90A4',
  },
  filterText: {
    fontSize: 14,
    fontWeight: '600',
    color: '#666',
  },
  filterTextActive: {
    color: '#fff',
  },
  summaryContainer: {
    alignItems: 'center',
  },
  summaryText: {
    fontSize: 14,
    color: '#999',
  },
  transactionCard: {
    backgroundColor: '#fff',
    marginHorizontal: 16,
    marginVertical: 6,
    borderRadius: 12,
    padding: 16,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 1 },
    shadowOpacity: 0.1,
    shadowRadius: 2,
    elevation: 2,
  },
  transactionHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 12,
  },
  typeIndicator: {
    paddingHorizontal: 8,
    paddingVertical: 4,
    borderRadius: 4,
  },
  sentIndicator: {
    backgroundColor: '#FFE5E5',
  },
  receivedIndicator: {
    backgroundColor: '#E5FFE5',
  },
  typeText: {
    fontSize: 10,
    fontWeight: '700',
    color: '#666',
  },
  transactionDate: {
    fontSize: 12,
    color: '#999',
  },
  transactionBody: {
    marginBottom: 12,
  },
  amountContainer: {
    flexDirection: 'row',
    alignItems: 'baseline',
    marginBottom: 8,
  },
  amount: {
    fontSize: 28,
    fontWeight: '700',
  },
  sentAmount: {
    color: '#E53935',
  },
  receivedAmount: {
    color: '#43A047',
  },
  currency: {
    fontSize: 14,
    color: '#666',
    marginLeft: 6,
  },
  partyInfo: {
    flexDirection: 'row',
    alignItems: 'center',
    marginBottom: 4,
  },
  partyLabel: {
    fontSize: 12,
    color: '#999',
    marginRight: 6,
  },
  partyDid: {
    fontSize: 12,
    color: '#666',
    fontFamily: 'monospace',
  },
  memoContainer: {
    marginTop: 8,
    padding: 8,
    backgroundColor: '#f9f9f9',
    borderRadius: 6,
  },
  memoLabel: {
    fontSize: 10,
    color: '#999',
    marginBottom: 2,
  },
  memoText: {
    fontSize: 14,
    color: '#333',
  },
  transactionFooter: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    paddingTop: 12,
    borderTopWidth: 1,
    borderTopColor: '#f0f0f0',
  },
  hashText: {
    fontSize: 10,
    color: '#999',
    fontFamily: 'monospace',
  },
  statusBadge: {
    fontSize: 10,
    fontWeight: '600',
    paddingHorizontal: 8,
    paddingVertical: 2,
    borderRadius: 4,
    overflow: 'hidden',
  },
  confirmed: {
    backgroundColor: '#E8F5E9',
    color: '#2E7D32',
  },
  pending: {
    backgroundColor: '#FFF3E0',
    color: '#EF6C00',
  },
  loadMoreButton: {
    margin: 16,
    padding: 16,
    backgroundColor: '#4A90A4',
    borderRadius: 8,
    alignItems: 'center',
  },
  loadMoreText: {
    color: '#fff',
    fontWeight: '600',
    fontSize: 16,
  },
  emptyList: {
    flex: 1,
  },
  emptyContainer: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    padding: 32,
  },
  emptyTitle: {
    fontSize: 18,
    fontWeight: '600',
    color: '#333',
    marginBottom: 8,
  },
  emptyText: {
    fontSize: 14,
    color: '#999',
    textAlign: 'center',
  },
  errorContainer: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    padding: 32,
  },
  errorText: {
    fontSize: 18,
    fontWeight: '600',
    color: '#E53935',
    marginBottom: 8,
  },
  errorDetail: {
    fontSize: 14,
    color: '#999',
    textAlign: 'center',
    marginBottom: 16,
  },
  retryButton: {
    paddingHorizontal: 24,
    paddingVertical: 12,
    backgroundColor: '#4A90A4',
    borderRadius: 8,
  },
  retryText: {
    color: '#fff',
    fontWeight: '600',
  },
  loadingOverlay: {
    position: 'absolute',
    bottom: 16,
    left: 0,
    right: 0,
    alignItems: 'center',
  },
});

export default TransactionsScreen;
