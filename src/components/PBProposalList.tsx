import React, { useState, useMemo } from 'react';
import {
  Box,
  Typography,
  TextField,
  MenuItem,
  FormControl,
  InputLabel,
  Select,
  Divider,
  Chip,
  Paper,
  Button,
  Stack,
  CircularProgress,
  Pagination,
  Grid,
  Alert,
  InputAdornment,
  FormControlLabel,
  Switch
} from '@mui/material';
import FilterListIcon from '@mui/icons-material/FilterList';
import ClearIcon from '@mui/icons-material/Clear';
import PBProposalCard from './PBProposalCard';
import SearchIcon from '@mui/icons-material/Search';
import { SelectChangeEvent } from '@mui/material';

// Export these types to be used by other components
export type VoteOption = 'approve' | 'reject' | 'abstain';

export type ProposalStatus = 
  | 'draft'
  | 'active'
  | 'approved'
  | 'rejected'
  | 'expired'
  | 'canceled'
  | 'all';

export interface Federation {
  id: string;
  name: string;
  iconUrl?: string;
}

export interface PBProposal {
  id: string;
  title: string;
  description: string;
  status: ProposalStatus;
  federation: Federation;
  requestedAmount: number;
  currencySymbol: string;
  createdAt: string; // ISO date string
  expiresAt: string; // ISO date string
  // Voting stats
  votesApprove: number;
  votesReject: number;
  votesAbstain: number;
  totalVoteWeight: number;
  quorumRequired: number;
  approvalThreshold: number; // Percentage needed to pass (e.g., 66%)
  // User-specific
  userHasVoted?: boolean;
  userVoteOption?: VoteOption;
  category: string;
  author: {
    id: string;
    name: string;
  };
}

interface PBProposalListProps {
  proposals: PBProposal[];
  federations: Federation[];
  isLoading?: boolean;
  error?: string;
  onViewDetails: (proposalId: string) => void;
  onVote: (proposalId: string) => void;
  refreshProposals?: () => void;
}

interface FilterOptions {
  status?: string;
  federationId?: string;
  searchTerm?: string;
}

