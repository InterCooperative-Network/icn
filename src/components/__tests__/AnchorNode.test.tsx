import React from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { AnchorNode } from '../AnchorNode';
import '@testing-library/jest-dom';

// Mock credential for testing
const mockAnchorCredential = {
  id: 'test-anchor-id',
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
    quorumInfo: {
      threshold: 3,
      signers: ['did:icn:signer1', 'did:icn:signer2', 'did:icn:signer3'],
    },
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

describe('AnchorNode Component', () => {
  test('renders the anchor node with federation and epoch info', () => {
    render(<AnchorNode credential={mockAnchorCredential} />);
    
    // Check for epoch and federation name
    expect(screen.getByText('Epoch 2025-Q2')).toBeInTheDocument();
    expect(screen.getByText('Palmyra Federation')).toBeInTheDocument();
    
    // Check for DAG anchor hash (shortened)
    expect(screen.getByText('bf3a7e...')).toBeInTheDocument();
    
    // Check for quorum status
    expect(screen.getByText('Verified')).toBeInTheDocument();
  });
  
  test('renders compact version with minimal info', () => {
    render(<AnchorNode credential={mockAnchorCredential} compact={true} />);
    
    // Check for minimal info in compact mode
    expect(screen.getByText('Epoch 2025-Q2')).toBeInTheDocument();
    expect(screen.getByText('bf3a7e...')).toBeInTheDocument();
    
    // These should not be present in compact mode
    expect(screen.queryByText('Palmyra Federation')).not.toBeInTheDocument();
    expect(screen.queryByText('Verified')).not.toBeInTheDocument();
  });
  
  test('handles click events', () => {
    const mockOnClick = jest.fn();
    render(<AnchorNode credential={mockAnchorCredential} onClick={mockOnClick} />);
    
    // Click the node
    fireEvent.click(screen.getByTestId('anchor-node'));
    
    // Check that onClick was called with the credential
    expect(mockOnClick).toHaveBeenCalledWith(mockAnchorCredential);
  });
  
  test('applies selected styling when selected', () => {
    const { container } = render(
      <AnchorNode credential={mockAnchorCredential} selected={true} />
    );
    
    // Check for ring class when selected
    const nodeElement = screen.getByTestId('anchor-node');
    expect(nodeElement).toHaveClass('ring-2');
    expect(nodeElement).toHaveClass('ring-white');
  });
}); 