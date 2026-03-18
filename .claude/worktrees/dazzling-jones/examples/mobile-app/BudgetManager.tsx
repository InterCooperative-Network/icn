/**
 * Budget Manager Component
 * Create and monitor spending budgets
 */
import React, { useState } from 'react';
import {
  View,
  Text,
  TouchableOpacity,
  StyleSheet,
  ScrollView,
  Alert,
} from 'react-native';
import { useBudgets } from '@icn/react-native';

interface BudgetCardProps {
  id: string;
  account: string;
  limit: number;
  spent: number;
  currency: string;
  period: string;
  status: string;
  description: string;
  onEdit: () => void;
  onDelete: () => void;
}

function BudgetCard({
  account,
  limit,
  spent,
  currency,
  period,
  status,
  description,
  onEdit,
  onDelete,
}: BudgetCardProps) {
  const remaining = limit - spent;
  const percentage = limit > 0 ? (spent / limit) * 100 : 0;
  const isExceeded = spent >= limit;
  const isWarning = percentage >= 80 && !isExceeded;

  const getBarColor = () => {
    if (isExceeded) return '#F44336';
    if (isWarning) return '#FF9800';
    return '#4CAF50';
  };

  const getStatusColor = () => {
    if (status === 'exceeded') return '#F44336';
    if (status === 'paused') return '#9E9E9E';
    return '#4CAF50';
  };

  return (
    <View style={styles.card}>
      {/* Header */}
      <View style={styles.cardHeader}>
        <View>
          <Text style={styles.cardAccount}>{account}</Text>
          <Text style={styles.cardDescription}>{description}</Text>
        </View>
        <View style={[styles.statusBadge, { backgroundColor: getStatusColor() }]}>
          <Text style={styles.statusText}>{status}</Text>
        </View>
      </View>

      {/* Progress Bar */}
      <View style={styles.progressContainer}>
        <View style={styles.progressBar}>
          <View
            style={[
              styles.progressFill,
              { width: `${Math.min(percentage, 100)}%`, backgroundColor: getBarColor() },
            ]}
          />
        </View>
        <Text style={styles.progressText}>
          {percentage.toFixed(1)}%
        </Text>
      </View>

      {/* Amounts */}
      <View style={styles.amounts}>
        <View style={styles.amountItem}>
          <Text style={styles.amountLabel}>Spent</Text>
          <Text style={[styles.amountValue, isExceeded && styles.exceededText]}>
            ${spent.toFixed(2)}
          </Text>
        </View>
        <View style={styles.amountItem}>
          <Text style={styles.amountLabel}>Limit</Text>
          <Text style={styles.amountValue}>${limit.toFixed(2)}</Text>
        </View>
        <View style={styles.amountItem}>
          <Text style={styles.amountLabel}>Remaining</Text>
          <Text style={[styles.amountValue, remaining < 0 && styles.exceededText]}>
            ${remaining.toFixed(2)}
          </Text>
        </View>
      </View>

      {/* Period */}
      <Text style={styles.period}>Period: {period}</Text>

      {/* Alerts */}
      {isExceeded && (
        <View style={styles.alertBox}>
          <Text style={styles.alertText}>⚠️ Budget exceeded!</Text>
        </View>
      )}
      {isWarning && (
        <View style={[styles.alertBox, styles.warningBox]}>
          <Text style={styles.warningText}>⚠️ Approaching limit</Text>
        </View>
      )}

      {/* Actions */}
      <View style={styles.actions}>
        <TouchableOpacity style={styles.actionButton} onPress={onEdit}>
          <Text style={styles.actionText}>Edit</Text>
        </TouchableOpacity>
        <TouchableOpacity style={[styles.actionButton, styles.deleteButton]} onPress={onDelete}>
          <Text style={[styles.actionText, styles.deleteText]}>Delete</Text>
        </TouchableOpacity>
      </View>
    </View>
  );
}

