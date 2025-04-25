import React, { useState } from 'react';
import { 
  WalletCredential, 
  linkCredentialToAgoraThread,
  CredentialLinkResult
} from '../../packages/credential-utils';
import { Button, Form, FormGroup, Input, Label, Card, CardBody, CardHeader, Alert } from 'reactstrap';

interface CredentialLinkFormProps {
  credential: WalletCredential;
  agoraNetEndpoint: string;
  onLinkComplete: (result: CredentialLinkResult) => void;
  onCancel: () => void;
}

export const CredentialLinkForm: React.FC<CredentialLinkFormProps> = ({
  credential,
  agoraNetEndpoint,
  onLinkComplete,
  onCancel
}) => {
  const [threadId, setThreadId] = useState<string>('');
  const [isManualThreadId, setIsManualThreadId] = useState<boolean>(false);
  const [metadata, setMetadata] = useState<string>('');
  const [isLinking, setIsLinking] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  
  // Extract proposal ID from the credential for display
  const proposalId = credential.credentialSubject.proposalId || 'Unknown';
  
  const handleLinkClick = async () => {
    setIsLinking(true);
    setError(null);
    
    try {
      // Parse metadata JSON if provided
      const parsedMetadata = metadata ? JSON.parse(metadata) : undefined;
      
      // Call the linking function
      const result = await linkCredentialToAgoraThread(credential, {
        agoraNetEndpoint,
        threadId: isManualThreadId ? threadId : undefined,
        metadata: parsedMetadata
      });
      
      if (result.success) {
        onLinkComplete(result);
      } else {
        setError(result.error || 'Failed to link credential to thread');
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'An unexpected error occurred');
    } finally {
      setIsLinking(false);
    }
  };
  
  return (
    <Card className="mb-4">
      <CardHeader>
        <h5>Link Credential to AgoraNet Thread</h5>
        <p className="text-muted">
          Connect this credential to a discussion thread on AgoraNet
        </p>
      </CardHeader>
      
      <CardBody>
        {error && (
          <Alert color="danger" className="mb-3">
            {error}
          </Alert>
        )}
        
        <div className="mb-3">
          <h6>Credential Details</h6>
          <dl className="row mb-0">
            <dt className="col-sm-3">Title</dt>
            <dd className="col-sm-9">{credential.title}</dd>
            
            <dt className="col-sm-3">Type</dt>
            <dd className="col-sm-9">{credential.type}</dd>
            
            <dt className="col-sm-3">Proposal ID</dt>
            <dd className="col-sm-9">{proposalId}</dd>
            
            <dt className="col-sm-3">Issuer</dt>
            <dd className="col-sm-9">{credential.issuer.name || credential.issuer.did}</dd>
          </dl>
        </div>
        
        <Form>
          <FormGroup className="mb-3">
            <div className="form-check">
              <Input
                type="checkbox"
                id="manualThreadId"
                checked={isManualThreadId}
                onChange={() => setIsManualThreadId(!isManualThreadId)}
              />
              <Label for="manualThreadId" className="form-check-label">
                Specify thread ID manually (optional)
              </Label>
            </div>
            <small className="form-text text-muted">
              If left unchecked, AgoraNet will automatically find a thread matching the proposal ID
            </small>
          </FormGroup>
          
          {isManualThreadId && (
            <FormGroup className="mb-3">
              <Label for="threadId">Thread ID</Label>
              <Input
                type="text"
                id="threadId"
                value={threadId}
                onChange={(e) => setThreadId(e.target.value)}
                placeholder="Enter AgoraNet thread ID"
              />
            </FormGroup>
          )}
          
          <FormGroup className="mb-3">
            <Label for="metadata">Additional Metadata (JSON, optional)</Label>
            <Input
              type="textarea"
              id="metadata"
              value={metadata}
              onChange={(e) => setMetadata(e.target.value)}
              placeholder='{"key": "value"}'
              rows={3}
            />
            <small className="form-text text-muted">
              Optional metadata to include with the credential link (must be valid JSON)
            </small>
          </FormGroup>
          
          <div className="d-flex justify-content-end gap-2 mt-4">
            <Button color="secondary" onClick={onCancel} disabled={isLinking}>
              Cancel
            </Button>
            <Button 
              color="primary" 
              onClick={handleLinkClick}
              disabled={isLinking || (isManualThreadId && !threadId)}
            >
              {isLinking ? 'Linking...' : 'Link Credential'}
            </Button>
          </div>
        </Form>
      </CardBody>
    </Card>
  );
}; 