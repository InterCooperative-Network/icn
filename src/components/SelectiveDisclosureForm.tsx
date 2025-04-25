import React, { useState, useEffect } from 'react';
import { 
  WalletCredential, 
  SelectiveDisclosureOptions, 
  SelectiveDisclosure,
  createSelectiveDisclosure
} from '../../packages/credential-utils';
import { Button, Checkbox, FormGroup, Input, Label, Card, CardBody, CardHeader } from 'reactstrap';

interface SelectiveDisclosureFormProps {
  credential: WalletCredential;
  onDisclose: (disclosure: SelectiveDisclosure) => void;
  onCancel: () => void;
}

export const SelectiveDisclosureForm: React.FC<SelectiveDisclosureFormProps> = ({
  credential,
  onDisclose,
  onCancel
}) => {
  const [availableFields, setAvailableFields] = useState<string[]>([]);
  const [selectedFields, setSelectedFields] = useState<string[]>([]);
  const [proofType, setProofType] = useState<'redaction' | 'zk'>('redaction');
  const [reason, setReason] = useState<string>('');
  const [showAdvanced, setShowAdvanced] = useState<boolean>(false);
  
  // On component mount, extract all fields from the credential
  useEffect(() => {
    // Helper function to get all fields from an object using dot notation
    const getAllFields = (obj: any, prefix = ''): string[] => {
      if (!obj || typeof obj !== 'object') return [];
      
      return Object.keys(obj).reduce((fields: string[], key) => {
        const newPrefix = prefix ? `${prefix}.${key}` : key;
        
        if (obj[key] && typeof obj[key] === 'object' && !Array.isArray(obj[key])) {
          return [...fields, newPrefix, ...getAllFields(obj[key], newPrefix)];
        }
        
        return [...fields, newPrefix];
      }, []);
    };
    
    // Get fields with special handling for nested fields
    const fields = getAllFields(credential)
      .filter(field => {
        // Filter out technical fields or fields that must be included
        return !['@context', 'id', 'type', 'issuer', 'issuanceDate'].includes(field);
      })
      // Sort by hierarchy and alphabetically
      .sort((a, b) => {
        const aDepth = a.split('.').length;
        const bDepth = b.split('.').length;
        if (aDepth !== bDepth) return aDepth - bDepth;
        return a.localeCompare(b);
      });
      
    setAvailableFields(fields);
    // Initially select all fields
    setSelectedFields(fields);
  }, [credential]);
  
  const toggleField = (field: string) => {
    if (selectedFields.includes(field)) {
      setSelectedFields(selectedFields.filter(f => f !== field));
    } else {
      setSelectedFields([...selectedFields, field]);
    }
  };
  
  const toggleAllFields = () => {
    if (selectedFields.length === availableFields.length) {
      setSelectedFields([]);
    } else {
      setSelectedFields([...availableFields]);
    }
  };
  
  const handleCreate = () => {
    const options: SelectiveDisclosureOptions = {
      includeFields: selectedFields,
      excludeFields: availableFields.filter(field => !selectedFields.includes(field)),
      proofType,
      reason: reason || undefined
    };
    
    const disclosure = createSelectiveDisclosure(credential, options);
    onDisclose(disclosure);
  };
  
  // Group fields by parent object for better UX
  const groupedFields = availableFields.reduce((groups: Record<string, string[]>, field) => {
    const parentKey = field.split('.')[0];
    if (!groups[parentKey]) groups[parentKey] = [];
    groups[parentKey].push(field);
    return groups;
  }, {});
  
  return (
    <Card className="mb-4">
      <CardHeader>
        <h5>Create Selective Disclosure</h5>
        <p className="text-muted">
          Choose which credential fields to include in your disclosure
        </p>
      </CardHeader>
      
      <CardBody>
        <div className="mb-3">
          <div className="d-flex justify-content-between mb-2">
            <h6>Select Fields to Include</h6>
            <Button 
              color="link" 
              size="sm" 
              onClick={toggleAllFields}
            >
              {selectedFields.length === availableFields.length ? 'Deselect All' : 'Select All'}
            </Button>
          </div>
          
          {Object.entries(groupedFields).map(([group, fields]) => (
            <div key={group} className="mb-3">
              <h6 className="text-capitalize">{group}</h6>
              <div className="ps-3">
                {fields.map(field => (
                  <FormGroup check key={field} className="mb-1">
                    <Input
                      type="checkbox"
                      id={`field-${field}`}
                      checked={selectedFields.includes(field)}
                      onChange={() => toggleField(field)}
                    />
                    <Label check for={`field-${field}`}>
                      {field.split('.').slice(1).join('.')}
                    </Label>
                  </FormGroup>
                ))}
              </div>
            </div>
          ))}
        </div>
        
        <Button 
          color="link" 
          className="mb-3" 
          onClick={() => setShowAdvanced(!showAdvanced)}
        >
          {showAdvanced ? 'Hide Advanced Options' : 'Show Advanced Options'}
        </Button>
        
        {showAdvanced && (
          <div className="mb-3">
            <FormGroup>
              <Label for="proofType">Proof Type</Label>
              <Input
                type="select"
                id="proofType"
                value={proofType}
                onChange={(e) => setProofType(e.target.value as 'redaction' | 'zk')}
              >
                <option value="redaction">Simple Redaction</option>
                <option value="zk" disabled>Zero-Knowledge Proof (Coming Soon)</option>
              </Input>
            </FormGroup>
            
            <FormGroup>
              <Label for="reason">Disclosure Reason (Optional)</Label>
              <Input
                type="text"
                id="reason"
                placeholder="e.g., Proof of governance participation"
                value={reason}
                onChange={(e) => setReason(e.target.value)}
              />
            </FormGroup>
          </div>
        )}
        
        <div className="d-flex justify-content-end gap-2">
          <Button color="secondary" onClick={onCancel}>
            Cancel
          </Button>
          <Button 
            color="primary" 
            onClick={handleCreate}
            disabled={selectedFields.length === 0}
          >
            Create Disclosure
          </Button>
        </div>
      </CardBody>
    </Card>
  );
}; 