export function BudgetManager() {
  const { budgets, loading, updateBudget, deleteBudget, refresh } = useBudgets();

  const handleEdit = (budgetId: string) => {
    // Navigate to edit screen
    Alert.alert('Edit Budget', `Edit budget ${budgetId}`);
  };

  const handleDelete = (budgetId: string, account: string) => {
    Alert.alert(
      'Delete Budget',
      `Are you sure you want to delete the budget for ${account}?`,
      [
        { text: 'Cancel', style: 'cancel' },
        {
          text: 'Delete',
          style: 'destructive',
          onPress: async () => {
            try {
              await deleteBudget(budgetId);
              Alert.alert('Success', 'Budget deleted');
            } catch (error) {
              Alert.alert('Error', 'Failed to delete budget');
            }
          },
        },
      ]
    );
  };

  const handleCreateNew = () => {
    // Navigate to create screen
    Alert.alert('Create Budget', 'Navigate to budget creation form');
  };

  // Group budgets by status
  const activeBudgets = budgets.filter((b) => b.status === 'active');
  const exceededBudgets = budgets.filter((b) => b.status === 'exceeded');
  const otherBudgets = budgets.filter(
    (b) => b.status !== 'active' && b.status !== 'exceeded'
  );

  return (
    <ScrollView style={styles.container}>
      <View style={styles.header}>
        <Text style={styles.headerTitle}>Budget Management</Text>
        <TouchableOpacity style={styles.createButton} onPress={handleCreateNew}>
          <Text style={styles.createButtonText}>+ New Budget</Text>
        </TouchableOpacity>
      </View>

      {/* Summary */}
      <View style={styles.summary}>
        <View style={styles.summaryItem}>
          <Text style={styles.summaryValue}>{activeBudgets.length}</Text>
          <Text style={styles.summaryLabel}>Active</Text>
        </View>
        <View style={styles.summaryItem}>
          <Text style={[styles.summaryValue, styles.exceededText]}>
            {exceededBudgets.length}
          </Text>
          <Text style={styles.summaryLabel}>Exceeded</Text>
        </View>
        <View style={styles.summaryItem}>
          <Text style={styles.summaryValue}>{budgets.length}</Text>
          <Text style={styles.summaryLabel}>Total</Text>
        </View>
      </View>

      {/* Exceeded Budgets (Priority) */}
      {exceededBudgets.length > 0 && (
        <View style={styles.section}>
          <Text style={styles.sectionTitle}>⚠️ Exceeded Budgets</Text>
          {exceededBudgets.map((budget) => (
            <BudgetCard
              key={budget.id}
              {...budget}
              onEdit={() => handleEdit(budget.id)}
              onDelete={() => handleDelete(budget.id, budget.account)}
            />
          ))}
        </View>
      )}

      {/* Active Budgets */}
      {activeBudgets.length > 0 && (
        <View style={styles.section}>
          <Text style={styles.sectionTitle}>Active Budgets</Text>
          {activeBudgets.map((budget) => (
            <BudgetCard
              key={budget.id}
              {...budget}
              onEdit={() => handleEdit(budget.id)}
              onDelete={() => handleDelete(budget.id, budget.account)}
            />
          ))}
        </View>
      )}

      {/* Other Budgets */}
      {otherBudgets.length > 0 && (
        <View style={styles.section}>
          <Text style={styles.sectionTitle}>Other Budgets</Text>
          {otherBudgets.map((budget) => (
            <BudgetCard
              key={budget.id}
              {...budget}
              onEdit={() => handleEdit(budget.id)}
              onDelete={() => handleDelete(budget.id, budget.account)}
            />
          ))}
        </View>
      )}

      {/* Empty State */}
      {budgets.length === 0 && !loading && (
        <View style={styles.emptyState}>
          <Text style={styles.emptyText}>No budgets yet</Text>
          <TouchableOpacity style={styles.emptyButton} onPress={handleCreateNew}>
            <Text style={styles.emptyButtonText}>Create Your First Budget</Text>
          </TouchableOpacity>
        </View>
      )}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#f5f5f5',
  },
  header: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    padding: 16,
    backgroundColor: '#fff',
    borderBottomWidth: 1,
    borderBottomColor: '#e0e0e0',
  },
  headerTitle: {
    fontSize: 24,
    fontWeight: 'bold',
    color: '#333',
  },
  createButton: {
    backgroundColor: '#007AFF',
    paddingHorizontal: 16,
    paddingVertical: 8,
    borderRadius: 8,
  },
  createButtonText: {
    color: '#fff',
    fontSize: 14,
    fontWeight: '600',
  },
  summary: {
    flexDirection: 'row',
    justifyContent: 'space-around',
    padding: 16,
    backgroundColor: '#fff',
    marginVertical: 8,
  },
  summaryItem: {
    alignItems: 'center',
  },
  summaryValue: {
    fontSize: 32,
    fontWeight: 'bold',
    color: '#007AFF',
  },
  summaryLabel: {
    fontSize: 12,
    color: '#666',
    marginTop: 4,
  },
  section: {
    marginVertical: 8,
  },
  sectionTitle: {
    fontSize: 18,
    fontWeight: '600',
    marginHorizontal: 16,
    marginBottom: 8,
    color: '#333',
  },
  card: {
    backgroundColor: '#fff',
    marginHorizontal: 16,
    marginVertical: 8,
    padding: 16,
    borderRadius: 8,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 1 },
    shadowOpacity: 0.1,
    shadowRadius: 2,
    elevation: 2,
  },
  cardHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    marginBottom: 12,
  },
  cardAccount: {
    fontSize: 16,
    fontWeight: '600',
    color: '#333',
  },
  cardDescription: {
    fontSize: 12,
    color: '#666',
    marginTop: 2,
  },
  statusBadge: {
    paddingHorizontal: 12,
    paddingVertical: 4,
    borderRadius: 12,
  },
  statusText: {
    color: '#fff',
    fontSize: 12,
    fontWeight: '600',
    textTransform: 'uppercase',
  },
  progressContainer: {
    flexDirection: 'row',
    alignItems: 'center',
    marginBottom: 16,
  },
  progressBar: {
    flex: 1,
    height: 8,
    backgroundColor: '#E0E0E0',
    borderRadius: 4,
    overflow: 'hidden',
    marginRight: 8,
  },
  progressFill: {
    height: '100%',
  },
  progressText: {
    fontSize: 12,
    fontWeight: '600',
    color: '#666',
    minWidth: 45,
    textAlign: 'right',
  },
  amounts: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    marginBottom: 12,
  },
  amountItem: {
    alignItems: 'center',
  },
  amountLabel: {
    fontSize: 10,
    color: '#666',
    marginBottom: 4,
  },
  amountValue: {
    fontSize: 16,
    fontWeight: '600',
    color: '#333',
  },
  exceededText: {
    color: '#F44336',
  },
  period: {
    fontSize: 12,
    color: '#666',
    marginBottom: 12,
  },
  alertBox: {
    backgroundColor: '#FFEBEE',
    padding: 8,
    borderRadius: 4,
    marginBottom: 12,
  },
  warningBox: {
    backgroundColor: '#FFF3E0',
  },
  alertText: {
    color: '#C62828',
    fontSize: 12,
    fontWeight: '600',
  },
  warningText: {
    color: '#E65100',
    fontSize: 12,
    fontWeight: '600',
  },
  actions: {
    flexDirection: 'row',
    gap: 8,
  },
  actionButton: {
    flex: 1,
    padding: 8,
    borderRadius: 4,
    borderWidth: 1,
    borderColor: '#007AFF',
    alignItems: 'center',
  },
  deleteButton: {
    borderColor: '#F44336',
  },
  actionText: {
    color: '#007AFF',
    fontSize: 14,
    fontWeight: '500',
  },
  deleteText: {
    color: '#F44336',
  },
  emptyState: {
    padding: 32,
    alignItems: 'center',
  },
  emptyText: {
    fontSize: 16,
    color: '#999',
    marginBottom: 16,
  },
  emptyButton: {
    backgroundColor: '#007AFF',
    paddingHorizontal: 24,
    paddingVertical: 12,
    borderRadius: 8,
  },
  emptyButtonText: {
    color: '#fff',
    fontSize: 14,
    fontWeight: '600',
  },
});
