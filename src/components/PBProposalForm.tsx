import React, { useState, useEffect } from 'react';
import {
  Box,
  Button,
  TextField,
  Typography,
  Paper,
  Grid,
  FormControl,
  InputLabel,
  Select,
  MenuItem,
  FormHelperText,
  Slider,
  InputAdornment,
  Divider,
  Alert,
  Chip,
  IconButton,
  Tooltip
} from '@mui/material';
import {
  Save as SaveIcon,
  Delete as DeleteIcon,
  Add as AddIcon,
  Info as InfoIcon,
  CheckCircle as CheckCircleIcon
} from '@mui/icons-material';

export interface Federation {
  id: string;
  name: string;
  avatarUrl?: string;
}

export interface PBProposalFormData {
  title: string;
  description: string;
  requestedAmount: number;
  currencyCode: string;
  category: string;
  federation: string;
  approvalThreshold: number;
  quorumRequired: number;
  votingPeriodDays: number;
  attachments?: File[];
  additionalInfo?: Record<string, string>;
}

interface PBProposalFormProps {
  federations: Federation[];
  categories: string[];
  currencies: string[];
  initialData?: Partial<PBProposalFormData>;
  onSubmit: (data: PBProposalFormData) => Promise<void>;
  onCancel: () => void;
  isEditMode?: boolean;
}

