/**
 * Coop Wallet - ICN React Native App
 *
 * Features:
 * - Authentication with secure wallet
 * - Balance display and payment
 * - QR code scan-to-pay
 * - Governance voting
 * - SDIS Identity verification
 */

import React, { useEffect, useState, useCallback } from 'react';
import { NavigationContainer } from '@react-navigation/native';
import { createNativeStackNavigator } from '@react-navigation/native-stack';
import {
  ActivityIndicator,
  View,
  StyleSheet,
  Platform,
  Text,
  TextInput,
  TouchableOpacity,
  ScrollView,
  RefreshControl,
  KeyboardAvoidingView,
  Clipboard,
  Alert,
} from 'react-native';
import { StatusBar } from 'expo-status-bar';

// QR Code - conditionally import for web compatibility
let QRCode: any = null;
try {
  QRCode = require('react-native-qrcode-svg').default;
} catch (e) {
  console.log('QR Code library not available');
}

// Clipboard helper
const copyToClipboard = async (text: string) => {
  if (Platform.OS === 'web') {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      return false;
    }
  } else {
    Clipboard.setString(text);
    return true;
  }
};

// Demo transaction data
const DEMO_TRANSACTIONS = [
  { id: '1', type: 'received', from: 'Alice', to: 'You', amount: 2, memo: 'Garden help', date: '2025-12-10' },
  { id: '2', type: 'sent', from: 'You', to: 'Bob', amount: 1.5, memo: 'Tutoring', date: '2025-12-09' },
  { id: '3', type: 'received', from: 'Carol', to: 'You', amount: 3, memo: 'Website design', date: '2025-12-08' },
  { id: '4', type: 'sent', from: 'You', to: 'Dave', amount: 0.5, memo: 'Coffee meetup', date: '2025-12-07' },
  { id: '5', type: 'received', from: 'Eve', to: 'You', amount: 4, memo: 'Car repair help', date: '2025-12-05' },
];

// Import client module - client is set after initializeClient() is called
let clientModule: any = null;
let initializeClient: () => Promise<void> = async () => {};

// Getter to always get the current client value
const getClient = () => clientModule?.client || null;

try {
  clientModule = require('./src/client');
  initializeClient = clientModule.initializeClient;
} catch (e) {
  console.error('Failed to import client:', e);
}

// ============================================================================
// Inline Screens (Web-safe, no external hook dependencies)
// ============================================================================