const PBProposalList: React.FC<PBProposalListProps> = ({
  proposals,
  federations,
  isLoading = false,
  error,
  onViewDetails,
  onVote,
  refreshProposals
}) => {
  // State for filtering and pagination
  const [searchQuery, setSearchQuery] = useState('');
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [federationFilter, setFederationFilter] = useState<string>('all');
  const [categoryFilter, setCategoryFilter] = useState<string>('all');
  const [sortBy, setSortBy] = useState<string>('newest');
  const [currentPage, setCurrentPage] = useState(1);
  const [itemsPerPage, setItemsPerPage] = useState(10);
  const [showMyVotable, setShowMyVotable] = useState(false);

  // Extract unique categories from proposals
  const categories = useMemo(() => {
    const uniqueCategories = new Set<string>();
    proposals.forEach(proposal => {
      if (proposal.category) {
        uniqueCategories.add(proposal.category);
      }
    });
    return Array.from(uniqueCategories);
  }, [proposals]);

  // Filter and sort proposals
  const filteredProposals = useMemo(() => {
    return proposals
      .filter(proposal => {
        // Text search filter
        const matchesSearch = 
          searchQuery === '' || 
          proposal.title.toLowerCase().includes(searchQuery.toLowerCase()) ||
          proposal.description.toLowerCase().includes(searchQuery.toLowerCase());
        
        // Status filter
        const matchesStatus = 
          statusFilter === 'all' || 
          proposal.status === statusFilter;
        
        // Federation filter
        const matchesFederation = 
          federationFilter === 'all' || 
          proposal.federation.id === federationFilter;
        
        // Category filter
        const matchesCategory = 
          categoryFilter === 'all' || 
          proposal.category === categoryFilter;
        
        // My votable filter - In a real implementation, you would check if the user can vote on this proposal
        // For now, we'll just check if proposal is active
        const matchesVotable = 
          !showMyVotable || 
          proposal.status === 'active';
        
        return matchesSearch && matchesStatus && matchesFederation && matchesCategory && matchesVotable;
      })
      .sort((a, b) => {
        switch (sortBy) {
          case 'newest':
            return new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime();
          case 'oldest':
            return new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime();
          case 'closing-soon':
            return new Date(a.expiresAt).getTime() - new Date(b.expiresAt).getTime();
          case 'highest-amount':
            return b.requestedAmount - a.requestedAmount;
          case 'lowest-amount':
            return a.requestedAmount - b.requestedAmount;
          case 'most-votes':
            return (b.votesApprove + b.votesReject) - (a.votesApprove + a.votesReject);
          default:
            return 0;
        }
      });
  }, [
    proposals, 
    searchQuery, 
    statusFilter, 
    federationFilter, 
    categoryFilter, 
    showMyVotable, 
    sortBy
  ]);

  // Pagination
  const pageCount = Math.ceil(filteredProposals.length / itemsPerPage);
  const paginatedProposals = useMemo(() => {
    const startIndex = (currentPage - 1) * itemsPerPage;
    return filteredProposals.slice(startIndex, startIndex + itemsPerPage);
  }, [filteredProposals, currentPage, itemsPerPage]);

  // Helper to display number of active filters
  const activeFilterCount = useMemo(() => {
    let count = 0;
    if (searchQuery) count++;
    if (statusFilter && statusFilter !== 'all') count++;
    if (federationFilter && federationFilter !== 'all') count++;
    if (categoryFilter && categoryFilter !== 'all') count++;
    if (showMyVotable) count++;
    return count;
  }, [searchQuery, statusFilter, federationFilter, categoryFilter, showMyVotable]);
  
  // Reset all filters
  const clearFilters = () => {
    setSearchQuery('');
    setStatusFilter('all');
    setFederationFilter('all');
    setCategoryFilter('all');
    setShowMyVotable(false);
    setCurrentPage(1);
  };
  
  // Render filter section
  const renderFilters = () => (
    <Paper variant="outlined" sx={{ p: 2, mb: 2 }}>
      <Stack spacing={2}>
        <Box display="flex" justifyContent="space-between" alignItems="center">
          <Typography variant="subtitle1">Filters</Typography>
          <Button 
            size="small" 
            startIcon={<ClearIcon />} 
            onClick={clearFilters}
            disabled={activeFilterCount === 0}
          >
            Clear All
          </Button>
        </Box>
        
        <TextField
          label="Search proposals"
          variant="outlined"
          fullWidth
          size="small"
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          placeholder="Search by title, description, or author"
        />
        
        <Box display="flex" gap={2}>
          <FormControl fullWidth size="small">
            <InputLabel>Status</InputLabel>
            <Select
              value={statusFilter}
              label="Status"
              onChange={(e) => setStatusFilter(e.target.value as ProposalStatus)}
            >
              <MenuItem value="all">All Statuses</MenuItem>
              <MenuItem value="active">Active</MenuItem>
              <MenuItem value="approved">Approved</MenuItem>
              <MenuItem value="rejected">Rejected</MenuItem>
              <MenuItem value="draft">Draft</MenuItem>
              <MenuItem value="expired">Expired</MenuItem>
            </Select>
          </FormControl>
          
          <FormControl fullWidth size="small">
            <InputLabel>Federation</InputLabel>
            <Select
              value={federationFilter}
              label="Federation"
              onChange={(e) => setFederationFilter(e.target.value)}
            >
              <MenuItem value="all">All Federations</MenuItem>
              {federations.map(federation => (
                <MenuItem key={federation.id} value={federation.id}>
                  {federation.name}
                </MenuItem>
              ))}
            </Select>
          </FormControl>
          
          <FormControl fullWidth size="small">
            <InputLabel>Category</InputLabel>
            <Select
              value={categoryFilter}
              label="Category"
              onChange={(e) => setCategoryFilter(e.target.value)}
            >
              <MenuItem value="all">All Categories</MenuItem>
              {categories.map(category => (
                <MenuItem key={category} value={category}>
                  {category}
                </MenuItem>
              ))}
            </Select>
          </FormControl>
          
          <FormControl fullWidth size="small">
            <InputLabel>Sort By</InputLabel>
            <Select
              value={sortBy}
              label="Sort By"
              onChange={(e) => setSortBy(e.target.value)}
            >
              <MenuItem value="newest">Newest First</MenuItem>
              <MenuItem value="oldest">Oldest First</MenuItem>
              <MenuItem value="closing-soon">Closing Soon</MenuItem>
              <MenuItem value="highest-amount">Highest Amount</MenuItem>
              <MenuItem value="lowest-amount">Lowest Amount</MenuItem>
              <MenuItem value="most-votes">Most Votes</MenuItem>
            </Select>
          </FormControl>
          
          <FormControl fullWidth size="small">
            <FormControlLabel
              control={
                <Switch
                  checked={showMyVotable}
                  onChange={(e) => setShowMyVotable(e.target.checked)}
                  color="primary"
                />
              }
              label="Show only votable proposals"
            />
          </FormControl>
        </Box>
      </Stack>
    </Paper>
  );

  // Handle filter changes
  const handleSearchChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    setSearchQuery(event.target.value);
    setCurrentPage(1); // Reset to first page when filter changes
  };

  const handleStatusChange = (event: SelectChangeEvent) => {
    setStatusFilter(event.target.value as ProposalStatus);
    setCurrentPage(1);
  };

  const handleFederationChange = (event: SelectChangeEvent) => {
    setFederationFilter(event.target.value);
    setCurrentPage(1);
  };

  const handleCategoryChange = (event: SelectChangeEvent) => {
    setCategoryFilter(event.target.value);
    setCurrentPage(1);
  };

  const handleSortChange = (event: SelectChangeEvent) => {
    setSortBy(event.target.value);
  };

  const handlePageChange = (event: React.ChangeEvent<unknown>, page: number) => {
    setCurrentPage(page);
  };

  const handleVotableChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    setShowMyVotable(event.target.checked);
    setCurrentPage(1);
  };

  return (
    <Box>
      <Box display="flex" justifyContent="space-between" alignItems="center" mb={2}>
        <Typography variant="h6">
          Participatory Budgeting Proposals
          {filteredProposals.length > 0 && (
            <Typography component="span" variant="body2" color="text.secondary" ml={1}>
              ({filteredProposals.length})
            </Typography>
          )}
        </Typography>
        
        <Box display="flex" gap={1}>
          <Button 
            startIcon={<FilterListIcon />}
            variant={activeFilterCount > 0 ? "contained" : "outlined"}
            size="small"
            onClick={renderFilters}
            color={activeFilterCount > 0 ? "primary" : "inherit"}
          >
            Filters
            {activeFilterCount > 0 && (
              <Chip 
                label={activeFilterCount} 
                size="small" 
                sx={{ ml: 1, height: 20, minWidth: 20 }}
              />
            )}
          </Button>
          
          {refreshProposals && (
            <Button 
              size="small"
              onClick={refreshProposals}
              disabled={isLoading}
            >
              {isLoading ? <CircularProgress size={20} /> : 'Refresh'}
            </Button>
          )}
        </Box>
      </Box>
      
      {renderFilters()}
      
      {isLoading && !proposals.length ? (
        <Box display="flex" justifyContent="center" my={4}>
          <CircularProgress />
        </Box>
      ) : error ? (
        <Paper variant="outlined" sx={{ p: 3, textAlign: 'center', color: 'error.main' }}>
          <Typography variant="body1">{error}</Typography>
          {refreshProposals && (
            <Button onClick={refreshProposals} sx={{ mt: 2 }}>
              Try Again
            </Button>
          )}
        </Paper>
      ) : filteredProposals.length === 0 ? (
        <Paper variant="outlined" sx={{ p: 3, textAlign: 'center' }}>
          <Typography variant="body1" color="text.secondary">
            {proposals.length === 0 
              ? 'No proposals available' 
              : 'No proposals match your filters'}
          </Typography>
          {activeFilterCount > 0 && (
            <Button onClick={clearFilters} sx={{ mt: 2 }}>
              Clear Filters
            </Button>
          )}
        </Paper>
      ) : (
        <>
          <Grid container spacing={3}>
            {paginatedProposals.map((proposal) => (
              <Grid item xs={12} sm={6} md={4} key={proposal.id}>
                <PBProposalCard
                  proposal={proposal}
                  onViewDetails={() => onViewDetails(proposal.id)}
                  onVote={() => onVote(proposal.id)}
                />
              </Grid>
            ))}
          </Grid>

          {/* Pagination */}
          {pageCount > 1 && (
            <Stack spacing={2} sx={{ mt: 3, display: 'flex', alignItems: 'center' }}>
              <Pagination 
                count={pageCount} 
                page={currentPage} 
                onChange={handlePageChange} 
                color="primary" 
              />
              <Typography variant="body2" color="text.secondary">
                Showing {(currentPage - 1) * itemsPerPage + 1}-
                {Math.min(currentPage * itemsPerPage, filteredProposals.length)} of {filteredProposals.length} proposals
              </Typography>
            </Stack>
          )}
        </>
      )}
    </Box>
  );
};

export default PBProposalList; 