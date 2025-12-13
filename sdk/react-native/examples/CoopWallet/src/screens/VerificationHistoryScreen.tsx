/**
 * Verification History Screen
 *
 * Displays a list of past identity verifications.
 */

import React from 'react';
import {
  View,
  Text,
  StyleSheet,
  FlatList,
  TouchableOpacity,
  Alert,
} from 'react-native';
import { NativeStackScreenProps } from '@react-navigation/native-stack';
import { HistoryEntry } from '@icn/react-native';
import { RootStackParamList } from '../../App';
import { useTheme, Theme } from '../contexts/ThemeContext';

type Props = NativeStackScreenProps<RootStackParamList, 'VerificationHistory'>;

export function VerificationHistoryScreen({ navigation }: Props) {
  const { theme } = useTheme();
  const styles = createStyles(theme);
  // Note: In a real implementation, history would come from a global state manager
  // or persisted storage. For now, we'll show an empty state.
  const history: HistoryEntry[] = [];
  const onClear = () => {};

  const handleClear = () => {
    Alert.alert(
      'Clear History',
      'Are you sure you want to clear all verification history?',
      [
        { text: 'Cancel', style: 'cancel' },
        {
          text: 'Clear',
          style: 'destructive',
          onPress: () => {
            onClear();
            navigation.goBack();
          },
        },
      ],
    );
  };

  const formatTime = (timestamp: number) => {
    const date = new Date(timestamp);
    const now = new Date();
    const isToday = date.toDateString() === now.toDateString();

    if (isToday) {
      return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    }

    const yesterday = new Date(now);
    yesterday.setDate(yesterday.getDate() - 1);
    if (date.toDateString() === yesterday.toDateString()) {
      return `Yesterday ${date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}`;
    }

    return date.toLocaleDateString([], {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  };

  const renderItem = ({ item }: { item: HistoryEntry }) => (
    <View style={styles.historyItem}>
      <View style={styles.historyHeader}>
        <View
          style={[
            styles.statusIndicator,
            { backgroundColor: item.result.valid ? '#4CAF50' : '#F44336' },
          ]}
        />
        <View style={styles.historyInfo}>
          <Text style={styles.proofType}>{item.proofTypeLabel}</Text>
          <Text style={styles.timestamp}>{formatTime(item.timestamp)}</Text>
        </View>
        <View style={styles.levelBadge}>
          <Text style={styles.levelBadgeText}>L{item.level}</Text>
        </View>
      </View>

      {item.result.warnings.length > 0 && (
        <View style={styles.warningsContainer}>
          {item.result.warnings.map((warning, i) => (
            <Text key={i} style={styles.warningText}>
              {warning}
            </Text>
          ))}
        </View>
      )}
    </View>
  );

  const renderEmptyState = () => (
    <View style={styles.emptyState}>
      <Text style={styles.emptyIcon}>&#128270;</Text>
      <Text style={styles.emptyTitle}>No Verifications Yet</Text>
      <Text style={styles.emptyDescription}>
        Scan SDIS identity QR codes to verify them. Your verification history will appear here.
      </Text>
      <TouchableOpacity style={styles.primaryButton} onPress={() => navigation.goBack()}>
        <Text style={styles.primaryButtonText}>Start Scanning</Text>
      </TouchableOpacity>
    </View>
  );

  const renderHeader = () => (
    <View style={styles.statsContainer}>
      <View style={styles.statItem}>
        <Text style={styles.statValue}>{history.length}</Text>
        <Text style={styles.statLabel}>Total</Text>
      </View>
      <View style={styles.statDivider} />
      <View style={styles.statItem}>
        <Text style={[styles.statValue, { color: '#4CAF50' }]}>
          {history.filter((h) => h.result.valid).length}
        </Text>
        <Text style={styles.statLabel}>Valid</Text>
      </View>
      <View style={styles.statDivider} />
      <View style={styles.statItem}>
        <Text style={[styles.statValue, { color: '#F44336' }]}>
          {history.filter((h) => !h.result.valid).length}
        </Text>
        <Text style={styles.statLabel}>Invalid</Text>
      </View>
    </View>
  );

  return (
    <View style={styles.container}>
      {history.length === 0 ? (
        renderEmptyState()
      ) : (
        <>
          <FlatList
            data={history}
            renderItem={renderItem}
            keyExtractor={(item) => item.id}
            ListHeaderComponent={renderHeader}
            contentContainerStyle={styles.listContent}
            ItemSeparatorComponent={() => <View style={styles.separator} />}
          />
          <View style={styles.footer}>
            <TouchableOpacity style={styles.clearButton} onPress={handleClear}>
              <Text style={styles.clearButtonText}>Clear History</Text>
            </TouchableOpacity>
          </View>
        </>
      )}
    </View>
  );
}

const createStyles = (theme: Theme) => StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: theme.colors.background,
  },
  listContent: {
    padding: 16,
  },
  statsContainer: {
    flexDirection: 'row',
    backgroundColor: theme.colors.card,
    borderRadius: 12,
    padding: 16,
    marginBottom: 16,
    justifyContent: 'space-around',
    alignItems: 'center',
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 1 },
    shadowOpacity: 0.1,
    shadowRadius: 2,
    elevation: 2,
  },
  statItem: {
    alignItems: 'center',
  },
  statValue: {
    fontSize: 24,
    fontWeight: 'bold',
    color: theme.colors.text,
  },
  statLabel: {
    fontSize: 12,
    color: theme.colors.textSecondary,
    marginTop: 4,
  },
  statDivider: {
    width: 1,
    height: 40,
    backgroundColor: theme.colors.borderLight,
  },
  historyItem: {
    backgroundColor: theme.colors.card,
    borderRadius: 12,
    padding: 16,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 1 },
    shadowOpacity: 0.1,
    shadowRadius: 2,
    elevation: 2,
  },
  historyHeader: {
    flexDirection: 'row',
    alignItems: 'center',
  },
  statusIndicator: {
    width: 12,
    height: 12,
    borderRadius: 6,
    marginRight: 12,
  },
  historyInfo: {
    flex: 1,
  },
  proofType: {
    fontSize: 16,
    fontWeight: '600',
    color: theme.colors.text,
  },
  timestamp: {
    fontSize: 12,
    color: theme.colors.textMuted,
    marginTop: 2,
  },
  levelBadge: {
    backgroundColor: theme.mode === 'dark' ? 'rgba(74, 144, 164, 0.2)' : '#E3F2FD',
    paddingHorizontal: 8,
    paddingVertical: 4,
    borderRadius: 12,
  },
  levelBadgeText: {
    fontSize: 12,
    color: theme.colors.primary,
    fontWeight: '600',
  },
  warningsContainer: {
    marginTop: 12,
    padding: 8,
    backgroundColor: theme.mode === 'dark' ? 'rgba(255, 152, 0, 0.2)' : '#FFF3E0',
    borderRadius: 8,
  },
  warningText: {
    fontSize: 12,
    color: theme.colors.warning,
  },
  separator: {
    height: 12,
  },
  emptyState: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    padding: 32,
  },
  emptyIcon: {
    fontSize: 64,
    marginBottom: 16,
  },
  emptyTitle: {
    fontSize: 20,
    fontWeight: '600',
    color: theme.colors.text,
    marginBottom: 8,
  },
  emptyDescription: {
    fontSize: 14,
    color: theme.colors.textSecondary,
    textAlign: 'center',
    marginBottom: 24,
    lineHeight: 20,
  },
  primaryButton: {
    backgroundColor: theme.colors.primary,
    paddingVertical: 12,
    paddingHorizontal: 24,
    borderRadius: 8,
  },
  primaryButtonText: {
    color: 'white',
    fontSize: 16,
    fontWeight: '600',
  },
  footer: {
    padding: 16,
    backgroundColor: theme.colors.card,
    borderTopWidth: 1,
    borderTopColor: theme.colors.borderLight,
  },
  clearButton: {
    backgroundColor: theme.mode === 'dark' ? 'rgba(198, 40, 40, 0.2)' : '#ffebee',
    padding: 12,
    borderRadius: 8,
    alignItems: 'center',
  },
  clearButtonText: {
    color: theme.colors.error,
    fontSize: 14,
    fontWeight: '500',
  },
});