function LoginScreen({ onLogin }: { onLogin: (coopId: string, did: string) => void }) {
  const [coopId, setCoopId] = useState('test-coop');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleLogin = async () => {
    if (!coopId.trim()) {
      setError('Please enter your cooperative ID');
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const client = getClient();
      if (client) {
        // Use real authentication with the gateway
        const authState = await client.login(coopId.trim(), [
          'coop:read',
          'coop:write',
          'ledger:read',
          'ledger:write',
        ]);
        onLogin(coopId.trim(), authState.did || '');
      } else {
        throw new Error('Client not initialized. Please refresh the page.');
      }
    } catch (err) {
      console.error('Login error:', err);
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <KeyboardAvoidingView
      style={styles.loginContainer}
      behavior={Platform.OS === 'ios' ? 'padding' : 'height'}
    >
      <View style={styles.loginContent}>
        <Text style={styles.loginTitle}>Coop Wallet</Text>
        <Text style={styles.loginSubtitle}>InterCooperative Network</Text>

        <View style={styles.loginForm}>
          <Text style={styles.label}>Cooperative ID</Text>
          <TextInput
            style={styles.input}
            placeholder="e.g., my-timebank"
            value={coopId}
            onChangeText={setCoopId}
            autoCapitalize="none"
            autoCorrect={false}
          />

          {error && <Text style={styles.errorText}>{error}</Text>}

          <TouchableOpacity
            style={[styles.primaryButton, isLoading && styles.buttonDisabled]}
            onPress={handleLogin}
            disabled={isLoading}
          >
            {isLoading ? (
              <ActivityIndicator color="#fff" />
            ) : (
              <Text style={styles.buttonText}>Login</Text>
            )}
          </TouchableOpacity>
        </View>

        <Text style={styles.footerText}>
          Your keys are stored securely on your device.
        </Text>
      </View>
    </KeyboardAvoidingView>
  );
}

function HomeScreen({
  navigation,
  coopId,
  userDid,
  onLogout
}: {
  navigation: any;
  coopId: string;
  userDid: string;
  onLogout: () => void;
}) {
  const [balance, setBalance] = useState(0);
  const [isLoading, setIsLoading] = useState(false);

  useEffect(() => {
    // Try to fetch real balance from API
    async function fetchBalance() {
      const client = getClient();
      if (client && coopId && userDid) {
        try {
          const result = await client.getBalance(coopId, userDid);
          setBalance(result?.balance ?? 0);
        } catch (e) {
          console.warn('Failed to fetch balance, using default:', e);
          setBalance(0);
        }
      }
    }
    fetchBalance();
  }, [coopId, userDid]);

  const refresh = useCallback(() => {
    setIsLoading(true);
    setTimeout(() => {
      setBalance(Math.floor(Math.random() * 100));
      setIsLoading(false);
    }, 1000);
  }, []);

  const handleLogout = async () => {
    onLogout();
  };

  const formatDid = (did: string) => {
    if (did && did.length > 20) {
      return `${did.slice(0, 12)}...${did.slice(-6)}`;
    }
    return did || 'Unknown';
  };

  return (
    <ScrollView
      style={styles.container}
      refreshControl={
        <RefreshControl refreshing={isLoading} onRefresh={refresh} />
      }
    >
      {/* Balance Card */}
      <View style={styles.balanceCard}>
        <Text style={styles.balanceLabel}>Available Balance</Text>
        <Text style={styles.balanceAmount}>
          {balance}
          <Text style={styles.balanceCurrency}> hours</Text>
        </Text>
        <Text style={styles.coopName}>{coopId}</Text>
        <Text style={styles.didText}>{formatDid(userDid)}</Text>
      </View>

      {/* Quick Actions */}
      <View style={styles.actionsGrid}>
        <TouchableOpacity
          style={styles.actionButton}
          onPress={() => navigation.navigate('Payment')}
        >
          <Text style={styles.actionIcon}>💸</Text>
          <Text style={styles.actionLabel}>Send</Text>
        </TouchableOpacity>

        <TouchableOpacity
          style={styles.actionButton}
          onPress={() => navigation.navigate('Receive')}
        >
          <Text style={styles.actionIcon}>📥</Text>
          <Text style={styles.actionLabel}>Receive</Text>
        </TouchableOpacity>

        <TouchableOpacity
          style={styles.actionButton}
          onPress={() => navigation.navigate('Scan')}
        >
          <Text style={styles.actionIcon}>📷</Text>
          <Text style={styles.actionLabel}>Scan</Text>
        </TouchableOpacity>

        <TouchableOpacity
          style={styles.actionButton}
          onPress={() => navigation.navigate('Governance')}
        >
          <Text style={styles.actionIcon}>🗳️</Text>
          <Text style={styles.actionLabel}>Vote</Text>
        </TouchableOpacity>
      </View>

      {/* Recent Transactions */}
      <View style={styles.section}>
        <View style={styles.sectionHeader}>
          <Text style={styles.sectionTitle}>Recent Activity</Text>
          <TouchableOpacity onPress={() => navigation.navigate('Transactions')}>
            <Text style={styles.seeAllLink}>See All</Text>
          </TouchableOpacity>
        </View>

        {DEMO_TRANSACTIONS.slice(0, 3).map((tx) => (
          <View key={tx.id} style={styles.transactionItem}>
            <View style={[
              styles.txIcon,
              tx.type === 'received' ? styles.txIconReceived : styles.txIconSent
            ]}>
              <Text style={styles.txIconText}>{tx.type === 'received' ? '↓' : '↑'}</Text>
            </View>
            <View style={styles.txContent}>
              <Text style={styles.txPerson}>
                {tx.type === 'received' ? `From ${tx.from}` : `To ${tx.to}`}
              </Text>
              <Text style={styles.txMemo}>{tx.memo}</Text>
            </View>
            <View style={styles.txAmountContainer}>
              <Text style={[
                styles.txAmount,
                tx.type === 'received' ? styles.txAmountReceived : styles.txAmountSent
              ]}>
                {tx.type === 'received' ? '+' : '-'}{tx.amount}h
              </Text>
              <Text style={styles.txDate}>{tx.date}</Text>
            </View>
          </View>
        ))}
      </View>

      {/* SDIS Section */}
      <View style={styles.section}>
        <Text style={styles.sectionTitle}>Identity & Verification</Text>

        <TouchableOpacity
          style={styles.menuItem}
          onPress={() => navigation.navigate('Identity')}
        >
          <Text style={styles.menuIcon}>🪪</Text>
          <View style={styles.menuContent}>
            <Text style={styles.menuLabel}>My Identity</Text>
            <Text style={styles.menuDescription}>View and share your identity</Text>
          </View>
          <Text style={styles.menuArrow}>›</Text>
        </TouchableOpacity>

        <TouchableOpacity
          style={styles.menuItem}
          onPress={() => navigation.navigate('Verify')}
        >
          <Text style={styles.menuIcon}>✅</Text>
          <View style={styles.menuContent}>
            <Text style={styles.menuLabel}>Verify Someone</Text>
            <Text style={styles.menuDescription}>Scan and verify another member</Text>
          </View>
          <Text style={styles.menuArrow}>›</Text>
        </TouchableOpacity>

        <TouchableOpacity
          style={styles.menuItem}
          onPress={() => navigation.navigate('VerificationHistory')}
        >
          <Text style={styles.menuIcon}>📋</Text>
          <View style={styles.menuContent}>
            <Text style={styles.menuLabel}>Verification History</Text>
            <Text style={styles.menuDescription}>View past verifications</Text>
          </View>
          <Text style={styles.menuArrow}>›</Text>
        </TouchableOpacity>
      </View>

      {/* Settings */}
      <TouchableOpacity
        style={styles.menuItem}
        onPress={() => navigation.navigate('Settings')}
      >
        <Text style={styles.menuIcon}>⚙️</Text>
        <View style={styles.menuContent}>
          <Text style={styles.menuLabel}>Settings</Text>
          <Text style={styles.menuDescription}>App preferences and account</Text>
        </View>
        <Text style={styles.menuArrow}>›</Text>
      </TouchableOpacity>

      {/* Logout Button */}
      <TouchableOpacity style={styles.logoutButton} onPress={handleLogout}>
        <Text style={styles.logoutText}>Logout</Text>
      </TouchableOpacity>
    </ScrollView>
  );
}

// ============================================================================
// Payment Screen - Send hours to another member
// ============================================================================
function PaymentScreen({ navigation }: { navigation: any }) {
  const [recipient, setRecipient] = useState('');
  const [amount, setAmount] = useState('');
  const [memo, setMemo] = useState('');
  const [isLoading, setIsLoading] = useState(false);
  const [success, setSuccess] = useState(false);

  const handleSend = async () => {
    if (!recipient.trim() || !amount.trim()) {
      alert('Please enter recipient and amount');
      return;
    }

    const amountNum = parseFloat(amount);
    if (isNaN(amountNum) || amountNum <= 0) {
      alert('Please enter a valid amount');
      return;
    }

    setIsLoading(true);
    // Simulate API call
    setTimeout(() => {
      setIsLoading(false);
      setSuccess(true);
    }, 1500);
  };

  if (success) {
    return (
      <View style={styles.successContainer}>
        <Text style={styles.successIcon}>✅</Text>
        <Text style={styles.successTitle}>Payment Sent!</Text>
        <Text style={styles.successAmount}>{amount} hours</Text>
        <Text style={styles.successRecipient}>to {recipient}</Text>
        {memo && <Text style={styles.successMemo}>"{memo}"</Text>}
        <TouchableOpacity
          style={styles.primaryButton}
          onPress={() => navigation.goBack()}
        >
          <Text style={styles.buttonText}>Done</Text>
        </TouchableOpacity>
      </View>
    );
  }

  return (
    <KeyboardAvoidingView
      style={styles.formContainer}
      behavior={Platform.OS === 'ios' ? 'padding' : 'height'}
    >
      <ScrollView style={styles.formScroll}>
        <View style={styles.formSection}>
          <Text style={styles.label}>Recipient DID or Name</Text>
          <TextInput
            style={styles.input}
            placeholder="did:icn:... or @username"
            value={recipient}
            onChangeText={setRecipient}
            autoCapitalize="none"
          />
        </View>

        <View style={styles.formSection}>
          <Text style={styles.label}>Amount (hours)</Text>
          <TextInput
            style={styles.amountInput}
            placeholder="0.0"
            value={amount}
            onChangeText={setAmount}
            keyboardType="decimal-pad"
          />
        </View>

        <View style={styles.formSection}>
          <Text style={styles.label}>Memo (optional)</Text>
          <TextInput
            style={[styles.input, styles.memoInput]}
            placeholder="What's this for?"
            value={memo}
            onChangeText={setMemo}
            multiline
            numberOfLines={3}
          />
        </View>

        <TouchableOpacity
          style={[styles.primaryButton, styles.sendButton, isLoading && styles.buttonDisabled]}
          onPress={handleSend}
          disabled={isLoading}
        >
          {isLoading ? (
            <ActivityIndicator color="#fff" />
          ) : (
            <Text style={styles.buttonText}>Send Payment</Text>
          )}
        </TouchableOpacity>
      </ScrollView>
    </KeyboardAvoidingView>
  );
}

// ============================================================================
// Receive Screen - Display QR code for receiving payments
// ============================================================================
function ReceiveScreen({ route }: { route: any }) {
  const [amount, setAmount] = useState('');
  const [copied, setCopied] = useState(false);
  const did = 'did:icn:demo123456789abcdef';

  const qrData = JSON.stringify({
    type: 'icn-payment-request',
    recipient: did,
    amount: amount ? parseFloat(amount) : undefined,
    timestamp: Date.now(),
  });

  const handleCopyDid = async () => {
    const success = await copyToClipboard(did);
    if (success) {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  };

  const handleShare = async () => {
    const paymentLink = `icn://pay?to=${did}${amount ? `&amount=${amount}` : ''}`;
    const success = await copyToClipboard(paymentLink);
    if (success) {
      if (Platform.OS === 'web') {
        alert('Payment link copied to clipboard!');
      } else {
        Alert.alert('Copied', 'Payment link copied to clipboard');
      }
    }
  };

  return (
    <ScrollView style={styles.receiveContainer}>
      <View style={styles.qrCard}>
        <Text style={styles.qrTitle}>Scan to Pay Me</Text>

        {/* QR Code */}
        <View style={styles.qrWrapper}>
          {QRCode && Platform.OS !== 'web' ? (
            <QRCode value={qrData} size={180} />
          ) : (
            <View style={styles.qrPlaceholder}>
              <View style={styles.qrFallback}>
                <Text style={styles.qrFallbackText}>⬛⬜⬛⬜⬛</Text>
                <Text style={styles.qrFallbackText}>⬜⬛⬜⬛⬜</Text>
                <Text style={styles.qrFallbackText}>⬛⬜⬛⬜⬛</Text>
                <Text style={styles.qrFallbackText}>⬜⬛⬜⬛⬜</Text>
                <Text style={styles.qrFallbackText}>⬛⬜⬛⬜⬛</Text>
              </View>
              <Text style={styles.qrPlaceholderLabel}>QR Code</Text>
            </View>
          )}
        </View>

        {amount && (
          <View style={styles.requestAmount}>
            <Text style={styles.requestAmountLabel}>Requesting</Text>
            <Text style={styles.requestAmountValue}>{amount} hours</Text>
          </View>
        )}

        <View style={styles.formSection}>
          <Text style={styles.label}>Request Amount (optional)</Text>
          <TextInput
            style={styles.input}
            placeholder="Enter amount in hours"
            value={amount}
            onChangeText={setAmount}
            keyboardType="decimal-pad"
          />
        </View>

        <View style={styles.didDisplay}>
          <Text style={styles.didLabel}>Your DID:</Text>
          <Text style={styles.didValue} selectable>{did}</Text>
        </View>
      </View>

      <TouchableOpacity
        style={[styles.copyButton, copied && styles.copyButtonSuccess]}
        onPress={handleCopyDid}
      >
        <Text style={[styles.copyButtonText, copied && styles.copyButtonTextSuccess]}>
          {copied ? '✓ Copied!' : '📋 Copy DID'}
        </Text>
      </TouchableOpacity>

      <TouchableOpacity style={styles.shareButton} onPress={handleShare}>
        <Text style={styles.shareButtonText}>📤 Share Payment Link</Text>
      </TouchableOpacity>
    </ScrollView>
  );
}

// ============================================================================
// Scan Screen - QR code scanner
// ============================================================================
function ScanScreen({ navigation }: { navigation: any }) {
  const [manualEntry, setManualEntry] = useState('');

  if (Platform.OS === 'web') {
    return (
      <View style={styles.scanContainer}>
        <View style={styles.cameraPlaceholder}>
          <Text style={styles.cameraIcon}>📷</Text>
          <Text style={styles.cameraText}>Camera not available on web</Text>
        </View>

        <View style={styles.manualEntrySection}>
          <Text style={styles.sectionTitle}>Manual Entry</Text>
          <TextInput
            style={styles.input}
            placeholder="Paste DID or payment request"
            value={manualEntry}
            onChangeText={setManualEntry}
            multiline
          />
          <TouchableOpacity
            style={styles.primaryButton}
            onPress={() => {
              if (manualEntry.trim()) {
                alert('Processing: ' + manualEntry.slice(0, 50) + '...');
              }
            }}
          >
            <Text style={styles.buttonText}>Process</Text>
          </TouchableOpacity>
        </View>
      </View>
    );
  }

  return (
    <View style={styles.scanContainer}>
      <View style={styles.cameraPlaceholder}>
        <Text style={styles.cameraIcon}>📷</Text>
        <Text style={styles.cameraText}>Point at QR Code</Text>
      </View>
    </View>
  );
}

// ============================================================================
// Governance Screen - View and vote on proposals
// ============================================================================
function GovernanceScreen({ navigation }: { navigation: any }) {
  const [proposals, setProposals] = useState([
    {
      id: '1',
      title: 'Increase new member credit limit',
      description: 'Raise the initial credit limit for new members from 10 to 20 hours.',
      status: 'active',
      votesFor: 23,
      votesAgainst: 5,
      endDate: '2025-12-20',
      myVote: null as 'for' | 'against' | null,
    },
    {
      id: '2',
      title: 'Add childcare service category',
      description: 'Create a new service category for childcare and babysitting services.',
      status: 'active',
      votesFor: 45,
      votesAgainst: 2,
      endDate: '2025-12-18',
      myVote: null as 'for' | 'against' | null,
    },
    {
      id: '3',
      title: 'Monthly community meetup',
      description: 'Establish a monthly in-person meetup for cooperative members.',
      status: 'passed',
      votesFor: 67,
      votesAgainst: 8,
      endDate: '2025-12-01',
      myVote: 'for' as 'for' | 'against' | null,
    },
  ]);

  const handleVote = (proposalId: string, voteType: 'for' | 'against') => {
    setProposals(prev => prev.map(p => {
      if (p.id !== proposalId) return p;

      // If already voted, don't allow changing
      if (p.myVote !== null) {
        if (Platform.OS === 'web') {
          alert('You have already voted on this proposal');
        } else {
          Alert.alert('Already Voted', 'You have already voted on this proposal');
        }
        return p;
      }

      // Record vote
      return {
        ...p,
        myVote: voteType,
        votesFor: voteType === 'for' ? p.votesFor + 1 : p.votesFor,
        votesAgainst: voteType === 'against' ? p.votesAgainst + 1 : p.votesAgainst,
      };
    }));
  };

  const totalVotes = proposals.reduce((sum, p) => sum + p.votesFor + p.votesAgainst, 0);
  const activeCount = proposals.filter(p => p.status === 'active').length;

  return (
    <ScrollView style={styles.governanceContainer}>
      <View style={styles.govStats}>
        <View style={styles.govStat}>
          <Text style={styles.govStatNumber}>{proposals.length}</Text>
          <Text style={styles.govStatLabel}>Proposals</Text>
        </View>
        <View style={styles.govStat}>
          <Text style={styles.govStatNumber}>{activeCount}</Text>
          <Text style={styles.govStatLabel}>Active</Text>
        </View>
        <View style={styles.govStat}>
          <Text style={styles.govStatNumber}>{totalVotes}</Text>
          <Text style={styles.govStatLabel}>Total Votes</Text>
        </View>
      </View>

      {proposals.map((proposal) => (
        <View key={proposal.id} style={styles.proposalCard}>
          <View style={styles.proposalHeader}>
            <Text style={styles.proposalTitle}>{proposal.title}</Text>
            <View style={[
              styles.statusBadge,
              proposal.status === 'active' ? styles.statusActive : styles.statusPassed
            ]}>
              <Text style={styles.statusText}>
                {proposal.status === 'active' ? '🗳️ Active' : '✅ Passed'}
              </Text>
            </View>
          </View>
          <Text style={styles.proposalDescription}>{proposal.description}</Text>
          <View style={styles.voteBar}>
            <View style={[styles.voteFor, { flex: proposal.votesFor || 1 }]} />
            <View style={[styles.voteAgainst, { flex: proposal.votesAgainst || 1 }]} />
          </View>
          <View style={styles.voteStats}>
            <Text style={styles.voteText}>👍 {proposal.votesFor}</Text>
            <Text style={styles.voteText}>👎 {proposal.votesAgainst}</Text>
            <Text style={styles.endDate}>Ends: {proposal.endDate}</Text>
          </View>

          {proposal.myVote && (
            <View style={styles.votedBadge}>
              <Text style={styles.votedText}>
                You voted {proposal.myVote === 'for' ? '👍 For' : '👎 Against'}
              </Text>
            </View>
          )}

          {proposal.status === 'active' && !proposal.myVote && (
            <View style={styles.voteButtons}>
              <TouchableOpacity
                style={styles.voteForButton}
                onPress={() => handleVote(proposal.id, 'for')}
              >
                <Text style={styles.voteButtonText}>Vote For</Text>
              </TouchableOpacity>
              <TouchableOpacity
                style={styles.voteAgainstButton}
                onPress={() => handleVote(proposal.id, 'against')}
              >
                <Text style={styles.voteButtonText}>Vote Against</Text>
              </TouchableOpacity>
            </View>
          )}
        </View>
      ))}
    </ScrollView>
  );
}

// ============================================================================
// Identity Screen - View and share your SDIS identity
// ============================================================================
function IdentityScreen({ navigation }: { navigation: any }) {
  const [proofType, setProofType] = useState<'membership' | 'age' | 'reputation'>('membership');
  const did = 'did:icn:demo123456789abcdef';

  const proofTypes = [
    { id: 'membership', label: 'Membership', icon: '🏠', desc: 'Prove you are a member' },
    { id: 'age', label: 'Age (18+)', icon: '🔞', desc: 'Prove you are 18 or older' },
    { id: 'reputation', label: 'Reputation', icon: '⭐', desc: 'Show your trust score' },
  ];

  return (
    <ScrollView style={styles.identityContainer}>
      {/* Identity Card */}
      <View style={styles.identityCard}>
        <View style={styles.identityAvatar}>
          <Text style={styles.avatarText}>👤</Text>
        </View>
        <Text style={styles.identityName}>Demo User</Text>
        <Text style={styles.identityDid} selectable>{did}</Text>
        <View style={styles.identityStats}>
          <View style={styles.identityStat}>
            <Text style={styles.identityStatValue}>4.8</Text>
            <Text style={styles.identityStatLabel}>Trust Score</Text>
          </View>
          <View style={styles.identityStat}>
            <Text style={styles.identityStatValue}>127</Text>
            <Text style={styles.identityStatLabel}>Transactions</Text>
          </View>
          <View style={styles.identityStat}>
            <Text style={styles.identityStatValue}>2y</Text>
            <Text style={styles.identityStatLabel}>Member Since</Text>
          </View>
        </View>
      </View>

      {/* Proof Generation */}
      <View style={styles.proofSection}>
        <Text style={styles.sectionTitle}>Generate Proof</Text>
        <Text style={styles.sectionSubtitle}>
          Create a one-time proof to share with others
        </Text>

        {proofTypes.map((type) => (
          <TouchableOpacity
            key={type.id}
            style={[
              styles.proofTypeOption,
              proofType === type.id && styles.proofTypeSelected
            ]}
            onPress={() => setProofType(type.id as any)}
          >
            <Text style={styles.proofTypeIcon}>{type.icon}</Text>
            <View style={styles.proofTypeContent}>
              <Text style={styles.proofTypeLabel}>{type.label}</Text>
              <Text style={styles.proofTypeDesc}>{type.desc}</Text>
            </View>
            {proofType === type.id && <Text style={styles.checkmark}>✓</Text>}
          </TouchableOpacity>
        ))}

        <TouchableOpacity style={styles.generateButton}>
          <Text style={styles.generateButtonText}>Generate QR Proof</Text>
        </TouchableOpacity>
      </View>

      {/* Credentials */}
      <View style={styles.credentialsSection}>
        <Text style={styles.sectionTitle}>My Credentials</Text>
        <View style={styles.credentialItem}>
          <Text style={styles.credentialIcon}>📧</Text>
          <View style={styles.credentialContent}>
            <Text style={styles.credentialLabel}>Email Verified</Text>
            <Text style={styles.credentialValue}>user@example.com</Text>
          </View>
          <Text style={styles.credentialStatus}>✅</Text>
        </View>
        <View style={styles.credentialItem}>
          <Text style={styles.credentialIcon}>📱</Text>
          <View style={styles.credentialContent}>
            <Text style={styles.credentialLabel}>Phone Verified</Text>
            <Text style={styles.credentialValue}>+1 (555) 123-4567</Text>
          </View>
          <Text style={styles.credentialStatus}>✅</Text>
        </View>
        <View style={styles.credentialItem}>
          <Text style={styles.credentialIcon}>🏠</Text>
          <View style={styles.credentialContent}>
            <Text style={styles.credentialLabel}>Cooperative Member</Text>
            <Text style={styles.credentialValue}>Since Dec 2023</Text>
          </View>
          <Text style={styles.credentialStatus}>✅</Text>
        </View>
      </View>
    </ScrollView>
  );
}

// ============================================================================
// Verify Screen - Verify another member's identity
// ============================================================================
function VerifyScreen({ navigation }: { navigation: any }) {
  const [verificationResult, setVerificationResult] = useState<null | {
    valid: boolean;
    name: string;
    did: string;
    proofType: string;
    trustScore: number;
  }>(null);

  const simulateVerification = () => {
    // Simulate scanning a QR code
    setTimeout(() => {
      setVerificationResult({
        valid: true,
        name: 'Alice Member',
        did: 'did:icn:alice789xyz',
        proofType: 'Membership',
        trustScore: 4.5,
      });
    }, 1000);
  };

  if (verificationResult) {
    return (
      <View style={styles.verifyResultContainer}>
        <View style={[
          styles.verifyResultCard,
          verificationResult.valid ? styles.verifyValid : styles.verifyInvalid
        ]}>
          <Text style={styles.verifyResultIcon}>
            {verificationResult.valid ? '✅' : '❌'}
          </Text>
          <Text style={styles.verifyResultTitle}>
            {verificationResult.valid ? 'Verified!' : 'Invalid'}
          </Text>

          {verificationResult.valid && (
            <>
              <Text style={styles.verifyResultName}>{verificationResult.name}</Text>
              <Text style={styles.verifyResultDid}>{verificationResult.did}</Text>

              <View style={styles.verifyResultDetails}>
                <View style={styles.verifyResultDetail}>
                  <Text style={styles.verifyDetailLabel}>Proof Type</Text>
                  <Text style={styles.verifyDetailValue}>{verificationResult.proofType}</Text>
                </View>
                <View style={styles.verifyResultDetail}>
                  <Text style={styles.verifyDetailLabel}>Trust Score</Text>
                  <Text style={styles.verifyDetailValue}>⭐ {verificationResult.trustScore}</Text>
                </View>
              </View>
            </>
          )}
        </View>

        <TouchableOpacity
          style={styles.primaryButton}
          onPress={() => setVerificationResult(null)}
        >
          <Text style={styles.buttonText}>Scan Another</Text>
        </TouchableOpacity>
      </View>
    );
  }

  return (
    <View style={styles.verifyContainer}>
      {Platform.OS === 'web' ? (
        <View style={styles.cameraPlaceholder}>
          <Text style={styles.cameraIcon}>📷</Text>
          <Text style={styles.cameraText}>Camera not available on web</Text>
          <TouchableOpacity
            style={[styles.primaryButton, { marginTop: 20 }]}
            onPress={simulateVerification}
          >
            <Text style={styles.buttonText}>Simulate Scan</Text>
          </TouchableOpacity>
        </View>
      ) : (
        <View style={styles.cameraPlaceholder}>
          <Text style={styles.cameraIcon}>📷</Text>
          <Text style={styles.cameraText}>Point at identity QR code</Text>
        </View>
      )}

      <View style={styles.verifyInstructions}>
        <Text style={styles.instructionTitle}>How to verify</Text>
        <Text style={styles.instructionText}>
          1. Ask the person to show their identity QR code{'\n'}
          2. Point your camera at the code{'\n'}
          3. Wait for verification result
        </Text>
      </View>
    </View>
  );
}

// ============================================================================
// Verification History Screen
// ============================================================================
function VerificationHistoryScreen() {
  const [history] = useState([
    {
      id: '1',
      name: 'Bob Builder',
      did: 'did:icn:bob123',
      date: '2025-12-10',
      type: 'Membership',
      direction: 'verified', // you verified them
    },
    {
      id: '2',
      name: 'Carol Worker',
      did: 'did:icn:carol456',
      date: '2025-12-09',
      type: 'Age (18+)',
      direction: 'verified',
    },
    {
      id: '3',
      name: 'Dave Helper',
      did: 'did:icn:dave789',
      date: '2025-12-08',
      type: 'Reputation',
      direction: 'was_verified', // they verified you
    },
  ]);

  return (
    <ScrollView style={styles.historyContainer}>
      <View style={styles.historyStats}>
        <View style={styles.historyStat}>
          <Text style={styles.historyStatNumber}>12</Text>
          <Text style={styles.historyStatLabel}>You Verified</Text>
        </View>
        <View style={styles.historyStat}>
          <Text style={styles.historyStatNumber}>8</Text>
          <Text style={styles.historyStatLabel}>Verified You</Text>
        </View>
      </View>

      {history.map((item) => (
        <View key={item.id} style={styles.historyItem}>
          <View style={styles.historyIcon}>
            <Text style={styles.historyIconText}>
              {item.direction === 'verified' ? '→' : '←'}
            </Text>
          </View>
          <View style={styles.historyContent}>
            <Text style={styles.historyName}>{item.name}</Text>
            <Text style={styles.historyDid}>{item.did.slice(0, 20)}...</Text>
            <Text style={styles.historyMeta}>
              {item.type} • {item.date}
            </Text>
          </View>
          <Text style={styles.historyStatus}>✅</Text>
        </View>
      ))}

      {history.length === 0 && (
        <View style={styles.emptyState}>
          <Text style={styles.emptyIcon}>📋</Text>
          <Text style={styles.emptyText}>No verifications yet</Text>
        </View>
      )}
    </ScrollView>
  );
}

// ============================================================================
// Transactions Screen - Full transaction history
// ============================================================================
function TransactionsScreen() {
  const [filter, setFilter] = useState<'all' | 'sent' | 'received'>('all');

  const filteredTx = DEMO_TRANSACTIONS.filter(tx => {
    if (filter === 'all') return true;
    return tx.type === filter;
  });

  const totalSent = DEMO_TRANSACTIONS.filter(t => t.type === 'sent')
    .reduce((sum, t) => sum + t.amount, 0);
  const totalReceived = DEMO_TRANSACTIONS.filter(t => t.type === 'received')
    .reduce((sum, t) => sum + t.amount, 0);

  return (
    <ScrollView style={styles.transactionsContainer}>
      <View style={styles.txSummary}>
        <View style={styles.txSummaryItem}>
          <Text style={styles.txSummaryLabel}>Total Received</Text>
          <Text style={[styles.txSummaryValue, styles.txAmountReceived]}>+{totalReceived}h</Text>
        </View>
        <View style={styles.txSummaryItem}>
          <Text style={styles.txSummaryLabel}>Total Sent</Text>
          <Text style={[styles.txSummaryValue, styles.txAmountSent]}>-{totalSent}h</Text>
        </View>
      </View>

      <View style={styles.filterRow}>
        {(['all', 'received', 'sent'] as const).map((f) => (
          <TouchableOpacity
            key={f}
            style={[styles.filterButton, filter === f && styles.filterButtonActive]}
            onPress={() => setFilter(f)}
          >
            <Text style={[styles.filterText, filter === f && styles.filterTextActive]}>
              {f.charAt(0).toUpperCase() + f.slice(1)}
            </Text>
          </TouchableOpacity>
        ))}
      </View>

      {filteredTx.map((tx) => (
        <View key={tx.id} style={styles.transactionItem}>
          <View style={[
            styles.txIcon,
            tx.type === 'received' ? styles.txIconReceived : styles.txIconSent
          ]}>
            <Text style={styles.txIconText}>{tx.type === 'received' ? '↓' : '↑'}</Text>
          </View>
          <View style={styles.txContent}>
            <Text style={styles.txPerson}>
              {tx.type === 'received' ? `From ${tx.from}` : `To ${tx.to}`}
            </Text>
            <Text style={styles.txMemo}>{tx.memo}</Text>
          </View>
          <View style={styles.txAmountContainer}>
            <Text style={[
              styles.txAmount,
              tx.type === 'received' ? styles.txAmountReceived : styles.txAmountSent
            ]}>
              {tx.type === 'received' ? '+' : '-'}{tx.amount}h
            </Text>
            <Text style={styles.txDate}>{tx.date}</Text>
          </View>
        </View>
      ))}

      {filteredTx.length === 0 && (
        <View style={styles.emptyState}>
          <Text style={styles.emptyIcon}>📭</Text>
          <Text style={styles.emptyText}>No transactions found</Text>
        </View>
      )}
    </ScrollView>
  );
}

// ============================================================================
// Settings Screen - App preferences
// ============================================================================
function SettingsScreen({ onLogout }: { onLogout: () => void }) {
  const [notifications, setNotifications] = useState(true);
  const [biometrics, setBiometrics] = useState(false);
  const did = 'did:icn:demo123456789abcdef';

  const handleExportKeys = async () => {
    if (Platform.OS === 'web') {
      alert('Key export would be initiated here. In production, this would securely export your keys.');
    } else {
      Alert.alert(
        'Export Keys',
        'This will export your private keys. Keep them secure!',
        [
          { text: 'Cancel', style: 'cancel' },
          { text: 'Export', onPress: () => Alert.alert('Exported', 'Keys exported successfully') }
        ]
      );
    }
  };

  return (
    <ScrollView style={styles.settingsContainer}>
      <View style={styles.settingsSection}>
        <Text style={styles.settingsSectionTitle}>Account</Text>

        <View style={styles.settingsItem}>
          <View style={styles.settingsItemContent}>
            <Text style={styles.settingsItemLabel}>Your DID</Text>
            <Text style={styles.settingsItemValue} numberOfLines={1}>
              {did.slice(0, 25)}...
            </Text>
          </View>
          <TouchableOpacity
            onPress={async () => {
              await copyToClipboard(did);
              if (Platform.OS === 'web') {
                alert('DID copied!');
              } else {
                Alert.alert('Copied', 'DID copied to clipboard');
              }
            }}
          >
            <Text style={styles.settingsAction}>Copy</Text>
          </TouchableOpacity>
        </View>

        <TouchableOpacity style={styles.settingsItem} onPress={handleExportKeys}>
          <View style={styles.settingsItemContent}>
            <Text style={styles.settingsItemLabel}>Export Keys</Text>
            <Text style={styles.settingsItemDescription}>Backup your private keys</Text>
          </View>
          <Text style={styles.settingsArrow}>›</Text>
        </TouchableOpacity>
      </View>

      <View style={styles.settingsSection}>
        <Text style={styles.settingsSectionTitle}>Preferences</Text>

        <View style={styles.settingsItem}>
          <View style={styles.settingsItemContent}>
            <Text style={styles.settingsItemLabel}>Push Notifications</Text>
            <Text style={styles.settingsItemDescription}>Receive payment alerts</Text>
          </View>
          <TouchableOpacity
            style={[styles.toggle, notifications && styles.toggleOn]}
            onPress={() => setNotifications(!notifications)}
          >
            <View style={[styles.toggleThumb, notifications && styles.toggleThumbOn]} />
          </TouchableOpacity>
        </View>

        <View style={styles.settingsItem}>
          <View style={styles.settingsItemContent}>
            <Text style={styles.settingsItemLabel}>Biometric Auth</Text>
            <Text style={styles.settingsItemDescription}>Use fingerprint or face ID</Text>
          </View>
          <TouchableOpacity
            style={[styles.toggle, biometrics && styles.toggleOn]}
            onPress={() => setBiometrics(!biometrics)}
          >
            <View style={[styles.toggleThumb, biometrics && styles.toggleThumbOn]} />
          </TouchableOpacity>
        </View>
      </View>

      <View style={styles.settingsSection}>
        <Text style={styles.settingsSectionTitle}>About</Text>

        <View style={styles.settingsItem}>
          <View style={styles.settingsItemContent}>
            <Text style={styles.settingsItemLabel}>Version</Text>
          </View>
          <Text style={styles.settingsValue}>0.1.0</Text>
        </View>

        <View style={styles.settingsItem}>
          <View style={styles.settingsItemContent}>
            <Text style={styles.settingsItemLabel}>Network</Text>
          </View>
          <Text style={styles.settingsValue}>ICN Testnet</Text>
        </View>
      </View>

      <TouchableOpacity style={styles.dangerButton} onPress={onLogout}>
        <Text style={styles.dangerButtonText}>Logout</Text>
      </TouchableOpacity>

      <TouchableOpacity style={styles.dangerButtonOutline}>
        <Text style={styles.dangerButtonOutlineText}>Delete Account</Text>
      </TouchableOpacity>

      <Text style={styles.settingsFooter}>
        InterCooperative Network{'\n'}
        Building the cooperative economy
      </Text>
    </ScrollView>
  );
}

// ============================================================================
// Navigation Setup
// ============================================================================

export type RootStackParamList = {
  Login: undefined;
  Home: undefined;
  Payment: undefined;
  Scan: undefined;
  Receive: undefined;
  Governance: undefined;
  Identity: undefined;
  Verify: undefined;
  VerificationHistory: undefined;
  Transactions: undefined;
  Settings: undefined;
};

const Stack = createNativeStackNavigator<RootStackParamList>();

export default function App() {
  const [isReady, setIsReady] = useState(false);
  const [isAuthenticated, setIsAuthenticated] = useState(false);
  const [coopId, setCoopId] = useState<string>('');
  const [userDid, setUserDid] = useState<string>('');

  useEffect(() => {
    async function init() {
      try {
        await initializeClient();
        const client = getClient();
        if (client?.authState?.isAuthenticated) {
          setIsAuthenticated(true);
          setCoopId(client.authState.coopId || '');
          setUserDid(client.authState.did || '');
        }
      } catch (error) {
        console.error('Init error:', error);
      } finally {
        setIsReady(true);
      }
    }
    init();
  }, []);

  const handleLogin = (coop: string, did: string) => {
    setCoopId(coop);
    setUserDid(did);
    setIsAuthenticated(true);
  };

  const handleLogout = async () => {
    const client = getClient();
    if (client) {
      try {
        await client.logout();
      } catch (e) {
        console.error('Logout error:', e);
      }
    }
    setIsAuthenticated(false);
    setCoopId('');
    setUserDid('');
  };

  if (!isReady) {
    return (
      <View style={styles.loading}>
        <ActivityIndicator size="large" color="#4A90A4" />
        <Text style={styles.loadingText}>Loading...</Text>
      </View>
    );
  }

  return (
    <NavigationContainer>
      <StatusBar style="auto" />
      <Stack.Navigator
        screenOptions={{
          headerStyle: { backgroundColor: '#4A90A4' },
          headerTintColor: '#fff',
          headerTitleStyle: { fontWeight: 'bold' },
        }}
      >
        {!isAuthenticated ? (
          <Stack.Screen name="Login" options={{ headerShown: false }}>
            {() => <LoginScreen onLogin={handleLogin} />}
          </Stack.Screen>
        ) : (
          <>
            <Stack.Screen name="Home" options={{ title: 'Coop Wallet' }}>
              {({ navigation }) => (
                <HomeScreen
                  navigation={navigation}
                  coopId={coopId}
                  userDid={userDid}
                  onLogout={handleLogout}
                />
              )}
            </Stack.Screen>
            <Stack.Screen name="Payment" component={PaymentScreen} options={{ title: 'Send Payment' }} />
            <Stack.Screen name="Scan" component={ScanScreen} options={{ title: 'Scan QR Code' }} />
            <Stack.Screen name="Receive" component={ReceiveScreen} options={{ title: 'Receive Payment' }} />
            <Stack.Screen name="Governance" component={GovernanceScreen} options={{ title: 'Governance' }} />
            <Stack.Screen name="Identity" component={IdentityScreen} options={{ title: 'My Identity' }} />
            <Stack.Screen name="Verify" component={VerifyScreen} options={{ title: 'Verify Identity' }} />
            <Stack.Screen name="VerificationHistory" component={VerificationHistoryScreen} options={{ title: 'History' }} />
            <Stack.Screen name="Transactions" component={TransactionsScreen} options={{ title: 'Transactions' }} />
            <Stack.Screen name="Settings" options={{ title: 'Settings' }}>
              {() => <SettingsScreen onLogout={handleLogout} />}
            </Stack.Screen>
          </>
        )}
      </Stack.Navigator>
    </NavigationContainer>
  );
}

// ============================================================================
// Styles
// ============================================================================

const styles = StyleSheet.create({
  // Loading
  loading: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    backgroundColor: '#f5f5f5',
  },
  loadingText: {
    marginTop: 10,
    color: '#666',
  },

  // Login
  loginContainer: {
    flex: 1,
    backgroundColor: '#4A90A4',
  },
  loginContent: {
    flex: 1,
    justifyContent: 'center',
    padding: 24,
  },
  loginTitle: {
    fontSize: 36,
    fontWeight: 'bold',
    color: '#fff',
    textAlign: 'center',
    marginBottom: 8,
  },
  loginSubtitle: {
    fontSize: 16,
    color: 'rgba(255,255,255,0.8)',
    textAlign: 'center',
    marginBottom: 48,
  },
  loginForm: {
    backgroundColor: '#fff',
    borderRadius: 16,
    padding: 24,
  },
  label: {
    fontSize: 14,
    fontWeight: '600',
    color: '#333',
    marginBottom: 8,
  },
  input: {
    backgroundColor: '#f5f5f5',
    borderRadius: 8,
    padding: 16,
    fontSize: 16,
    marginBottom: 16,
  },
  primaryButton: {
    backgroundColor: '#4A90A4',
    borderRadius: 8,
    padding: 16,
    alignItems: 'center',
  },
  buttonDisabled: {
    opacity: 0.7,
  },
  buttonText: {
    color: '#fff',
    fontSize: 18,
    fontWeight: '600',
  },
  errorText: {
    color: '#e53935',
    fontSize: 14,
    marginBottom: 16,
    textAlign: 'center',
  },
  footerText: {
    color: 'rgba(255,255,255,0.7)',
    fontSize: 12,
    textAlign: 'center',
    marginTop: 24,
  },

  // Home
  container: {
    flex: 1,
    backgroundColor: '#f5f5f5',
  },
  balanceCard: {
    backgroundColor: '#4A90A4',
    margin: 16,
    padding: 24,
    borderRadius: 16,
    alignItems: 'center',
  },
  balanceLabel: {
    color: 'rgba(255,255,255,0.8)',
    fontSize: 14,
  },
  balanceAmount: {
    color: '#fff',
    fontSize: 48,
    fontWeight: 'bold',
    marginVertical: 8,
  },
  balanceCurrency: {
    fontSize: 24,
    fontWeight: 'normal',
  },
  coopName: {
    color: 'rgba(255,255,255,0.9)',
    fontSize: 16,
    marginTop: 8,
  },
  didText: {
    color: 'rgba(255,255,255,0.6)',
    fontSize: 12,
    marginTop: 4,
  },

  // Actions Grid
  actionsGrid: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    padding: 8,
    justifyContent: 'center',
  },
  actionButton: {
    width: '45%',
    backgroundColor: '#fff',
    margin: 8,
    padding: 20,
    borderRadius: 12,
    alignItems: 'center',
  },
  actionIcon: {
    fontSize: 32,
    marginBottom: 8,
  },
  actionLabel: {
    fontSize: 14,
    fontWeight: '600',
    color: '#333',
  },

  // Sections
  section: {
    margin: 16,
  },
  sectionTitle: {
    fontSize: 18,
    fontWeight: 'bold',
    color: '#333',
    marginBottom: 12,
  },
  menuItem: {
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: '#fff',
    padding: 16,
    borderRadius: 12,
    marginBottom: 8,
  },
  menuIcon: {
    fontSize: 24,
    marginRight: 16,
  },
  menuContent: {
    flex: 1,
  },
  menuLabel: {
    fontSize: 16,
    fontWeight: '600',
    color: '#333',
  },
  menuDescription: {
    fontSize: 12,
    color: '#666',
    marginTop: 2,
  },
  menuArrow: {
    fontSize: 24,
    color: '#ccc',
  },

  // Logout
  logoutButton: {
    margin: 16,
    padding: 16,
    alignItems: 'center',
  },
  logoutText: {
    color: '#e53935',
    fontSize: 16,
  },

  // Form styles
  formContainer: {
    flex: 1,
    backgroundColor: '#f5f5f5',
  },
  formScroll: {
    padding: 16,
  },
  formSection: {
    marginBottom: 20,
  },
  amountInput: {
    backgroundColor: '#fff',
    borderRadius: 12,
    padding: 20,
    fontSize: 32,
    textAlign: 'center',
    fontWeight: 'bold',
  },
  memoInput: {
    height: 100,
    textAlignVertical: 'top',
  },
  sendButton: {
    marginTop: 20,
  },

  // Success screen
  successContainer: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    backgroundColor: '#f5f5f5',
    padding: 24,
  },
  successIcon: {
    fontSize: 80,
    marginBottom: 20,
  },
  successTitle: {
    fontSize: 28,
    fontWeight: 'bold',
    color: '#333',
    marginBottom: 16,
  },
  successAmount: {
    fontSize: 36,
    fontWeight: 'bold',
    color: '#4A90A4',
  },
  successRecipient: {
    fontSize: 18,
    color: '#666',
    marginTop: 8,
  },
  successMemo: {
    fontSize: 16,
    color: '#999',
    fontStyle: 'italic',
    marginTop: 12,
  },

  // Receive screen
  receiveContainer: {
    flex: 1,
    backgroundColor: '#f5f5f5',
    padding: 16,
  },
  qrCard: {
    backgroundColor: '#fff',
    borderRadius: 16,
    padding: 24,
    alignItems: 'center',
  },
  qrTitle: {
    fontSize: 20,
    fontWeight: 'bold',
    color: '#333',
    marginBottom: 20,
  },
  qrPlaceholder: {
    width: 200,
    height: 200,
    backgroundColor: '#f0f0f0',
    borderRadius: 12,
    justifyContent: 'center',
    alignItems: 'center',
    marginBottom: 20,
  },
  qrPlaceholderText: {
    fontSize: 64,
  },
  qrPlaceholderLabel: {
    fontSize: 14,
    color: '#666',
    marginTop: 8,
  },
  qrData: {
    fontSize: 10,
    color: '#999',
    marginTop: 4,
  },
  didDisplay: {
    marginTop: 16,
    padding: 12,
    backgroundColor: '#f5f5f5',
    borderRadius: 8,
    width: '100%',
  },
  didLabel: {
    fontSize: 12,
    color: '#666',
    marginBottom: 4,
  },
  didValue: {
    fontSize: 12,
    color: '#333',
    fontFamily: Platform.OS === 'ios' ? 'Menlo' : 'monospace',
  },
  copyButton: {
    backgroundColor: '#fff',
    padding: 16,
    borderRadius: 12,
    marginTop: 16,
    alignItems: 'center',
  },
  copyButtonText: {
    fontSize: 16,
    color: '#4A90A4',
    fontWeight: '600',
  },
  shareButton: {
    backgroundColor: '#4A90A4',
    padding: 16,
    borderRadius: 12,
    marginTop: 12,
    alignItems: 'center',
  },
  shareButtonText: {
    fontSize: 16,
    color: '#fff',
    fontWeight: '600',
  },

  // Scan screen
  scanContainer: {
    flex: 1,
    backgroundColor: '#f5f5f5',
  },
  cameraPlaceholder: {
    flex: 1,
    backgroundColor: '#1a1a2e',
    justifyContent: 'center',
    alignItems: 'center',
    minHeight: 300,
  },
  cameraIcon: {
    fontSize: 64,
    marginBottom: 16,
  },
  cameraText: {
    fontSize: 18,
    color: '#fff',
  },
  manualEntrySection: {
    padding: 16,
  },

  // Governance screen
  governanceContainer: {
    flex: 1,
    backgroundColor: '#f5f5f5',
  },
  govStats: {
    flexDirection: 'row',
    backgroundColor: '#4A90A4',
    padding: 20,
    justifyContent: 'space-around',
  },
  govStat: {
    alignItems: 'center',
  },
  govStatNumber: {
    fontSize: 28,
    fontWeight: 'bold',
    color: '#fff',
  },
  govStatLabel: {
    fontSize: 12,
    color: 'rgba(255,255,255,0.8)',
    marginTop: 4,
  },
  proposalCard: {
    backgroundColor: '#fff',
    margin: 16,
    marginBottom: 8,
    padding: 16,
    borderRadius: 12,
  },
  proposalHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'flex-start',
    marginBottom: 8,
  },
  proposalTitle: {
    fontSize: 16,
    fontWeight: 'bold',
    color: '#333',
    flex: 1,
    marginRight: 8,
  },
  statusBadge: {
    paddingHorizontal: 8,
    paddingVertical: 4,
    borderRadius: 12,
  },
  statusActive: {
    backgroundColor: '#e3f2fd',
  },
  statusPassed: {
    backgroundColor: '#e8f5e9',
  },
  statusText: {
    fontSize: 12,
  },
  proposalDescription: {
    fontSize: 14,
    color: '#666',
    marginBottom: 12,
  },
  voteBar: {
    flexDirection: 'row',
    height: 8,
    borderRadius: 4,
    overflow: 'hidden',
    marginBottom: 8,
  },
  voteFor: {
    backgroundColor: '#4caf50',
  },
  voteAgainst: {
    backgroundColor: '#f44336',
  },
  voteStats: {
    flexDirection: 'row',
    justifyContent: 'space-between',
  },
  voteText: {
    fontSize: 14,
    color: '#666',
  },
  endDate: {
    fontSize: 12,
    color: '#999',
  },
  voteButtons: {
    flexDirection: 'row',
    marginTop: 12,
    gap: 8,
  },
  voteForButton: {
    flex: 1,
    backgroundColor: '#4caf50',
    padding: 12,
    borderRadius: 8,
    alignItems: 'center',
  },
  voteAgainstButton: {
    flex: 1,
    backgroundColor: '#f44336',
    padding: 12,
    borderRadius: 8,
    alignItems: 'center',
  },
  voteButtonText: {
    color: '#fff',
    fontWeight: '600',
  },

  // Identity screen
  identityContainer: {
    flex: 1,
    backgroundColor: '#f5f5f5',
  },
  identityCard: {
    backgroundColor: '#4A90A4',
    padding: 24,
    alignItems: 'center',
  },
  identityAvatar: {
    width: 80,
    height: 80,
    borderRadius: 40,
    backgroundColor: 'rgba(255,255,255,0.2)',
    justifyContent: 'center',
    alignItems: 'center',
    marginBottom: 16,
  },
  avatarText: {
    fontSize: 40,
  },
  identityName: {
    fontSize: 24,
    fontWeight: 'bold',
    color: '#fff',
    marginBottom: 8,
  },
  identityDid: {
    fontSize: 12,
    color: 'rgba(255,255,255,0.8)',
    fontFamily: Platform.OS === 'ios' ? 'Menlo' : 'monospace',
  },
  identityStats: {
    flexDirection: 'row',
    marginTop: 20,
    justifyContent: 'space-around',
    width: '100%',
  },
  identityStat: {
    alignItems: 'center',
  },
  identityStatValue: {
    fontSize: 24,
    fontWeight: 'bold',
    color: '#fff',
  },
  identityStatLabel: {
    fontSize: 12,
    color: 'rgba(255,255,255,0.8)',
    marginTop: 4,
  },
  proofSection: {
    padding: 16,
  },
  sectionSubtitle: {
    fontSize: 14,
    color: '#666',
    marginBottom: 16,
  },
  proofTypeOption: {
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: '#fff',
    padding: 16,
    borderRadius: 12,
    marginBottom: 8,
  },
  proofTypeSelected: {
    borderWidth: 2,
    borderColor: '#4A90A4',
  },
  proofTypeIcon: {
    fontSize: 32,
    marginRight: 16,
  },
  proofTypeContent: {
    flex: 1,
  },
  proofTypeLabel: {
    fontSize: 16,
    fontWeight: '600',
    color: '#333',
  },
  proofTypeDesc: {
    fontSize: 12,
    color: '#666',
    marginTop: 2,
  },
  checkmark: {
    fontSize: 24,
    color: '#4A90A4',
    fontWeight: 'bold',
  },
  generateButton: {
    backgroundColor: '#4A90A4',
    padding: 16,
    borderRadius: 12,
    alignItems: 'center',
    marginTop: 16,
  },
  generateButtonText: {
    color: '#fff',
    fontSize: 18,
    fontWeight: '600',
  },
  credentialsSection: {
    padding: 16,
  },
  credentialItem: {
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: '#fff',
    padding: 16,
    borderRadius: 12,
    marginBottom: 8,
  },
  credentialIcon: {
    fontSize: 24,
    marginRight: 16,
  },
  credentialContent: {
    flex: 1,
  },
  credentialLabel: {
    fontSize: 14,
    fontWeight: '600',
    color: '#333',
  },
  credentialValue: {
    fontSize: 12,
    color: '#666',
    marginTop: 2,
  },
  credentialStatus: {
    fontSize: 20,
  },

  // Verify screen
  verifyContainer: {
    flex: 1,
    backgroundColor: '#f5f5f5',
  },
  verifyInstructions: {
    padding: 16,
  },
  instructionTitle: {
    fontSize: 18,
    fontWeight: 'bold',
    color: '#333',
    marginBottom: 12,
  },
  instructionText: {
    fontSize: 14,
    color: '#666',
    lineHeight: 24,
  },
  verifyResultContainer: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    backgroundColor: '#f5f5f5',
    padding: 24,
  },
  verifyResultCard: {
    backgroundColor: '#fff',
    borderRadius: 16,
    padding: 32,
    alignItems: 'center',
    width: '100%',
    marginBottom: 24,
  },
  verifyValid: {
    borderWidth: 3,
    borderColor: '#4caf50',
  },
  verifyInvalid: {
    borderWidth: 3,
    borderColor: '#f44336',
  },
  verifyResultIcon: {
    fontSize: 64,
    marginBottom: 16,
  },
  verifyResultTitle: {
    fontSize: 24,
    fontWeight: 'bold',
    color: '#333',
    marginBottom: 16,
  },
  verifyResultName: {
    fontSize: 20,
    fontWeight: '600',
    color: '#333',
  },
  verifyResultDid: {
    fontSize: 12,
    color: '#666',
    marginTop: 4,
    fontFamily: Platform.OS === 'ios' ? 'Menlo' : 'monospace',
  },
  verifyResultDetails: {
    flexDirection: 'row',
    marginTop: 24,
    justifyContent: 'space-around',
    width: '100%',
  },
  verifyResultDetail: {
    alignItems: 'center',
  },
  verifyDetailLabel: {
    fontSize: 12,
    color: '#666',
  },
  verifyDetailValue: {
    fontSize: 18,
    fontWeight: '600',
    color: '#333',
    marginTop: 4,
  },

  // History screen
  historyContainer: {
    flex: 1,
    backgroundColor: '#f5f5f5',
  },
  historyStats: {
    flexDirection: 'row',
    backgroundColor: '#4A90A4',
    padding: 20,
    justifyContent: 'space-around',
  },
  historyStat: {
    alignItems: 'center',
  },
  historyStatNumber: {
    fontSize: 28,
    fontWeight: 'bold',
    color: '#fff',
  },
  historyStatLabel: {
    fontSize: 12,
    color: 'rgba(255,255,255,0.8)',
    marginTop: 4,
  },
  historyItem: {
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: '#fff',
    margin: 16,
    marginBottom: 8,
    padding: 16,
    borderRadius: 12,
  },
  historyIcon: {
    width: 40,
    height: 40,
    borderRadius: 20,
    backgroundColor: '#f0f0f0',
    justifyContent: 'center',
    alignItems: 'center',
    marginRight: 12,
  },
  historyIconText: {
    fontSize: 18,
    color: '#666',
  },
  historyContent: {
    flex: 1,
  },
  historyName: {
    fontSize: 16,
    fontWeight: '600',
    color: '#333',
  },
  historyDid: {
    fontSize: 12,
    color: '#999',
    marginTop: 2,
  },
  historyMeta: {
    fontSize: 12,
    color: '#666',
    marginTop: 4,
  },
  historyStatus: {
    fontSize: 20,
  },
  emptyState: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
    padding: 48,
  },
  emptyIcon: {
    fontSize: 64,
    marginBottom: 16,
  },
  emptyText: {
    fontSize: 16,
    color: '#666',
  },

  // Transaction styles (Home)
  sectionHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 12,
  },
  seeAllLink: {
    fontSize: 14,
    color: '#4A90A4',
    fontWeight: '600',
  },
  transactionItem: {
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: '#fff',
    padding: 12,
    borderRadius: 12,
    marginBottom: 8,
  },
  txIcon: {
    width: 40,
    height: 40,
    borderRadius: 20,
    justifyContent: 'center',
    alignItems: 'center',
    marginRight: 12,
  },
  txIconReceived: {
    backgroundColor: '#e8f5e9',
  },
  txIconSent: {
    backgroundColor: '#ffebee',
  },
  txIconText: {
    fontSize: 18,
    fontWeight: 'bold',
  },
  txContent: {
    flex: 1,
  },
  txPerson: {
    fontSize: 14,
    fontWeight: '600',
    color: '#333',
  },
  txMemo: {
    fontSize: 12,
    color: '#666',
    marginTop: 2,
  },
  txAmountContainer: {
    alignItems: 'flex-end',
  },
  txAmount: {
    fontSize: 16,
    fontWeight: 'bold',
  },
  txAmountReceived: {
    color: '#4caf50',
  },
  txAmountSent: {
    color: '#f44336',
  },
  txDate: {
    fontSize: 10,
    color: '#999',
    marginTop: 2,
  },

  // QR Code styles
  qrWrapper: {
    marginBottom: 20,
    alignItems: 'center',
  },
  qrFallback: {
    alignItems: 'center',
    marginBottom: 8,
  },
  qrFallbackText: {
    fontSize: 24,
    letterSpacing: 4,
  },
  requestAmount: {
    backgroundColor: '#e3f2fd',
    paddingHorizontal: 16,
    paddingVertical: 8,
    borderRadius: 20,
    marginBottom: 16,
  },
  requestAmountLabel: {
    fontSize: 12,
    color: '#666',
    textAlign: 'center',
  },
  requestAmountValue: {
    fontSize: 18,
    fontWeight: 'bold',
    color: '#4A90A4',
    textAlign: 'center',
  },
  copyButtonSuccess: {
    backgroundColor: '#4caf50',
  },
  copyButtonTextSuccess: {
    color: '#fff',
  },

  // Voted badge
  votedBadge: {
    backgroundColor: '#e3f2fd',
    padding: 12,
    borderRadius: 8,
    marginTop: 12,
    alignItems: 'center',
  },
  votedText: {
    fontSize: 14,
    color: '#4A90A4',
    fontWeight: '600',
  },

  // Transactions screen
  transactionsContainer: {
    flex: 1,
    backgroundColor: '#f5f5f5',
  },
  txSummary: {
    flexDirection: 'row',
    backgroundColor: '#4A90A4',
    padding: 20,
    justifyContent: 'space-around',
  },
  txSummaryItem: {
    alignItems: 'center',
  },
  txSummaryLabel: {
    fontSize: 12,
    color: 'rgba(255,255,255,0.8)',
  },
  txSummaryValue: {
    fontSize: 24,
    fontWeight: 'bold',
    marginTop: 4,
  },
  filterRow: {
    flexDirection: 'row',
    padding: 16,
    gap: 8,
  },
  filterButton: {
    flex: 1,
    padding: 12,
    borderRadius: 8,
    backgroundColor: '#fff',
    alignItems: 'center',
  },
  filterButtonActive: {
    backgroundColor: '#4A90A4',
  },
  filterText: {
    fontSize: 14,
    color: '#666',
    fontWeight: '600',
  },
  filterTextActive: {
    color: '#fff',
  },

  // Settings screen
  settingsContainer: {
    flex: 1,
    backgroundColor: '#f5f5f5',
  },
  settingsSection: {
    marginTop: 24,
  },
  settingsSectionTitle: {
    fontSize: 14,
    fontWeight: '600',
    color: '#666',
    marginLeft: 16,
    marginBottom: 8,
    textTransform: 'uppercase',
  },
  settingsItem: {
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: '#fff',
    padding: 16,
    borderBottomWidth: 1,
    borderBottomColor: '#f0f0f0',
  },
  settingsItemContent: {
    flex: 1,
  },
  settingsItemLabel: {
    fontSize: 16,
    color: '#333',
  },
  settingsItemValue: {
    fontSize: 12,
    color: '#666',
    marginTop: 2,
  },
  settingsItemDescription: {
    fontSize: 12,
    color: '#999',
    marginTop: 2,
  },
  settingsAction: {
    fontSize: 14,
    color: '#4A90A4',
    fontWeight: '600',
  },
  settingsArrow: {
    fontSize: 24,
    color: '#ccc',
  },
  settingsValue: {
    fontSize: 14,
    color: '#666',
  },
  toggle: {
    width: 50,
    height: 30,
    borderRadius: 15,
    backgroundColor: '#ccc',
    padding: 2,
    justifyContent: 'center',
  },
  toggleOn: {
    backgroundColor: '#4A90A4',
  },
  toggleThumb: {
    width: 26,
    height: 26,
    borderRadius: 13,
    backgroundColor: '#fff',
  },
  toggleThumbOn: {
    alignSelf: 'flex-end',
  },
  dangerButton: {
    backgroundColor: '#f44336',
    margin: 16,
    padding: 16,
    borderRadius: 12,
    alignItems: 'center',
  },
  dangerButtonText: {
    color: '#fff',
    fontSize: 16,
    fontWeight: '600',
  },
  dangerButtonOutline: {
    borderWidth: 1,
    borderColor: '#f44336',
    marginHorizontal: 16,
    padding: 16,
    borderRadius: 12,
    alignItems: 'center',
  },
  dangerButtonOutlineText: {
    color: '#f44336',
    fontSize: 16,
    fontWeight: '600',
  },
  settingsFooter: {
    fontSize: 12,
    color: '#999',
    textAlign: 'center',
    marginTop: 32,
    marginBottom: 48,
    lineHeight: 20,
  },
});
