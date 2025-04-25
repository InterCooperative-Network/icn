import React from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { AnchorDAGView } from '../AnchorDAGView';
import '@testing-library/jest-dom';

// Mock the CredentialDAGView component since we're testing integration
jest.mock('../CredentialDAGView', () => {
  return {
    CredentialDAGView: jest.fn(({ credentials, selectedCredentialId, onCredentialSelect }) => (
      <div data-testid="mock-dag-view">
        <div>Credential Count: {credentials.length}</div>
        <div>Selected ID: {selectedCredentialId || 'none'}</div>
        <button 
          onClick={() => onCredentialSelect && onCredentialSelect('test-receipt-1')}
          data-testid="select-button"
        >
          Select Receipt
        </button>
      </div>
    )),
  };
});

// Mock credentials for testing
const mockAnchorCredential = {
  id: 'test-anchor-1',
  title: 'Test Anchor Credential',
  type: ['VerifiableCredential', 'AnchorCredential'],
  issuer: {
    did: 'did:icn:federation:palmyra',
    name: 'Palmyra Federation',
  },
  subjectDid: 'did:icn:test-subject',
  issuanceDate: '2025-04-15T12:00:00Z',
  credentialSubject: {
    id: 'did:icn:test-subject',
    epochId: '2025-Q2',
    dagAnchor: 'bf3a7e21c932d798ef8b5718359783b4f5e2c69ed9ace82d',
    issuanceDate: '2025-04-15T12:00:00Z',
  },
  metadata: {
    federation: {
      id: 'palmyra-fed-123',
      name: 'Palmyra Federation',
    },
    dag: {
      root_hash: 'bf3a7e21c932d798ef8b5718359783b4f5e2c69ed9ace82d',
      timestamp: '2025-04-15T12:00:00Z',
    },
  },
};

const mockReceiptCredential1 = {
  id: 'test-receipt-1',
  title: 'Test Receipt Credential 1',
  type: ['VerifiableCredential', 'ExecutionVerifiableCredential'],
  issuer: {
    did: 'did:icn:runtime:123',
    name: 'ICN Runtime',
  },
  subjectDid: 'did:icn:test-subject',
  issuanceDate: '2025-04-15T14:30:00Z',
  credentialSubject: {
    id: 'did:icn:test-subject',
    proposalId: 'proposal-123',
    dagAnchor: 'bf3a7e21c932d798ef8b5718359783b4f5e2c69ed9ace82d',
  },
};

const mockReceiptCredential2 = {
  id: 'test-receipt-2',
  title: 'Test Receipt Credential 2',
  type: ['VerifiableCredential', 'ExecutionVerifiableCredential'],
  issuer: {
    did: 'did:icn:runtime:123',
    name: 'ICN Runtime',
  },
  subjectDid: 'did:icn:test-subject',
  issuanceDate: '2025-04-15T15:45:00Z',
  credentialSubject: {
    id: 'did:icn:test-subject',
    proposalId: 'proposal-456',
    dagAnchor: 'bf3a7e21c932d798ef8b5718359783b4f5e2c69ed9ace82d',
  },
};

const mockCredentials = [
  mockAnchorCredential,
  mockReceiptCredential1,
  mockReceiptCredential2,
];

describe('AnchorDAGView Component', () => {
  test('renders with anchor selection bar', () => {
    render(<AnchorDAGView credentials={mockCredentials} />);
    
    // Check that the anchor node is displayed in the selection bar
    expect(screen.getByText('Epoch 2025-Q2')).toBeInTheDocument();
    
    // Check that the DAG view is rendered with all credentials
    expect(screen.getByText('Credential Count: 3')).toBeInTheDocument();
  });
  
  test('handles anchor selection', () => {
    render(<AnchorDAGView credentials={mockCredentials} />);
    
    // Click the anchor node
    fireEvent.click(screen.getByText('Epoch 2025-Q2'));
    
    // Expect the relevant credentials to be passed to the DAG view
    // Since we selected the anchor, we should get anchor + 2 receipts = 3 credentials
    expect(screen.getByText('Credential Count: 3')).toBeInTheDocument();
    expect(screen.getByText('Selected ID: test-anchor-1')).toBeInTheDocument();
  });
  
  test('passes selected credential ID to the DAG view', () => {
    const onSelectMock = jest.fn();
    render(
      <AnchorDAGView 
        credentials={mockCredentials} 
        selectedCredentialId="test-receipt-1"
        onCredentialSelect={onSelectMock}
      />
    );
    
    // Check that the selected ID is passed to the DAG view
    expect(screen.getByText('Selected ID: test-receipt-1')).toBeInTheDocument();
    
    // Test that the onCredentialSelect callback is passed through
    fireEvent.click(screen.getByTestId('select-button'));
    expect(onSelectMock).toHaveBeenCalledWith('test-receipt-1');
  });
  
  test('shows message when no anchor credentials are found', () => {
    // Create a test credential set without any anchor credentials
    const nonAnchorCredentials = [mockReceiptCredential1, mockReceiptCredential2];
    
    render(<AnchorDAGView credentials={nonAnchorCredentials} />);
    
    // Check that the 'no anchors' message is displayed
    expect(screen.getByText('No anchor credentials found')).toBeInTheDocument();
  });
}); 