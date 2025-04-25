import React, { useState } from 'react';
import {
  Card,
  CardContent,
  Box,
  Typography,
  Button,
  Chip,
  Dialog,
  DialogTitle,
  DialogContent,
  DialogActions,
  Divider,
  Grid,
  Paper,
  IconButton,
  Tooltip,
  Link,
} from '@mui/material';
import VerifiedIcon from '@mui/icons-material/Verified';
import VisibilityIcon from '@mui/icons-material/Visibility';
import ContentCopyIcon from '@mui/icons-material/ContentCopy';
import DescriptionIcon from '@mui/icons-material/Description';
import AccountTreeIcon from '@mui/icons-material/AccountTree';
import HowToVoteIcon from '@mui/icons-material/HowToVote';
import { WalletCredential } from '../../packages/credential-utils/types';

interface PBVoteReceiptViewProps {
  credential: WalletCredential;
  compact?: boolean;
  onClick?: () => void;
}

/**
 * Component for displaying a Participatory Budgeting vote receipt as a verifiable credential
 */
export const PBVoteReceiptView: React.FC<PBVoteReceiptViewProps> = ({
  credential,
  compact = false,
  onClick,
}) => {
  const [dialogOpen, setDialogOpen] = useState(false);
  const [jsonDialogOpen, setJsonDialogOpen] = useState(false);

  // Format date from ISO string
  const formatDate = (isoDate: string) => {
    return new Date(isoDate).toLocaleString();
  };

  // Get vote weight from credential
  const getVoteWeight = () => {
    return credential.credentialSubject.voteWeight || '1';
  };

  // Get voting mechanism from credential
  const getVotingMechanism = () => {
    return credential.credentialSubject.votingMechanism || 'simple_majority';
  };

  // Get vote choice from credential
  const getVoteChoice = () => {
    return credential.credentialSubject.voteChoice || 'approve';
  };

  // Format vote choice for display
  const formatVoteChoice = (choice: string) => {
    switch (choice.toLowerCase()) {
      case 'approve':
        return 'Approve';
      case 'reject':
        return 'Reject';
      case 'abstain':
        return 'Abstain';
      default:
        return choice;
    }
  };

  // Get color for vote choice
  const getVoteChoiceColor = (choice: string) => {
    switch (choice.toLowerCase()) {
      case 'approve':
        return 'success';
      case 'reject':
        return 'error';
      case 'abstain':
        return 'warning';
      default:
        return 'default';
    }
  };

  // Format voting mechanism for display
  const formatVotingMechanism = (mechanism: string) => {
    return mechanism.replace('_', ' ').replace(/\b\w/g, (c) => c.toUpperCase());
  };

  // Get DAG anchor from credential
  const getDagAnchor = () => {
    return credential.proof?.merkleRoot || credential.metadata?.dagAnchor || 'Unknown';
  };

  // Copy text to clipboard
  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
  };

  // Handle opening the detail dialog
  const handleOpenDialog = () => {
    setDialogOpen(true);
    if (onClick) onClick();
  };

  // Handle viewing the raw JSON
  const handleViewJson = (e: React.MouseEvent) => {
    e.stopPropagation();
    setJsonDialogOpen(true);
  };

  // Handle viewing the linked proposal
  const handleViewProposal = (e: React.MouseEvent) => {
    e.stopPropagation();
    // Navigate to proposal view or open in new tab
    // This would be implemented based on the app's navigation system
    console.log('View proposal:', credential.credentialSubject.proposalId);
  };

  // Handle viewing the DAG proof
  const handleViewDagProof = (e: React.MouseEvent) => {
    e.stopPropagation();
    // Navigate to DAG proof view or open in new tab
    // This would be implemented based on the app's navigation system
    console.log('View DAG proof:', getDagAnchor());
  };

  // Compact view for list items
  if (compact) {
    return (
      <Card 
        sx={{ 
          mb: 1, 
          cursor: 'pointer',
          '&:hover': { boxShadow: 3 },
          border: credential.trustLevel === 'Verified' ? '1px solid #4caf50' : undefined,
          borderLeft: credential.trustLevel === 'Verified' ? '5px solid #4caf50' : undefined,
        }}
        onClick={handleOpenDialog}
      >
        <CardContent sx={{ p: 2 }}>
          <Box sx={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', mb: 1 }}>
            <Typography variant="subtitle1" component="div" noWrap sx={{ fontWeight: 'medium' }}>
              Vote on: {credential.credentialSubject.proposalTitle || credential.title}
            </Typography>
            {credential.trustLevel === 'Verified' && (
              <Tooltip title="Verified Credential">
                <VerifiedIcon color="success" fontSize="small" />
              </Tooltip>
            )}
          </Box>
          
          <Box sx={{ display: 'flex', mb: 1, gap: 1, flexWrap: 'wrap' }}>
            <Chip 
              label={formatVoteChoice(getVoteChoice())} 
              color={getVoteChoiceColor(getVoteChoice()) as any}
              size="small"
            />
            <Chip 
              icon={<HowToVoteIcon fontSize="small" />}
              label={`Weight: ${getVoteWeight()}`}
              size="small"
              variant="outlined"
            />
            <Chip 
              label={formatVotingMechanism(getVotingMechanism())}
              size="small"
              variant="outlined"
            />
          </Box>
          
          <Box sx={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <Typography variant="caption" color="text.secondary">
              {formatDate(credential.issuanceDate)}
            </Typography>
            <Typography variant="caption" color="text.secondary" noWrap sx={{ maxWidth: '50%' }}>
              Federation: {credential.credentialSubject.federationId || credential.issuer.name || credential.issuer.did}
            </Typography>
          </Box>
        </CardContent>
      </Card>
    );
  }

  // Full view
  return (
    <>
      <Card 
        sx={{ 
          mb: 2, 
          cursor: onClick ? 'pointer' : undefined,
          border: credential.trustLevel === 'Verified' ? '1px solid #4caf50' : undefined,
          borderLeft: credential.trustLevel === 'Verified' ? '5px solid #4caf50' : undefined,
        }}
        onClick={onClick ? handleOpenDialog : undefined}
      >
        <CardContent>
          <Box sx={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', mb: 2 }}>
            <Typography variant="h5" component="div">
              Participatory Budgeting Vote
            </Typography>
            {credential.trustLevel === 'Verified' && (
              <Tooltip title="Verified Credential">
                <VerifiedIcon color="success" />
              </Tooltip>
            )}
          </Box>
          
          <Box sx={{ mb: 2 }}>
            <Typography variant="h6" gutterBottom>
              {credential.credentialSubject.proposalTitle || credential.title}
            </Typography>
            <Typography variant="body2" color="text.secondary" gutterBottom>
              Proposal ID: {credential.credentialSubject.proposalId}
            </Typography>
          </Box>
          
          <Grid container spacing={2} sx={{ mb: 2 }}>
            <Grid item xs={12} sm={6}>
              <Paper sx={{ p: 2, height: '100%' }}>
                <Typography variant="subtitle2" gutterBottom>
                  Vote Details
                </Typography>
                <Box sx={{ display: 'flex', flexDirection: 'column', gap: 1 }}>
                  <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                    <Typography variant="body2">Choice:</Typography>
                    <Chip 
                      label={formatVoteChoice(getVoteChoice())} 
                      color={getVoteChoiceColor(getVoteChoice()) as any}
                      size="small"
                    />
                  </Box>
                  <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                    <Typography variant="body2">Weight:</Typography>
                    <Chip 
                      label={getVoteWeight()}
                      size="small"
                    />
                  </Box>
                  <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                    <Typography variant="body2">Mechanism:</Typography>
                    <Chip 
                      label={formatVotingMechanism(getVotingMechanism())}
                      size="small"
                    />
                  </Box>
                </Box>
              </Paper>
            </Grid>
            
            <Grid item xs={12} sm={6}>
              <Paper sx={{ p: 2, height: '100%' }}>
                <Typography variant="subtitle2" gutterBottom>
                  Verification
                </Typography>
                <Box sx={{ display: 'flex', flexDirection: 'column', gap: 1 }}>
                  <Typography variant="body2">
                    <strong>Issuer:</strong> {credential.issuer.name || credential.issuer.did}
                  </Typography>
                  <Typography variant="body2">
                    <strong>Date:</strong> {formatDate(credential.issuanceDate)}
                  </Typography>
                  <Typography variant="body2">
                    <strong>Federation:</strong> {credential.credentialSubject.federationId || 
                      credential.metadata?.federation?.id || 'Unknown'}
                  </Typography>
                </Box>
              </Paper>
            </Grid>
          </Grid>
          
          <Box sx={{ mb: 1 }}>
            <Typography variant="subtitle2" gutterBottom>
              DAG Anchor
            </Typography>
            <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
              <Typography 
                variant="body2" 
                sx={{ 
                  fontFamily: 'monospace',
                  overflow: 'hidden',
                  textOverflow: 'ellipsis',
                  maxWidth: '80%'
                }}
              >
                {getDagAnchor()}
              </Typography>
              <Tooltip title="Copy to clipboard">
                <IconButton size="small" onClick={() => copyToClipboard(getDagAnchor())}>
                  <ContentCopyIcon fontSize="small" />
                </IconButton>
              </Tooltip>
            </Box>
          </Box>
          
          <Box sx={{ display: 'flex', justifyContent: 'space-between', mt: 2 }}>
            <Button
              variant="outlined"
              startIcon={<DescriptionIcon />}
              size="small"
              onClick={handleViewJson}
            >
              View Credential
            </Button>
            
            <Box sx={{ display: 'flex', gap: 1 }}>
              <Button
                variant="outlined"
                startIcon={<VisibilityIcon />}
                size="small"
                onClick={handleViewProposal}
              >
                View Proposal
              </Button>
              <Button
                variant="outlined"
                startIcon={<AccountTreeIcon />}
                size="small"
                onClick={handleViewDagProof}
              >
                View DAG Proof
              </Button>
            </Box>
          </Box>
        </CardContent>
      </Card>

      {/* Detail Dialog */}
      <Dialog 
        open={dialogOpen} 
        onClose={() => setDialogOpen(false)}
        maxWidth="md"
        fullWidth
      >
        <DialogTitle>
          <Box sx={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <Typography variant="h6">
              Participatory Budgeting Vote Receipt
            </Typography>
            {credential.trustLevel === 'Verified' && (
              <Tooltip title="Verified Credential">
                <VerifiedIcon color="success" />
              </Tooltip>
            )}
          </Box>
        </DialogTitle>
        <DialogContent dividers>
          <Box sx={{ mb: 3 }}>
            <Typography variant="h5" gutterBottom>
              {credential.credentialSubject.proposalTitle || credential.title}
            </Typography>
            <Typography variant="body1" gutterBottom>
              {credential.credentialSubject.proposalDescription || ''}
            </Typography>
            <Box sx={{ display: 'flex', gap: 1, mt: 1 }}>
              <Chip 
                label={formatVoteChoice(getVoteChoice())} 
                color={getVoteChoiceColor(getVoteChoice()) as any}
              />
              <Chip 
                icon={<HowToVoteIcon />}
                label={`Weight: ${getVoteWeight()}`}
                variant="outlined"
              />
              <Chip 
                label={formatVotingMechanism(getVotingMechanism())}
                variant="outlined"
              />
            </Box>
          </Box>
          
          <Divider sx={{ mb: 3 }} />
          
          <Grid container spacing={3}>
            <Grid item xs={12} md={6}>
              <Typography variant="subtitle1" gutterBottom>
                Proposal Details
              </Typography>
              <Typography variant="body2" gutterBottom>
                <strong>ID:</strong> {credential.credentialSubject.proposalId}
              </Typography>
              <Typography variant="body2" gutterBottom>
                <strong>Federation:</strong> {credential.credentialSubject.federationId || 
                  credential.metadata?.federation?.id || 'Unknown'}
              </Typography>
              <Typography variant="body2" gutterBottom>
                <strong>Amount:</strong> {credential.credentialSubject.requestedAmount || 'Unknown'} {' '}
                {credential.credentialSubject.tokenType || ''}
              </Typography>
              {credential.credentialSubject.comment && (
                <Box sx={{ mt: 2 }}>
                  <Typography variant="subtitle2" gutterBottom>
                    Your Comment
                  </Typography>
                  <Paper sx={{ p: 2, bgcolor: 'background.default' }}>
                    <Typography variant="body2">
                      {credential.credentialSubject.comment}
                    </Typography>
                  </Paper>
                </Box>
              )}
            </Grid>
            
            <Grid item xs={12} md={6}>
              <Typography variant="subtitle1" gutterBottom>
                Credential Information
              </Typography>
              <Typography variant="body2" gutterBottom>
                <strong>Issuer:</strong> {credential.issuer.name || credential.issuer.did}
              </Typography>
              <Typography variant="body2" gutterBottom>
                <strong>Date Issued:</strong> {formatDate(credential.issuanceDate)}
              </Typography>
              <Typography variant="body2" gutterBottom>
                <strong>Credential ID:</strong> {credential.id}
              </Typography>
              <Typography variant="body2" gutterBottom>
                <strong>Trust Level:</strong> {credential.trustLevel}
              </Typography>
              <Typography variant="body2" gutterBottom>
                <strong>DAG Anchor:</strong>
              </Typography>
              <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                <Typography 
                  variant="body2" 
                  sx={{ 
                    fontFamily: 'monospace',
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    maxWidth: '80%',
                    bgcolor: 'background.default',
                    p: 1,
                    borderRadius: 1
                  }}
                >
                  {getDagAnchor()}
                </Typography>
                <Tooltip title="Copy to clipboard">
                  <IconButton size="small" onClick={() => copyToClipboard(getDagAnchor())}>
                    <ContentCopyIcon fontSize="small" />
                  </IconButton>
                </Tooltip>
              </Box>
            </Grid>
          </Grid>
          
          <Box sx={{ mt: 3 }}>
            <Typography variant="subtitle1" gutterBottom>
              Actions
            </Typography>
            <Box sx={{ display: 'flex', gap: 2 }}>
              <Button
                variant="outlined"
                startIcon={<VisibilityIcon />}
                onClick={handleViewProposal}
              >
                View Proposal
              </Button>
              <Button
                variant="outlined"
                startIcon={<AccountTreeIcon />}
                onClick={handleViewDagProof}
              >
                View DAG Proof
              </Button>
              <Button
                variant="outlined"
                startIcon={<DescriptionIcon />}
                onClick={handleViewJson}
              >
                View Raw Credential
              </Button>
            </Box>
          </Box>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setDialogOpen(false)}>
            Close
          </Button>
        </DialogActions>
      </Dialog>

      {/* JSON Dialog */}
      <Dialog 
        open={jsonDialogOpen} 
        onClose={() => setJsonDialogOpen(false)}
        maxWidth="md"
        fullWidth
      >
        <DialogTitle>
          Raw Credential JSON
        </DialogTitle>
        <DialogContent dividers>
          <Paper 
            sx={{ 
              p: 2, 
              fontFamily: 'monospace', 
              whiteSpace: 'pre-wrap',
              overflow: 'auto',
              maxHeight: '60vh'
            }}
          >
            {JSON.stringify(credential, null, 2)}
          </Paper>
        </DialogContent>
        <DialogActions>
          <Button 
            onClick={() => copyToClipboard(JSON.stringify(credential, null, 2))}
            startIcon={<ContentCopyIcon />}
          >
            Copy
          </Button>
          <Button onClick={() => setJsonDialogOpen(false)}>
            Close
          </Button>
        </DialogActions>
      </Dialog>
    </>
  );
}; 