import React, { useState, useEffect, useRef } from 'react';
import { 
  DocumentCheckIcon, 
  ExclamationCircleIcon,
  ArrowPathIcon,
  CheckCircleIcon
} from '@heroicons/react/24/outline';
import { credentialApi } from '../services/runtimeApi';
import { useDagSync } from '../contexts/DagSyncContext';

export default function ReceiptMonitor({ proposalId, onReceiptFound }) {
  const { syncNow } = useDagSync();
  const [status, setStatus] = useState('idle'); // 'idle', 'polling', 'found', 'error'
  const [receipt, setReceipt] = useState(null);
  const [error, setError] = useState(null);
  const [pollingCount, setPollingCount] = useState(0);
  const maxPolls = 30; // 60 seconds at 2 second intervals
  const timerRef = useRef(null);
  
  // Start monitoring when component mounts
  useEffect(() => {
    const startMonitoring = async () => {
      try {
        // Check if receipt already exists first
        const receipts = await credentialApi.getReceiptsForProposal(proposalId);
        
        if (receipts && receipts.length > 0) {
          setReceipt(receipts[0]);
          setStatus('found');
          
          if (onReceiptFound) {
            onReceiptFound(receipts[0]);
          }
          
          return; // Already found, no need to poll
        }
        
        // Start polling
        startPolling();
      } catch (err) {
        console.error('Error checking for existing receipts:', err);
      }
    };
    
    startMonitoring();
    
    return () => {
      // Clean up
      if (timerRef.current) {
        clearTimeout(timerRef.current);
      }
    };
  }, [proposalId, onReceiptFound]);
  
  // Poll for receipt
  const startPolling = () => {
    setStatus('polling');
    setPollingCount(0);
    pollForReceipt();
  };
  
  const pollForReceipt = async () => {
    if (pollingCount >= maxPolls) {
      setStatus('idle');
      return;
    }
    
    try {
      const receipts = await credentialApi.getReceiptsForProposal(proposalId);
      
      if (receipts && receipts.length > 0) {
        setReceipt(receipts[0]);
        setStatus('found');
        
        // Trigger DAG sync to update any proposal status changes
        syncNow();
        
        if (onReceiptFound) {
          onReceiptFound(receipts[0]);
        }
        
        return;
      }
      
      // Not found yet, continue polling
      setPollingCount(prev => prev + 1);
      timerRef.current = setTimeout(pollForReceipt, 2000); // Poll every 2 seconds
    } catch (err) {
      console.error('Error polling for receipt:', err);
      setError('Failed to check for execution receipt');
      setStatus('error');
    }
  };
  
  // Manually restart polling
  const handleRestartPolling = () => {
    setError(null);
    startPolling();
  };
  
  if (status === 'found' && receipt) {
    return (
      <div className="bg-green-50 rounded-md p-4 border border-green-200">
        <div className="flex">
          <div className="flex-shrink-0">
            <CheckCircleIcon className="h-5 w-5 text-green-400" aria-hidden="true" />
          </div>
          <div className="ml-3">
            <h3 className="text-sm font-medium text-green-800">Execution Receipt Available</h3>
            <div className="mt-2 text-sm text-green-700">
              <p>The proposal has been executed and a receipt has been generated.</p>
            </div>
          </div>
        </div>
      </div>
    );
  }
  
  if (status === 'error') {
    return (
      <div className="bg-red-50 rounded-md p-4 border border-red-200">
        <div className="flex">
          <div className="flex-shrink-0">
            <ExclamationCircleIcon className="h-5 w-5 text-red-400" aria-hidden="true" />
          </div>
          <div className="ml-3">
            <h3 className="text-sm font-medium text-red-800">Error Monitoring Receipt</h3>
            <div className="mt-2 text-sm text-red-700">
              <p>{error || 'Failed to check for execution receipt'}</p>
            </div>
            <div className="mt-4">
              <button
                type="button"
                onClick={handleRestartPolling}
                className="rounded-md bg-red-50 px-2 py-1.5 text-sm font-medium text-red-800 hover:bg-red-100 focus:outline-none"
              >
                Try Again
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }
  
  if (status === 'polling') {
    return (
      <div className="bg-blue-50 rounded-md p-4 border border-blue-200">
        <div className="flex">
          <div className="flex-shrink-0">
            <ArrowPathIcon className="h-5 w-5 text-blue-400 animate-spin" aria-hidden="true" />
          </div>
          <div className="ml-3">
            <h3 className="text-sm font-medium text-blue-800">Monitoring for Execution Receipt</h3>
            <div className="mt-2 text-sm text-blue-700">
              <p>Waiting for the proposal to be executed and a receipt to be generated.</p>
              <div className="mt-1 text-xs text-blue-500">
                Attempted {pollingCount} of {maxPolls} checks...
              </div>
            </div>
          </div>
        </div>
      </div>
    );
  }
  
  return (
    <div className="bg-gray-50 rounded-md p-4 border border-gray-200">
      <div className="flex">
        <div className="flex-shrink-0">
          <DocumentCheckIcon className="h-5 w-5 text-gray-400" aria-hidden="true" />
        </div>
        <div className="ml-3">
          <h3 className="text-sm font-medium text-gray-800">Receipt Monitoring</h3>
          <div className="mt-2 text-sm text-gray-700">
            <p>No execution receipt is available yet.</p>
          </div>
          <div className="mt-4">
            <button
              type="button"
              onClick={handleRestartPolling}
              className="rounded-md bg-white px-2.5 py-1.5 text-sm font-medium text-gray-700 shadow-sm ring-1 ring-inset ring-gray-300 hover:bg-gray-50"
            >
              Check for Receipt
            </button>
          </div>
        </div>
      </div>
    </div>
  );
} 