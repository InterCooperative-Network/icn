/**
 * Vouch History Screen
 *
 * Shows the steward's vouch history.
 */

import React from 'react';
import {
  View,
  Text,
  StyleSheet,
  FlatList,
  RefreshControl,
  ActivityIndicator,
} from 'react-native';
import { useVouchHistory, VouchRecord } from '@icn/react-native';
import { client } from '../client';
import { useTheme, Theme } from '../contexts/ThemeContext';

export function VouchHistoryScreen() {
  const { theme } = useTheme();
  const styles = createStyles(theme);
  const { history, total, isLoading, error, refresh } = useVouchHistory(client!, 100);

  const renderVouchItem = ({ item }: { item: VouchRecord }) => (
    <View style={styles.vouchCard}>
      <View style={styles.cardHeader}>
        <Text style={styles.identityName}>{item.identity_name}</Text>
        <View style={styles.vouchedBadge}>
          <Text style={styles.vouchedText}>Vouched</Text>
        </View>
      </View>
      <View style={styles.cardMeta}>
        <Text style={styles.metaText}>Coop: {item.coop_id}</Text>
        <Text style={styles.metaText}>Date: {formatDate(item.vouched_at)}</Text>
      </View>
      {item.vouch_statement && (
        <View style={styles.statementContainer}>
          <Text style={styles.statementLabel}>Statement:</Text>
          <Text style={styles.statementText}>{item.vouch_statement}</Text>
        </View>
      )}
    </View>
  );

  return (
    <View style={styles.container}>
      {/* Header with count */}
      <View style={styles.header}>
        <Text style={styles.headerText}>
          {total} {total === 1 ? 'vouch' : 'vouches'} total
        </Text>
      </View>

      <FlatList
        data={history}
        renderItem={renderVouchItem}
        keyExtractor={(item) => item.enrollment_id + item.vouched_at}
        refreshControl={
          <RefreshControl refreshing={isLoading} onRefresh={refresh} />
        }
        contentContainerStyle={styles.listContent}
        ListEmptyComponent={
          isLoading ? (
            <View style={styles.loadingContainer}>
              <ActivityIndicator size="large" color={theme.colors.primary} />
              <Text style={styles.loadingText}>Loading history...</Text>
            </View>
          ) : error ? (
            <View style={styles.errorContainer}>
              <Text style={styles.errorText}>{error}</Text>
            </View>
          ) : (
            <View style={styles.emptyContainer}>
              <Text style={styles.emptyIcon}>📋</Text>
              <Text style={styles.emptyTitle}>No History Yet</Text>
              <Text style={styles.emptyText}>
                Your vouch history will appear here after you vouch for enrollments.
              </Text>
            </View>
          )
        }
      />
    </View>
  );
}

function formatDate(dateString: string): string {
  return new Date(dateString).toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

const createStyles = (theme: Theme) => StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: theme.colors.background,
  },
  header: {
    backgroundColor: theme.colors.card,
    padding: 16,
    borderBottomWidth: 1,
    borderBottomColor: theme.colors.border,
  },
  headerText: {
    fontSize: 14,
    color: theme.colors.textSecondary,
  },
  listContent: {
    padding: 16,
  },
  vouchCard: {
    backgroundColor: theme.colors.card,
    borderRadius: 12,
    padding: 16,
    marginBottom: 12,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 1 },
    shadowOpacity: theme.mode === 'dark' ? 0.3 : 0.05,
    shadowRadius: 4,
    elevation: 2,
  },
  cardHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 8,
  },
  identityName: {
    fontSize: 16,
    fontWeight: '600',
    color: theme.colors.text,
    flex: 1,
  },
  vouchedBadge: {
    backgroundColor: theme.colors.successBackground,
    paddingHorizontal: 10,
    paddingVertical: 4,
    borderRadius: 12,
  },
  vouchedText: {
    fontSize: 12,
    fontWeight: '600',
    color: theme.colors.successDark,
  },
  cardMeta: {
    flexDirection: 'row',
    gap: 16,
    marginBottom: 8,
  },
  metaText: {
    fontSize: 13,
    color: theme.colors.textSecondary,
  },
  statementContainer: {
    backgroundColor: theme.colors.background,
    borderRadius: 8,
    padding: 12,
    marginTop: 8,
  },
  statementLabel: {
    fontSize: 12,
    color: theme.colors.textSecondary,
    marginBottom: 4,
  },
  statementText: {
    fontSize: 13,
    color: theme.colors.text,
    fontStyle: 'italic',
  },
  loadingContainer: {
    padding: 40,
    alignItems: 'center',
  },
  loadingText: {
    marginTop: 12,
    fontSize: 14,
    color: theme.colors.textSecondary,
  },
  errorContainer: {
    padding: 40,
    alignItems: 'center',
  },
  errorText: {
    fontSize: 14,
    color: theme.colors.error,
    textAlign: 'center',
  },
  emptyContainer: {
    padding: 40,
    alignItems: 'center',
  },
  emptyIcon: {
    fontSize: 48,
    marginBottom: 16,
  },
  emptyTitle: {
    fontSize: 20,
    fontWeight: '600',
    color: theme.colors.text,
    marginBottom: 8,
  },
  emptyText: {
    fontSize: 14,
    color: theme.colors.textSecondary,
    textAlign: 'center',
  },
});
