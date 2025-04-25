import React, { useState, useEffect } from 'react';
import { CredentialDAGView } from './CredentialDAGView';
import { WalletCredential } from '../../packages/credential-utils/types';
import { FederationManifest } from '../../packages/credential-utils/types/federation';

interface FederationQuorumDashboardProps {
  credentials: WalletCredential[];
  federationManifests: Record<string, FederationManifest>;
  onCredentialSelect?: (id: string) => void;
  width?: number;
  height?: number;
}

/**
 * A dashboard component that displays federation-signed reports and their quorum validation status
 * using an enhanced version of the CredentialDAGView
 */
export const FederationQuorumDashboard: React.FC<FederationQuorumDashboardProps> = ({
  credentials,
  federationManifests,
  onCredentialSelect,
  width = 900,
  height = 700,
}) => {
  const [selectedCredentialId, setSelectedCredentialId] = useState<string | undefined>(undefined);
  const [selectedReport, setSelectedReport] = useState<WalletCredential | null>(null);
  const [viewOptions, setViewOptions] = useState({
    showSignerNodes: true,
    showMissingSigners: true,
    showLabels: true,
    groupByThread: false,
  });

  // Update selected report when credential is selected
  useEffect(() => {
    if (selectedCredentialId) {
      const credential = credentials.find(c => c.id === selectedCredentialId);
      setSelectedReport(credential || null);
    } else {
      setSelectedReport(null);
    }
  }, [selectedCredentialId, credentials]);

  // Handle credential selection
  const handleCredentialSelect = (id: string) => {
    setSelectedCredentialId(id);
    if (onCredentialSelect) {
      onCredentialSelect(id);
    }
  };

  // Toggle view options
  const toggleOption = (option: keyof typeof viewOptions) => {
    setViewOptions({
      ...viewOptions,
      [option]: !viewOptions[option],
    });
  };

  return (
    <div className="federation-quorum-dashboard">
      <div className="dashboard-header" style={{ padding: '10px', marginBottom: '20px' }}>
        <h2>Federation Quorum Visualization</h2>
        <div className="view-options" style={{ display: 'flex', gap: '15px', marginBottom: '15px' }}>
          <label>
            <input
              type="checkbox"
              checked={viewOptions.showSignerNodes}
              onChange={() => toggleOption('showSignerNodes')}
            />
            Show Signers
          </label>
          <label>
            <input
              type="checkbox"
              checked={viewOptions.showMissingSigners}
              onChange={() => toggleOption('showMissingSigners')}
            />
            Show Missing Signers
          </label>
          <label>
            <input
              type="checkbox"
              checked={viewOptions.showLabels}
              onChange={() => toggleOption('showLabels')}
            />
            Show Labels
          </label>
          <label>
            <input
              type="checkbox"
              checked={viewOptions.groupByThread}
              onChange={() => toggleOption('groupByThread')}
            />
            Group by Thread
          </label>
        </div>
        
        <div className="legend" style={{ display: 'flex', gap: '20px', marginBottom: '15px' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: '5px' }}>
            <div style={{ width: '15px', height: '15px', borderRadius: '50%', backgroundColor: '#4CAF50' }}></div>
            <span>Quorum Satisfied</span>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: '5px' }}>
            <div style={{ width: '15px', height: '15px', borderRadius: '50%', backgroundColor: '#FFC107' }}></div>
            <span>Partial Quorum</span>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: '5px' }}>
            <div style={{ width: '15px', height: '15px', borderRadius: '50%', backgroundColor: '#F44336' }}></div>
            <span>Invalid/No Quorum</span>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: '5px' }}>
            <div style={{ width: '15px', height: '15px', borderRadius: '50%', backgroundColor: '#64B5F6' }}></div>
            <span>Signer</span>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: '5px' }}>
            <div style={{ width: '15px', height: '15px', borderRadius: '50%', backgroundColor: '#BDBDBD' }}></div>
            <span>Missing Signer</span>
          </div>
        </div>
        
        <div className="tip" style={{ fontSize: '14px', color: '#666', marginBottom: '10px' }}>
          Tip: Hover over nodes to see details. Click a node to select it.
        </div>
      </div>
      
      <div className="dashboard-content" style={{ display: 'flex', height: `${height}px` }}>
        <div className="graph-container" style={{ flex: '1' }}>
          <CredentialDAGView
            credentials={credentials}
            selectedCredentialId={selectedCredentialId}
            onCredentialSelect={handleCredentialSelect}
            width={width}
            height={height}
            showLabels={viewOptions.showLabels}
            groupByThread={viewOptions.groupByThread}
            highlightSelected={true}
            federationManifests={federationManifests}
            showSignerNodes={viewOptions.showSignerNodes}
            showMissingSigners={viewOptions.showMissingSigners}
          />
        </div>
        
        {selectedReport && (
          <div className="report-details" style={{ width: '300px', padding: '15px', borderLeft: '1px solid #eee', overflow: 'auto' }}>
            <h3>Selected Report</h3>
            <div style={{ marginBottom: '15px' }}>
              <strong>ID:</strong> <span style={{ fontSize: '12px', wordBreak: 'break-all' }}>{selectedReport.id}</span>
            </div>
            <div style={{ marginBottom: '15px' }}>
              <strong>Type:</strong> {Array.isArray(selectedReport.type) ? selectedReport.type.join(', ') : selectedReport.type}
            </div>
            
            {/* Display federation information */}
            {selectedReport.metadata?.federation && (
              <div style={{ marginBottom: '15px' }}>
                <strong>Federation:</strong> {selectedReport.metadata.federation.name || selectedReport.metadata.federation.id}
              </div>
            )}
            
            {/* Display issuance date */}
            <div style={{ marginBottom: '15px' }}>
              <strong>Issued:</strong> {new Date(selectedReport.issuanceDate).toLocaleString()}
            </div>
            
            {/* Display quorum information if it's a federation report */}
            {(Array.isArray(selectedReport.type) && selectedReport.type.includes('FederationReport')) && (
              <div className="quorum-info">
                <h4>Quorum Information</h4>
                {(selectedReport as any).multiSignatureProof?.signatures ? (
                  <div>
                    <div><strong>Signatures:</strong> {(selectedReport as any).multiSignatureProof.signatures.length}</div>
                    <div><strong>Policy:</strong> {selectedReport.metadata?.federationMetadata?.quorum_policy || 'Unknown'}</div>
                    
                    <h5>Signers</h5>
                    <ul style={{ paddingLeft: '20px', marginTop: '5px' }}>
                      {(selectedReport as any).multiSignatureProof.signatures.map((sig: any, i: number) => (
                        <li key={i} style={{ fontSize: '12px', marginBottom: '5px' }}>
                          {sig.verificationMethod.split('#')[0]}
                        </li>
                      ))}
                    </ul>
                  </div>
                ) : (
                  <div>No signature information available</div>
                )}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
};

export default FederationQuorumDashboard; 