const PBProposalForm: React.FC<PBProposalFormProps> = ({
  federations,
  categories,
  currencies,
  initialData,
  onSubmit,
  onCancel,
  isEditMode = false
}) => {
  const [formData, setFormData] = useState<PBProposalFormData>({
    title: '',
    description: '',
    requestedAmount: 0,
    currencyCode: 'ICN',
    category: '',
    federation: '',
    approvalThreshold: 50,
    quorumRequired: 25,
    votingPeriodDays: 7,
    additionalInfo: {},
    ...initialData
  });

  const [errors, setErrors] = useState<Record<string, string>>({});
  const [isSubmitting, setIsSubmitting] = useState<boolean>(false);
  const [additionalInfoKey, setAdditionalInfoKey] = useState<string>('');
  const [additionalInfoValue, setAdditionalInfoValue] = useState<string>('');
  const [submissionError, setSubmissionError] = useState<string | null>(null);
  const [files, setFiles] = useState<File[]>([]);

  useEffect(() => {
    if (initialData?.attachments) {
      setFiles(initialData.attachments);
    }
  }, [initialData]);

  const validate = (): boolean => {
    const newErrors: Record<string, string> = {};
    
    if (!formData.title.trim()) {
      newErrors.title = 'Title is required';
    } else if (formData.title.length < 5) {
      newErrors.title = 'Title must be at least 5 characters';
    }
    
    if (!formData.description.trim()) {
      newErrors.description = 'Description is required';
    } else if (formData.description.length < 20) {
      newErrors.description = 'Description must be at least 20 characters';
    }
    
    if (formData.requestedAmount <= 0) {
      newErrors.requestedAmount = 'Amount must be greater than 0';
    }
    
    if (!formData.federation) {
      newErrors.federation = 'Federation is required';
    }
    
    if (!formData.category) {
      newErrors.category = 'Category is required';
    }
    
    setErrors(newErrors);
    return Object.keys(newErrors).length === 0;
  };

  const handleChange = (event: React.ChangeEvent<HTMLInputElement | { name?: string; value: unknown }>) => {
    const { name, value } = event.target;
    if (name) {
      setFormData({
        ...formData,
        [name]: value
      });
    }
  };

  const handleNumberChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    const { name, value } = event.target;
    if (name) {
      setFormData({
        ...formData,
        [name]: parseFloat(value) || 0
      });
    }
  };

  const handleSliderChange = (name: string) => (_: Event, value: number | number[]) => {
    setFormData({
      ...formData,
      [name]: value as number
    });
  };

  const handleFileChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    if (event.target.files) {
      const newFiles = Array.from(event.target.files);
      setFiles([...files, ...newFiles]);
    }
  };

  const handleRemoveFile = (index: number) => {
    const newFiles = [...files];
    newFiles.splice(index, 1);
    setFiles(newFiles);
  };

  const handleAddAdditionalInfo = () => {
    if (additionalInfoKey.trim() && additionalInfoValue.trim()) {
      setFormData({
        ...formData,
        additionalInfo: {
          ...formData.additionalInfo,
          [additionalInfoKey]: additionalInfoValue
        }
      });
      setAdditionalInfoKey('');
      setAdditionalInfoValue('');
    }
  };

  const handleRemoveAdditionalInfo = (key: string) => {
    const newAdditionalInfo = { ...formData.additionalInfo };
    delete newAdditionalInfo[key];
    
    setFormData({
      ...formData,
      additionalInfo: newAdditionalInfo
    });
  };

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    
    if (!validate()) {
      return;
    }
    
    setIsSubmitting(true);
    setSubmissionError(null);
    
    try {
      await onSubmit({
        ...formData,
        attachments: files
      });
    } catch (error) {
      setSubmissionError(error instanceof Error ? error.message : 'An unexpected error occurred');
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <Paper elevation={2} sx={{ p: 3, borderRadius: 2 }}>
      <Typography variant="h5" gutterBottom>
        {isEditMode ? 'Edit Proposal' : 'Create New Proposal'}
      </Typography>
      
      <Divider sx={{ mb: 3 }} />
      
      {submissionError && (
        <Alert severity="error" sx={{ mb: 2 }}>
          {submissionError}
        </Alert>
      )}
      
      <form onSubmit={handleSubmit}>
        <Grid container spacing={3}>
          {/* Basic Information */}
          <Grid item xs={12}>
            <Typography variant="subtitle1" gutterBottom fontWeight="bold">
              Basic Information
            </Typography>
          </Grid>
          
          <Grid item xs={12}>
            <TextField
              fullWidth
              label="Title"
              name="title"
              value={formData.title}
              onChange={handleChange}
              error={!!errors.title}
              helperText={errors.title}
              required
              variant="outlined"
            />
          </Grid>
          
          <Grid item xs={12}>
            <TextField
              fullWidth
              label="Description"
              name="description"
              value={formData.description}
              onChange={handleChange}
              error={!!errors.description}
              helperText={errors.description}
              required
              multiline
              rows={4}
              variant="outlined"
            />
          </Grid>
          
          <Grid item xs={12} sm={6}>
            <TextField
              fullWidth
              label="Requested Amount"
              name="requestedAmount"
              type="number"
              value={formData.requestedAmount}
              onChange={handleNumberChange}
              error={!!errors.requestedAmount}
              helperText={errors.requestedAmount}
              required
              InputProps={{
                endAdornment: (
                  <InputAdornment position="end">
                    {formData.currencyCode}
                  </InputAdornment>
                )
              }}
              variant="outlined"
            />
          </Grid>
          
          <Grid item xs={12} sm={6}>
            <FormControl fullWidth variant="outlined">
              <InputLabel id="currency-label">Currency</InputLabel>
              <Select
                labelId="currency-label"
                label="Currency"
                name="currencyCode"
                value={formData.currencyCode}
                onChange={handleChange}
              >
                {currencies.map(currency => (
                  <MenuItem key={currency} value={currency}>
                    {currency}
                  </MenuItem>
                ))}
              </Select>
            </FormControl>
          </Grid>
          
          <Grid item xs={12} sm={6}>
            <FormControl fullWidth variant="outlined" error={!!errors.federation}>
              <InputLabel id="federation-label">Federation</InputLabel>
              <Select
                labelId="federation-label"
                label="Federation"
                name="federation"
                value={formData.federation}
                onChange={handleChange}
                required
              >
                {federations.map(federation => (
                  <MenuItem key={federation.id} value={federation.id}>
                    {federation.name}
                  </MenuItem>
                ))}
              </Select>
              {errors.federation && <FormHelperText>{errors.federation}</FormHelperText>}
            </FormControl>
          </Grid>
          
          <Grid item xs={12} sm={6}>
            <FormControl fullWidth variant="outlined" error={!!errors.category}>
              <InputLabel id="category-label">Category</InputLabel>
              <Select
                labelId="category-label"
                label="Category"
                name="category"
                value={formData.category}
                onChange={handleChange}
                required
              >
                {categories.map(category => (
                  <MenuItem key={category} value={category}>
                    {category}
                  </MenuItem>
                ))}
              </Select>
              {errors.category && <FormHelperText>{errors.category}</FormHelperText>}
            </FormControl>
          </Grid>
          
          {/* Voting Settings */}
          <Grid item xs={12} sx={{ mt: 2 }}>
            <Divider />
            <Typography variant="subtitle1" gutterBottom fontWeight="bold" sx={{ mt: 2 }}>
              Voting Settings
            </Typography>
          </Grid>
          
          <Grid item xs={12} sm={4}>
            <Typography id="approval-threshold-label" gutterBottom>
              Approval Threshold (%)
              <Tooltip title="Percentage of 'yes' votes required for approval">
                <InfoIcon fontSize="small" sx={{ ml: 1, verticalAlign: 'middle', color: 'text.secondary' }} />
              </Tooltip>
            </Typography>
            <Slider
              value={formData.approvalThreshold}
              onChange={handleSliderChange('approvalThreshold')}
              aria-labelledby="approval-threshold-label"
              valueLabelDisplay="auto"
              step={5}
              marks
              min={50}
              max={100}
            />
            <Typography variant="body2" color="text.secondary">
              Current: {formData.approvalThreshold}%
            </Typography>
          </Grid>
          
          <Grid item xs={12} sm={4}>
            <Typography id="quorum-label" gutterBottom>
              Quorum Required (%)
              <Tooltip title="Minimum participation required for the vote to be valid">
                <InfoIcon fontSize="small" sx={{ ml: 1, verticalAlign: 'middle', color: 'text.secondary' }} />
              </Tooltip>
            </Typography>
            <Slider
              value={formData.quorumRequired}
              onChange={handleSliderChange('quorumRequired')}
              aria-labelledby="quorum-label"
              valueLabelDisplay="auto"
              step={5}
              marks
              min={10}
              max={75}
            />
            <Typography variant="body2" color="text.secondary">
              Current: {formData.quorumRequired}%
            </Typography>
          </Grid>
          
          <Grid item xs={12} sm={4}>
            <Typography id="voting-period-label" gutterBottom>
              Voting Period (days)
              <Tooltip title="Duration of the voting period">
                <InfoIcon fontSize="small" sx={{ ml: 1, verticalAlign: 'middle', color: 'text.secondary' }} />
              </Tooltip>
            </Typography>
            <Slider
              value={formData.votingPeriodDays}
              onChange={handleSliderChange('votingPeriodDays')}
              aria-labelledby="voting-period-label"
              valueLabelDisplay="auto"
              step={1}
              marks
              min={1}
              max={30}
            />
            <Typography variant="body2" color="text.secondary">
              Current: {formData.votingPeriodDays} days
            </Typography>
          </Grid>
          
          {/* Attachments */}
          <Grid item xs={12} sx={{ mt: 2 }}>
            <Divider />
            <Typography variant="subtitle1" gutterBottom fontWeight="bold" sx={{ mt: 2 }}>
              Attachments
            </Typography>
          </Grid>
          
          <Grid item xs={12}>
            <Button
              variant="outlined"
              component="label"
              startIcon={<AddIcon />}
              sx={{ mb: 2 }}
            >
              Add Attachment
              <input
                type="file"
                hidden
                onChange={handleFileChange}
                multiple
              />
            </Button>
            
            {files.length > 0 && (
              <Box sx={{ mt: 2 }}>
                <Typography variant="body2" gutterBottom>
                  Attached Files:
                </Typography>
                <Box sx={{ display: 'flex', flexWrap: 'wrap', gap: 1 }}>
                  {files.map((file, index) => (
                    <Chip
                      key={index}
                      label={file.name}
                      onDelete={() => handleRemoveFile(index)}
                      size="medium"
                    />
                  ))}
                </Box>
              </Box>
            )}
          </Grid>
          
          {/* Additional Information */}
          <Grid item xs={12} sx={{ mt: 2 }}>
            <Divider />
            <Typography variant="subtitle1" gutterBottom fontWeight="bold" sx={{ mt: 2 }}>
              Additional Information (Optional)
            </Typography>
          </Grid>
          
          <Grid item xs={12} sm={5}>
            <TextField
              fullWidth
              label="Key"
              value={additionalInfoKey}
              onChange={(e) => setAdditionalInfoKey(e.target.value)}
              variant="outlined"
            />
          </Grid>
          
          <Grid item xs={12} sm={5}>
            <TextField
              fullWidth
              label="Value"
              value={additionalInfoValue}
              onChange={(e) => setAdditionalInfoValue(e.target.value)}
              variant="outlined"
            />
          </Grid>
          
          <Grid item xs={12} sm={2}>
            <Button
              fullWidth
              variant="outlined"
              onClick={handleAddAdditionalInfo}
              disabled={!additionalInfoKey.trim() || !additionalInfoValue.trim()}
              sx={{ height: '56px' }}
            >
              Add
            </Button>
          </Grid>
          
          {Object.keys(formData.additionalInfo || {}).length > 0 && (
            <Grid item xs={12}>
              <Box sx={{ mt: 1 }}>
                <Typography variant="body2" gutterBottom>
                  Additional Information:
                </Typography>
                <Box sx={{ display: 'flex', flexWrap: 'wrap', gap: 1 }}>
                  {Object.entries(formData.additionalInfo || {}).map(([key, value]) => (
                    <Chip
                      key={key}
                      label={`${key}: ${value}`}
                      onDelete={() => handleRemoveAdditionalInfo(key)}
                      size="medium"
                    />
                  ))}
                </Box>
              </Box>
            </Grid>
          )}
          
          {/* Form Buttons */}
          <Grid item xs={12} sx={{ mt: 2 }}>
            <Divider />
            <Box sx={{ display: 'flex', justifyContent: 'flex-end', mt: 3, gap: 2 }}>
              <Button
                variant="outlined"
                onClick={onCancel}
                disabled={isSubmitting}
              >
                Cancel
              </Button>
              <Button
                type="submit"
                variant="contained"
                color="primary"
                startIcon={isEditMode ? <SaveIcon /> : <CheckCircleIcon />}
                disabled={isSubmitting}
              >
                {isEditMode ? 'Save Changes' : 'Create Proposal'}
              </Button>
            </Box>
          </Grid>
        </Grid>
      </form>
    </Paper>
  );
};

export default PBProposalForm; 