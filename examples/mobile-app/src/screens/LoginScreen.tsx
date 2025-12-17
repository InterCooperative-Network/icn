import React, { useState, useContext } from 'react';
import { View, StyleSheet, ScrollView, KeyboardAvoidingView, Platform } from 'react-native';
import { TextInput, Button, Text, Card, Title } from 'react-native-paper';
import { AuthContext } from '../contexts/AuthContext';

export default function LoginScreen() {
  const [did, setDid] = useState('');
  const [apiUrl, setApiUrl] = useState('http://localhost:8000');
  const [challenge, setChallenge] = useState('');
  const [signature, setSignature] = useState('');
  const [step, setStep] = useState<'did' | 'sign' | 'verify'>('did');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const { login } = useContext(AuthContext);

  const requestChallenge = async () => {
    setLoading(true);
    setError('');
    
    try {
      const response = await fetch(`${apiUrl}/v1/auth/challenge`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ did }),
      });

      if (!response.ok) throw new Error('Failed to get challenge');

      const data = await response.json();
      setChallenge(data.challenge);
      setStep('sign');
    } catch (err: any) {
      setError(err.message || 'Failed to request challenge');
    } finally {
      setLoading(false);
    }
  };

  const verifySignature = async () => {
    setLoading(true);
    setError('');

    try {
      const response = await fetch(`${apiUrl}/v1/auth/verify`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ did, challenge, signature }),
      });

      if (!response.ok) throw new Error('Invalid signature');

      const data = await response.json();
      await login(data.token, apiUrl);
    } catch (err: any) {
      setError(err.message || 'Authentication failed');
    } finally {
      setLoading(false);
    }
  };

  return (
    <KeyboardAvoidingView 
      behavior={Platform.OS === 'ios' ? 'padding' : 'height'}
      style={styles.container}
    >
      <ScrollView contentContainerStyle={styles.scrollContent}>
        <View style={styles.header}>
          <Text style={styles.logo}>🌐</Text>
          <Title style={styles.title}>ICN Mobile</Title>
          <Text style={styles.subtitle}>Intercooperative Network</Text>
        </View>

        <Card style={styles.card}>
          <Card.Content>
            {step === 'did' && (
              <>
                <Title>Sign In</Title>
                <TextInput
                  label="API URL"
                  value={apiUrl}
                  onChangeText={setApiUrl}
                  mode="outlined"
                  style={styles.input}
                />
                <TextInput
                  label="Your DID"
                  value={did}
                  onChangeText={setDid}
                  mode="outlined"
                  placeholder="did:icn:..."
                  style={styles.input}
                  autoCapitalize="none"
                />
                {error ? <Text style={styles.error}>{error}</Text> : null}
                <Button
                  mode="contained"
                  onPress={requestChallenge}
                  loading={loading}
                  disabled={!did || loading}
                  style={styles.button}
                >
                  Request Challenge
                </Button>
              </>
            )}

            {step === 'sign' && (
              <>
                <Title>Sign Challenge</Title>
                <Text style={styles.instruction}>
                  Copy the challenge below and sign it with your private key:
                </Text>
                <Card style={styles.challengeCard}>
                  <Card.Content>
                    <Text style={styles.challenge}>{challenge}</Text>
                  </Card.Content>
                </Card>
                <Text style={styles.instruction}>
                  Then paste your signature below:
                </Text>
                <TextInput
                  label="Signature"
                  value={signature}
                  onChangeText={setSignature}
                  mode="outlined"
                  multiline
                  numberOfLines={3}
                  style={styles.input}
                />
                {error ? <Text style={styles.error}>{error}</Text> : null}
                <Button
                  mode="contained"
                  onPress={verifySignature}
                  loading={loading}
                  disabled={!signature || loading}
                  style={styles.button}
                >
                  Verify & Sign In
                </Button>
                <Button
                  mode="text"
                  onPress={() => setStep('did')}
                  style={styles.backButton}
                >
                  Back
                </Button>
              </>
            )}
          </Card.Content>
        </Card>

        <Text style={styles.help}>
          Need help? Visit docs.icn.coop
        </Text>
      </ScrollView>
    </KeyboardAvoidingView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#f1f5f9',
  },
  scrollContent: {
    flexGrow: 1,
    justifyContent: 'center',
    padding: 20,
  },
  header: {
    alignItems: 'center',
    marginBottom: 30,
  },
  logo: {
    fontSize: 64,
    marginBottom: 10,
  },
  title: {
    fontSize: 28,
    fontWeight: 'bold',
    color: '#1e293b',
  },
  subtitle: {
    fontSize: 14,
    color: '#64748b',
  },
  card: {
    elevation: 4,
  },
  input: {
    marginBottom: 16,
  },
  button: {
    marginTop: 8,
  },
  backButton: {
    marginTop: 8,
  },
  error: {
    color: '#ef4444',
    marginBottom: 12,
  },
  instruction: {
    marginVertical: 12,
    color: '#64748b',
  },
  challengeCard: {
    backgroundColor: '#f8fafc',
    marginVertical: 12,
  },
  challenge: {
    fontFamily: Platform.OS === 'ios' ? 'Courier' : 'monospace',
    fontSize: 12,
    color: '#475569',
  },
  help: {
    textAlign: 'center',
    marginTop: 20,
    color: '#94a3b8',
    fontSize: 12,
  },
});
