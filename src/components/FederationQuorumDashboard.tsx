import React, { useState, useEffect } from 'react';
import {
  Box,
  Button,
  Card,
  Checkbox,
  FormControlLabel,
  Grid,
  Paper,
  Typography,
  Divider,
  Drawer,
  List,
  ListItem,
  ListItemText,
  IconButton,
  Tooltip,
} from '@mui/material';
import CloseIcon from '@mui/icons-material/Close';
import InfoIcon from '@mui/icons-material/Info';
import { WalletCredential } from '../../packages/credential-utils/types';
import { FederationManifest } from '../../packages/credential-utils/types/federation';
import { CredentialDAGView } from './CredentialDAGView';
import { validateFederationReport } from '../../packages/credential-utils/utils/quorumValidation';

// Define prop interface
interface FederationQuorumDashboardProps {
  credentials: WalletCredential[];
  federationManifests: Record<string, FederationManifest>;
}

/**
 * Component for displaying federation-signed reports and their quorum validation status
 * using the CredentialDAGView
 */
export const FederationQuorumDashboard: React.FC<FederationQuorumDashboardProps> = ({
  credentials,
  federationManifests,
}) => {
  // State for view options
  const [showLabels, setShowLabels] = useState(true);
  const [groupByThread, setGroupByThread] = useState(false);
  const [showSignerNodes, setShowSignerNodes] = useState(true);
  const [showMissingSigners, setShowMissingSigners] = useState(false);
  const [selectedCredentialId, setSelectedCredentialId] = useState<string | undefined>(undefined);
  
  // State for signer drill-down
  const [selectedSignerDid, setSelectedSignerDid] = useState<string | undefined>(undefined);
  const [selectedFederationId, setSelectedFederationId] = useState<string | undefined>(undefined);
  const [signerDetailsOpen, setSignerDetailsOpen] = useState(false);
  const [signerCredentials, setSignerCredentials] = useState<WalletCredential[]>([]);

  // Filter credentials that are federation reports
  const federationReports = credentials.filter(cred => 
    Array.isArray(cred.type) && cred.type.includes('FederationReport') || 
    (cred.metadata?.federationMetadata && (cred as any).multiSignatureProof?.signatures?.length > 0)
  );

  // Handle credential selection
  const handleCredentialSelect = (id: string) => {
    setSelectedCredentialId(id);
    // Close signer details if open
    if (signerDetailsOpen) {
      setSignerDetailsOpen(false);
    }
  };

  // Handle signer selection
  const handleSignerSelect = (signerDid: string, federationId: string) => {
    setSelectedSignerDid(signerDid);
    setSelectedFederationId(federationId);
    
    // Find credentials associated with this signer
    const signerCreds = credentials.filter(cred => {
      // Check if this credential was issued by the signer
      if (cred.issuer.did === signerDid) return true;
      
      // Check if this credential has a signature from the signer
      if ((cred as any).multiSignatureProof?.signatures) {
        return (cred as any).multiSignatureProof.signatures.some(
          (sig: any) => sig.verificationMethod?.includes(signerDid)
        );
      }
      
      return false;
    });
    
    setSignerCredentials(signerCreds);
    setSignerDetailsOpen(true);
  };

  // Close signer details panel
  const handleCloseSignerDetails = () => {
    setSignerDetailsOpen(false);
  };

  // Get federation member details if available
  const getSignerDetails = () => {
    if (!selectedSignerDid || !selectedFederationId || !federationManifests[selectedFederationId]) {
      return null;
    }
    
    const manifest = federationManifests[selectedFederationId];
    const memberInfo = manifest.members[selectedSignerDid];
    
    return memberInfo ? {
      role: memberInfo.role || 'Unknown',
      weight: memberInfo.weight || 1,
      did: selectedSignerDid,
      federation: manifest.name || selectedFederationId,
      canVeto: memberInfo.can_veto || false,
      votingPower: memberInfo.voting_power || memberInfo.weight || 1
    } : {
      role: 'Unknown',
      weight: 0,
      did: selectedSignerDid,
      federation: manifest.name || selectedFederationId,
      canVeto: false,
      votingPower: 0
    };
  };

  const signerDetails = getSignerDetails();

  return (
    <Grid container spacing={2}>
      <Grid item xs={12}>
        <Typography variant="h5" gutterBottom>
          Federation Quorum Dashboard
        </Typography>
        <Paper sx={{ p: 2, mb: 2 }}>
          <Typography variant="h6" gutterBottom>
            View Options
          </Typography>
          <Grid container spacing={2}>
            <Grid item xs={12} sm={6} md={3}>
              <FormControlLabel
                control={
                  <Checkbox
                    checked={showLabels}
                    onChange={(e) => setShowLabels(e.target.checked)}
                  />
                }
                label="Show Labels"
              />
            </Grid>
            <Grid item xs={12} sm={6} md={3}>
              <FormControlLabel
                control={
                  <Checkbox
                    checked={groupByThread}
                    onChange={(e) => setGroupByThread(e.target.checked)}
                  />
                }
                label="Group by Thread"
              />
            </Grid>
            <Grid item xs={12} sm={6} md={3}>
              <FormControlLabel
                control={
                  <Checkbox
                    checked={showSignerNodes}
                    onChange={(e) => setShowSignerNodes(e.target.checked)}
                  />
                }
                label="Show Signers"
              />
            </Grid>
            <Grid item xs={12} sm={6} md={3}>
              <FormControlLabel
                control={
                  <Checkbox
                    checked={showMissingSigners}
                    onChange={(e) => setShowMissingSigners(e.target.checked)}
                  />
                }
                label="Show Missing Signers"
              />
            </Grid>
          </Grid>
        </Paper>
      </Grid>
      <Grid item xs={12}>
        <Paper sx={{ p: 2, height: '70vh' }}>
          <CredentialDAGView
            credentials={federationReports}
            selectedCredentialId={selectedCredentialId}
            onCredentialSelect={handleCredentialSelect}
            onSignerSelect={handleSignerSelect}
            width={1000}
            height={600}
            showLabels={showLabels}
            groupByThread={groupByThread}
            federationManifests={federationManifests}
            showSignerNodes={showSignerNodes}
            showMissingSigners={showMissingSigners}
          />
        </Paper>
      </Grid>
      <Grid item xs={12}>
        <Paper sx={{ p: 2 }}>
          <Typography variant="h6" gutterBottom>
            Quorum Legend
          </Typography>
          <Grid container spacing={2}>
            <Grid item xs={12} sm={6} md={4}>
              <Box display="flex" alignItems="center">
                <Box
                  sx={{
                    width: 20,
                    height: 20,
                    backgroundColor: '#4CAF50',
                    borderRadius: '50%',
                    mr: 1,
                  }}
                />
                <Typography variant="body2">Satisfied Quorum</Typography>
              </Box>
            </Grid>
            <Grid item xs={12} sm={6} md={4}>
              <Box display="flex" alignItems="center">
                <Box
                  sx={{
                    width: 20,
                    height: 20,
                    backgroundColor: '#FFC107',
                    borderRadius: '50%',
                    mr: 1,
                  }}
                />
                <Typography variant="body2">Partial Quorum</Typography>
              </Box>
            </Grid>
            <Grid item xs={12} sm={6} md={4}>
              <Box display="flex" alignItems="center">
                <Box
                  sx={{
                    width: 20,
                    height: 20,
                    backgroundColor: '#F44336',
                    borderRadius: '50%',
                    mr: 1,
                  }}
                />
                <Typography variant="body2">Unsatisfied Quorum</Typography>
              </Box>
            </Grid>
            <Grid item xs={12} sm={6} md={4}>
              <Box display="flex" alignItems="center">
                <Box
                  sx={{
                    width: 20,
                    height: 20,
                    backgroundColor: '#64B5F6',
                    borderRadius: '50%',
                    mr: 1,
                  }}
                />
                <Typography variant="body2">Signer</Typography>
              </Box>
            </Grid>
            <Grid item xs={12} sm={6} md={4}>
              <Box display="flex" alignItems="center">
                <Box
                  sx={{
                    width: 20,
                    height: 20,
                    backgroundColor: '#BDBDBD',
                    borderRadius: '50%',
                    mr: 1,
                  }}
                />
                <Typography variant="body2">Missing Signer</Typography>
              </Box>
            </Grid>
          </Grid>
        </Paper>
      </Grid>
      
      {/* Signer Details Drawer */}
      <Drawer
        anchor="right"
        open={signerDetailsOpen}
        onClose={handleCloseSignerDetails}
        sx={{ 
          '& .MuiDrawer-paper': { 
            width: { xs: '100%', sm: 400 }, 
            padding: 2 
          } 
        }}
      >
        <Box sx={{ p: 2 }}>
          <Box display="flex" justifyContent="space-between" alignItems="center" mb={2}>
            <Typography variant="h6">Signer Details</Typography>
            <IconButton onClick={handleCloseSignerDetails} size="small">
              <CloseIcon />
            </IconButton>
          </Box>
          
          <Divider sx={{ mb: 2 }} />
          
          {signerDetails ? (
            <>
              <List disablePadding>
                <ListItem>
                  <ListItemText 
                    primary="DID" 
                    secondary={signerDetails.did} 
                    secondaryTypographyProps={{ 
                      sx: { wordBreak: 'break-all' } 
                    }}
                  />
                </ListItem>
                <ListItem>
                  <ListItemText 
                    primary="Federation" 
                    secondary={signerDetails.federation}
                  />
                </ListItem>
                <ListItem>
                  <ListItemText 
                    primary="Role" 
                    secondary={signerDetails.role}
                  />
                </ListItem>
                <ListItem>
                  <ListItemText 
                    primary="Weight" 
                    secondary={signerDetails.weight}
                  />
                </ListItem>
                <ListItem>
                  <ListItemText 
                    primary="Voting Power" 
                    secondary={signerDetails.votingPower}
                  />
                </ListItem>
                <ListItem>
                  <ListItemText 
                    primary="Can Veto" 
                    secondary={signerDetails.canVeto ? 'Yes' : 'No'}
                  />
                </ListItem>
              </List>
              
              <Typography variant="h6" sx={{ mt: 3, mb: 1 }}>
                Associated Credentials ({signerCredentials.length})
              </Typography>
              
              {signerCredentials.length > 0 ? (
                <List sx={{ maxHeight: 300, overflow: 'auto' }}>
                  {signerCredentials.map((cred) => (
                    <ListItem key={cred.id} button onClick={() => handleCredentialSelect(cred.id)}>
                      <ListItemText 
                        primary={Array.isArray(cred.type) ? cred.type[0] : cred.type}
                        secondary={
                          <>
                            <Typography variant="body2" component="span">
                              {cred.id.substring(0, 15)}...
                            </Typography>
                            <br />
                            <Typography variant="caption" component="span">
                              {new Date(cred.issuanceDate).toLocaleDateString()}
                            </Typography>
                          </>
                        }
                      />
                    </ListItem>
                  ))}
                </List>
              ) : (
                <Typography variant="body2" color="text.secondary">
                  No credentials associated with this signer.
                </Typography>
              )}
            </>
          ) : (
            <Typography variant="body1">
              No details available for this signer.
            </Typography>
          )}
        </Box>
      </Drawer>
    </Grid>
  );
};

export default FederationQuorumDashboard; 