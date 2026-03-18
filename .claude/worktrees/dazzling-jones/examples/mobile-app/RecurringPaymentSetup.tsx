/**
 * Recurring Payment Setup Component
 * Create and manage recurring payment schedules
 */
import React, { useState } from 'react';
import {
  View,
  Text,
  TextInput,
  TouchableOpacity,
  StyleSheet,
  ScrollView,
  Alert,
} from 'react-native';
import { Picker } from '@react-native-picker/picker';
import DateTimePicker from '@react-native-community/datetimepicker';
import { useCreateRecurringPayment } from '@icn/react-native';

type PaymentFrequency = 'daily' | 'weekly' | 'monthly' | 'yearly';

export function RecurringPaymentSetup({ onComplete }: { onComplete?: () => void }) {
  const { create, creating, error } = useCreateRecurringPayment();

  const [formData, setFormData] = useState({
    from_account: '',
    to_account: '',
    amount: '',
    currency: 'USD',
    frequency: 'monthly' as PaymentFrequency,
    description: '',
  });

  const [startDate, setStartDate] = useState(new Date());
  const [endDate, setEndDate] = useState<Date | null>(null);
  const [showStartPicker, setShowStartPicker] = useState(false);
  const [showEndPicker, setShowEndPicker] = useState(false);

  const handleSubmit = async () => {
    // Validation
    if (!formData.from_account || !formData.to_account) {
      Alert.alert('Error', 'Please enter account details');
      return;
    }

    const amount = parseInt(formData.amount, 10);
    if (isNaN(amount) || amount <= 0) {
      Alert.alert('Error', 'Please enter a valid amount');
      return;
    }

    try {
      await create({
        from_account: formData.from_account,
        to_account: formData.to_account,
        amount,
        currency: formData.currency,
        frequency: formData.frequency,
        description: formData.description,
        start_date: Math.floor(startDate.getTime() / 1000),
        end_date: endDate ? Math.floor(endDate.getTime() / 1000) : undefined,
      });

      Alert.alert('Success', 'Recurring payment created');
      onComplete?.();
    } catch (err) {
      Alert.alert('Error', error || 'Failed to create payment');
    }
  };

  return (
    <ScrollView style={styles.container}>
      <Text style={styles.title}>Set Up Recurring Payment</Text>

      {/* From Account */}
      <View style={styles.inputGroup}>
        <Text style={styles.label}>From Account</Text>
        <TextInput
          style={styles.input}
          placeholder="alice-checking"
          value={formData.from_account}
          onChangeText={(text) => setFormData({ ...formData, from_account: text })}
        />
      </View>

      {/* To Account */}
      <View style={styles.inputGroup}>
        <Text style={styles.label}>To Account / Recipient</Text>
        <TextInput
          style={styles.input}
          placeholder="bob-savings"
          value={formData.to_account}
          onChangeText={(text) => setFormData({ ...formData, to_account: text })}
        />
      </View>

      {/* Amount */}
      <View style={styles.inputGroup}>
        <Text style={styles.label}>Amount</Text>
        <View style={styles.amountRow}>
          <Text style={styles.currencySymbol}>$</Text>
          <TextInput
            style={[styles.input, styles.amountInput]}
            placeholder="100.00"
            keyboardType="decimal-pad"
            value={formData.amount}
            onChangeText={(text) => setFormData({ ...formData, amount: text })}
          />
        </View>
      </View>

      {/* Frequency */}
      <View style={styles.inputGroup}>
        <Text style={styles.label}>Frequency</Text>
        <Picker
          selectedValue={formData.frequency}
          onValueChange={(value) => setFormData({ ...formData, frequency: value })}
          style={styles.picker}
        >
          <Picker.Item label="Daily" value="daily" />
          <Picker.Item label="Weekly" value="weekly" />
          <Picker.Item label="Monthly" value="monthly" />
          <Picker.Item label="Yearly" value="yearly" />
        </Picker>
      </View>

      {/* Start Date */}
      <View style={styles.inputGroup}>
        <Text style={styles.label}>Start Date</Text>
        <TouchableOpacity
          style={styles.dateButton}
          onPress={() => setShowStartPicker(true)}
        >
          <Text style={styles.dateText}>{startDate.toLocaleDateString()}</Text>
        </TouchableOpacity>
        {showStartPicker && (
          <DateTimePicker
            value={startDate}
            mode="date"
            onChange={(event, date) => {
              setShowStartPicker(false);
              if (date) setStartDate(date);
            }}
          />
        )}
      </View>

      {/* End Date (Optional) */}
      <View style={styles.inputGroup}>
        <Text style={styles.label}>End Date (Optional)</Text>
        <TouchableOpacity
          style={styles.dateButton}
          onPress={() => setShowEndPicker(true)}
        >
          <Text style={styles.dateText}>
            {endDate ? endDate.toLocaleDateString() : 'No end date'}
          </Text>
        </TouchableOpacity>
        {showEndPicker && (
          <DateTimePicker
            value={endDate || new Date()}
            mode="date"
            onChange={(event, date) => {
              setShowEndPicker(false);
              setEndDate(date || null);
            }}
          />
        )}
      </View>

      {/* Description */}
      <View style={styles.inputGroup}>
        <Text style={styles.label}>Description</Text>
        <TextInput
          style={[styles.input, styles.textArea]}
          placeholder="Monthly subscription"
          multiline
          numberOfLines={3}
          value={formData.description}
          onChangeText={(text) => setFormData({ ...formData, description: text })}
        />
      </View>

      {/* Preview */}
      <View style={styles.preview}>
        <Text style={styles.previewTitle}>Summary</Text>
        <Text style={styles.previewText}>
          ${formData.amount || '0.00'} {formData.frequency}
        </Text>
        <Text style={styles.previewText}>
          From {formData.from_account || '(not set)'}
        </Text>
        <Text style={styles.previewText}>
          To {formData.to_account || '(not set)'}
        </Text>
      </View>

      {/* Error */}
      {error && (
        <View style={styles.errorContainer}>
          <Text style={styles.errorText}>{error}</Text>
        </View>
      )}

      {/* Submit Button */}
      <TouchableOpacity
        style={[styles.submitButton, creating && styles.submitButtonDisabled]}
        onPress={handleSubmit}
        disabled={creating}
      >
        <Text style={styles.submitButtonText}>
          {creating ? 'Creating...' : 'Create Recurring Payment'}
        </Text>
      </TouchableOpacity>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#f5f5f5',
    padding: 16,
  },
  title: {
    fontSize: 24,
    fontWeight: 'bold',
    marginBottom: 24,
    color: '#333',
  },
  inputGroup: {
    marginBottom: 20,
  },
  label: {
    fontSize: 14,
    fontWeight: '600',
    marginBottom: 8,
    color: '#333',
  },
  input: {
    backgroundColor: '#fff',
    borderWidth: 1,
    borderColor: '#ddd',
    borderRadius: 8,
    padding: 12,
    fontSize: 16,
  },
  amountRow: {
    flexDirection: 'row',
    alignItems: 'center',
  },
  currencySymbol: {
    fontSize: 24,
    fontWeight: 'bold',
    marginRight: 8,
    color: '#666',
  },
  amountInput: {
    flex: 1,
  },
  picker: {
    backgroundColor: '#fff',
    borderWidth: 1,
    borderColor: '#ddd',
    borderRadius: 8,
  },
  dateButton: {
    backgroundColor: '#fff',
    borderWidth: 1,
    borderColor: '#ddd',
    borderRadius: 8,
    padding: 12,
  },
  dateText: {
    fontSize: 16,
    color: '#333',
  },
  textArea: {
    height: 80,
    textAlignVertical: 'top',
  },
  preview: {
    backgroundColor: '#E3F2FD',
    padding: 16,
    borderRadius: 8,
    marginBottom: 20,
  },
  previewTitle: {
    fontSize: 16,
    fontWeight: '600',
    marginBottom: 8,
    color: '#1976D2',
  },
  previewText: {
    fontSize: 14,
    color: '#333',
    marginBottom: 4,
  },
  errorContainer: {
    backgroundColor: '#FFEBEE',
    padding: 12,
    borderRadius: 8,
    marginBottom: 16,
  },
  errorText: {
    color: '#C62828',
    fontSize: 14,
  },
  submitButton: {
    backgroundColor: '#007AFF',
    padding: 16,
    borderRadius: 8,
    alignItems: 'center',
    marginBottom: 32,
  },
  submitButtonDisabled: {
    backgroundColor: '#999',
  },
  submitButtonText: {
    color: '#fff',
    fontSize: 16,
    fontWeight: '600',
  },
});
