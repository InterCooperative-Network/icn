import React, { useState, useEffect } from 'react';
import {
  View,
  Text,
  StyleSheet,
  FlatList,
  TouchableOpacity,
  TextInput,
  Modal,
  ScrollView,
  Alert,
} from 'react-native';
import { ICNClient } from '@icn/sdk';

interface Cooperative {
  id: string;
  name: string;
  description: string;
  memberCount: number;
  status: 'Active' | 'Forming' | 'Dissolved';
  myRole: 'Member' | 'Founder' | 'Admin' | null;
  joinedAt?: string;
}

interface Member {
  did: string;
  name: string;
  role: 'Member' | 'Founder' | 'Admin';
  joinedAt: string;
}

export const CooperativeManager: React.FC = () => {
  const [client] = useState(() => new ICNClient({ url: process.env.ICN_API_URL || 'http://localhost:8080' }));
  const [cooperatives, setCooperatives] = useState<Cooperative[]>([]);
  const [selectedCoop, setSelectedCoop] = useState<Cooperative | null>(null);
  const [members, setMembers] = useState<Member[]>([]);
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [showDetailsModal, setShowDetailsModal] = useState(false);
  const [loading, setLoading] = useState(false);

  // Create form state
  const [newCoopName, setNewCoopName] = useState('');
  const [newCoopDescription, setNewCoopDescription] = useState('');

  useEffect(() => {
    loadCooperatives();
  }, []);

  const loadCooperatives = async () => {
    setLoading(true);
    try {
      const coops = await client.listCoops();
      setCooperatives(coops);
    } catch (error) {
      Alert.alert('Error', 'Failed to load cooperatives');
      console.error(error);
    } finally {
      setLoading(false);
    }
  };

  const loadCoopDetails = async (coopId: string) => {
    setLoading(true);
    try {
      const details = await client.getCoop(coopId);
      const memberList = await client.listMembers(coopId);
      setSelectedCoop(details);
      setMembers(memberList);
      setShowDetailsModal(true);
    } catch (error) {
      Alert.alert('Error', 'Failed to load cooperative details');
      console.error(error);
    } finally {
      setLoading(false);
    }
  };

  const createCooperative = async () => {
    if (!newCoopName.trim()) {
      Alert.alert('Error', 'Cooperative name is required');
      return;
    }

    setLoading(true);
    try {
      await client.createCoop({
        name: newCoopName,
        description: newCoopDescription,
      });
      Alert.alert('Success', 'Cooperative created successfully');
      setShowCreateModal(false);
      setNewCoopName('');
      setNewCoopDescription('');
      loadCooperatives();
    } catch (error) {
      Alert.alert('Error', 'Failed to create cooperative');
      console.error(error);
    } finally {
      setLoading(false);
    }
  };

  const joinCooperative = async (coopId: string) => {
    Alert.alert(
      'Join Cooperative',
      'Do you want to join this cooperative?',
      [
        { text: 'Cancel', style: 'cancel' },
        {
          text: 'Join',
          onPress: async () => {
            setLoading(true);
            try {
              // Join by adding self as member
              const myDid = 'did:icn:self'; // TODO: Get from auth context
              await client.addMember(coopId, { did: myDid, role: 'member' });
              Alert.alert('Success', 'Successfully joined cooperative');
              loadCooperatives();
            } catch (error) {
              Alert.alert('Error', 'Failed to join cooperative');
              console.error(error);
            } finally {
              setLoading(false);
            }
          },
        },
      ]
    );
  };

  const leaveCooperative = async (coopId: string) => {
    Alert.alert(
      'Leave Cooperative',
      'Are you sure you want to leave this cooperative?',
      [
        { text: 'Cancel', style: 'cancel' },
        {
          text: 'Leave',
          style: 'destructive',
          onPress: async () => {
            setLoading(true);
            try {
              const myDid = 'did:icn:self'; // TODO: Get from auth context
              await client.removeMember(coopId, myDid);
              Alert.alert('Success', 'Successfully left cooperative');
              setShowDetailsModal(false);
              loadCooperatives();
            } catch (error) {
              Alert.alert('Error', 'Failed to leave cooperative');
              console.error(error);
            } finally {
              setLoading(false);
            }
          },
        },
      ]
    );
  };

  const renderCooperative = ({ item }: { item: Cooperative }) => (
    <TouchableOpacity
      style={styles.coopCard}
      onPress={() => loadCoopDetails(item.id)}
    >
      <View style={styles.coopHeader}>
        <Text style={styles.coopName}>{item.name}</Text>
        <View style={[styles.statusBadge, styles[`status${item.status}`]]}>
          <Text style={styles.statusText}>{item.status}</Text>
        </View>
      </View>
      <Text style={styles.coopDescription} numberOfLines={2}>
        {item.description}
      </Text>
      <View style={styles.coopFooter}>
        <Text style={styles.memberCount}>👥 {item.memberCount} members</Text>
        {item.myRole && (
          <Text style={styles.roleText}>Your role: {item.myRole}</Text>
        )}
      </View>
    </TouchableOpacity>
  );

  const renderMember = ({ item }: { item: Member }) => (
    <View style={styles.memberCard}>
      <View>
        <Text style={styles.memberName}>{item.name || item.did.slice(0, 20)}</Text>
        <Text style={styles.memberDid}>{item.did}</Text>
        <Text style={styles.memberJoined}>Joined {new Date(item.joinedAt).toLocaleDateString()}</Text>
      </View>
      <View style={[styles.roleBadge, styles[`role${item.role}`]]}>
        <Text style={styles.roleText}>{item.role}</Text>
      </View>
    </View>
  );

  return (
    <View style={styles.container}>
      <View style={styles.header}>
        <Text style={styles.title}>Cooperatives</Text>
        <TouchableOpacity
          style={styles.createButton}
          onPress={() => setShowCreateModal(true)}
        >
          <Text style={styles.createButtonText}>+ Create</Text>
        </TouchableOpacity>
      </View>

      <FlatList
        data={cooperatives}
        renderItem={renderCooperative}
        keyExtractor={(item) => item.id}
        contentContainerStyle={styles.list}
        refreshing={loading}
        onRefresh={loadCooperatives}
        ListEmptyComponent={
          <View style={styles.emptyState}>
            <Text style={styles.emptyText}>No cooperatives yet</Text>
            <Text style={styles.emptySubtext}>Create one to get started</Text>
          </View>
        }
      />

      {/* Create Modal */}
      <Modal
        visible={showCreateModal}
        animationType="slide"
        transparent={true}
        onRequestClose={() => setShowCreateModal(false)}
      >
        <View style={styles.modalOverlay}>
          <View style={styles.modalContent}>
            <Text style={styles.modalTitle}>Create Cooperative</Text>

            <TextInput
              style={styles.input}
              placeholder="Cooperative Name"
              value={newCoopName}
              onChangeText={setNewCoopName}
            />

            <TextInput
              style={[styles.input, styles.textArea]}
              placeholder="Description"
              value={newCoopDescription}
              onChangeText={setNewCoopDescription}
              multiline
              numberOfLines={4}
            />

            <View style={styles.modalButtons}>
              <TouchableOpacity
                style={[styles.button, styles.cancelButton]}
                onPress={() => setShowCreateModal(false)}
              >
                <Text style={styles.cancelButtonText}>Cancel</Text>
              </TouchableOpacity>
              <TouchableOpacity
                style={[styles.button, styles.primaryButton]}
                onPress={createCooperative}
                disabled={loading}
              >
                <Text style={styles.primaryButtonText}>
                  {loading ? 'Creating...' : 'Create'}
                </Text>
              </TouchableOpacity>
            </View>
          </View>
        </View>
      </Modal>

      {/* Details Modal */}
      <Modal
        visible={showDetailsModal}
        animationType="slide"
        onRequestClose={() => setShowDetailsModal(false)}
      >
        <View style={styles.detailsContainer}>
          <View style={styles.detailsHeader}>
            <TouchableOpacity onPress={() => setShowDetailsModal(false)}>
              <Text style={styles.backButton}>← Back</Text>
            </TouchableOpacity>
            <Text style={styles.detailsTitle}>{selectedCoop?.name}</Text>
          </View>

          <ScrollView style={styles.detailsContent}>
            <View style={styles.detailsSection}>
              <Text style={styles.sectionTitle}>Description</Text>
              <Text style={styles.sectionText}>{selectedCoop?.description}</Text>
            </View>

            <View style={styles.detailsSection}>
              <Text style={styles.sectionTitle}>Status</Text>
              <View style={[styles.statusBadge, styles[`status${selectedCoop?.status}`]]}>
                <Text style={styles.statusText}>{selectedCoop?.status}</Text>
              </View>
            </View>

            <View style={styles.detailsSection}>
              <Text style={styles.sectionTitle}>Members ({members.length})</Text>
              {members.map((member) => (
                <View key={member.did}>
                  {renderMember({ item: member })}
                </View>
              ))}
            </View>

            {selectedCoop?.myRole ? (
              <TouchableOpacity
                style={[styles.button, styles.dangerButton]}
                onPress={() => leaveCooperative(selectedCoop.id)}
              >
                <Text style={styles.dangerButtonText}>Leave Cooperative</Text>
              </TouchableOpacity>
            ) : (
              <TouchableOpacity
                style={[styles.button, styles.primaryButton]}
                onPress={() => joinCooperative(selectedCoop!.id)}
              >
                <Text style={styles.primaryButtonText}>Join Cooperative</Text>
              </TouchableOpacity>
            )}
          </ScrollView>
        </View>
      </Modal>
    </View>
  );
};

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
  title: {
    fontSize: 24,
    fontWeight: 'bold',
  },
  createButton: {
    backgroundColor: '#007AFF',
    paddingHorizontal: 16,
    paddingVertical: 8,
    borderRadius: 8,
  },
  createButtonText: {
    color: '#fff',
    fontWeight: '600',
  },
  list: {
    padding: 16,
  },
  coopCard: {
    backgroundColor: '#fff',
    padding: 16,
    borderRadius: 12,
    marginBottom: 12,
    shadowColor: '#000',
    shadowOffset: { width: 0, height: 2 },
    shadowOpacity: 0.1,
    shadowRadius: 4,
    elevation: 3,
  },
  coopHeader: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    marginBottom: 8,
  },
  coopName: {
    fontSize: 18,
    fontWeight: '600',
    flex: 1,
  },
  coopDescription: {
    fontSize: 14,
    color: '#666',
    marginBottom: 12,
  },
  coopFooter: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  memberCount: {
    fontSize: 14,
    color: '#666',
  },
  statusBadge: {
    paddingHorizontal: 8,
    paddingVertical: 4,
    borderRadius: 4,
  },
  statusActive: {
    backgroundColor: '#4CAF50',
  },
  statusForming: {
    backgroundColor: '#FFC107',
  },
  statusDissolved: {
    backgroundColor: '#9E9E9E',
  },
  statusText: {
    color: '#fff',
    fontSize: 12,
    fontWeight: '600',
  },
  roleText: {
    fontSize: 12,
    color: '#007AFF',
    fontWeight: '500',
  },
  emptyState: {
    alignItems: 'center',
    justifyContent: 'center',
    paddingVertical: 48,
  },
  emptyText: {
    fontSize: 18,
    fontWeight: '600',
    color: '#666',
    marginBottom: 8,
  },
  emptySubtext: {
    fontSize: 14,
    color: '#999',
  },
  modalOverlay: {
    flex: 1,
    backgroundColor: 'rgba(0,0,0,0.5)',
    justifyContent: 'center',
    alignItems: 'center',
  },
  modalContent: {
    backgroundColor: '#fff',
    borderRadius: 12,
    padding: 24,
    width: '90%',
    maxWidth: 400,
  },
  modalTitle: {
    fontSize: 20,
    fontWeight: 'bold',
    marginBottom: 16,
  },
  input: {
    borderWidth: 1,
    borderColor: '#ddd',
    borderRadius: 8,
    padding: 12,
    marginBottom: 12,
    fontSize: 16,
  },
  textArea: {
    height: 100,
    textAlignVertical: 'top',
  },
  modalButtons: {
    flexDirection: 'row',
    justifyContent: 'flex-end',
    gap: 12,
    marginTop: 8,
  },
  button: {
    paddingHorizontal: 20,
    paddingVertical: 12,
    borderRadius: 8,
  },
  cancelButton: {
    backgroundColor: '#f0f0f0',
  },
  cancelButtonText: {
    color: '#333',
    fontWeight: '600',
  },
  primaryButton: {
    backgroundColor: '#007AFF',
  },
  primaryButtonText: {
    color: '#fff',
    fontWeight: '600',
  },
  dangerButton: {
    backgroundColor: '#FF3B30',
    marginTop: 16,
  },
  dangerButtonText: {
    color: '#fff',
    fontWeight: '600',
    textAlign: 'center',
  },
  detailsContainer: {
    flex: 1,
    backgroundColor: '#f5f5f5',
  },
  detailsHeader: {
    flexDirection: 'row',
    alignItems: 'center',
    padding: 16,
    backgroundColor: '#fff',
    borderBottomWidth: 1,
    borderBottomColor: '#e0e0e0',
  },
  backButton: {
    fontSize: 16,
    color: '#007AFF',
    marginRight: 16,
  },
  detailsTitle: {
    fontSize: 20,
    fontWeight: 'bold',
    flex: 1,
  },
  detailsContent: {
    flex: 1,
    padding: 16,
  },
  detailsSection: {
    backgroundColor: '#fff',
    padding: 16,
    borderRadius: 12,
    marginBottom: 16,
  },
  sectionTitle: {
    fontSize: 16,
    fontWeight: '600',
    marginBottom: 8,
  },
  sectionText: {
    fontSize: 14,
    color: '#666',
  },
  memberCard: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
    padding: 12,
    backgroundColor: '#f9f9f9',
    borderRadius: 8,
    marginTop: 8,
  },
  memberName: {
    fontSize: 14,
    fontWeight: '600',
  },
  memberDid: {
    fontSize: 12,
    color: '#999',
    marginTop: 2,
  },
  memberJoined: {
    fontSize: 12,
    color: '#666',
    marginTop: 2,
  },
  roleBadge: {
    paddingHorizontal: 8,
    paddingVertical: 4,
    borderRadius: 4,
  },
  roleMember: {
    backgroundColor: '#2196F3',
  },
  roleFounder: {
    backgroundColor: '#9C27B0',
  },
  roleAdmin: {
    backgroundColor: '#FF9800',
  },
});
