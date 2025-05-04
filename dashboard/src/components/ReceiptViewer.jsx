import React, { useState } from 'react';
import { 
  ShieldCheckIcon, 
  ShieldExclamationIcon,
  ArrowTopRightOnSquareIcon 
} from '@heroicons/react/24/outline';
import { verifyExecutionReceipt } from '../utils/credentialVerifier';

export default function ReceiptViewer({ receipt, onVerify }) {
  const [verificationResult, setVerificationResult] = useState(null);
  const [showFullJson, setShowFullJson] = useState(false);
  
  // Extract key information from the receipt
  const { credentialSubject } = receipt.vc || {};
  const { 
    proposal_id: proposalId, 
    outcome, 
    dag_anchor: dagAnchor,
    federation_scope: federationScope,
    execution_timestamp: executionTimestamp,
    thread_id: threadId,
    resource_usage: resourceUsage
  } = credentialSubject || {};
  
  // Verify the receipt
  const handleVerify = async () => {
    // If a custom onVerify function is provided, use it
    if (onVerify) {
      const result = await onVerify(receipt);
      setVerificationResult(result);
      return;
    }
    
    // Otherwise, use the local verification
    const isValid = verifyExecutionReceipt(receipt);
    setVerificationResult({
      valid: isValid,
      message: isValid ? 'Receipt is valid' : 'Receipt verification failed'
    });
  };
  
  return (
    <div className="bg-white shadow overflow-hidden sm:rounded-lg">
      <div className="px-4 py-5 sm:px-6 flex justify-between items-center">
        <div>
          <h3 className="text-lg leading-6 font-medium text-gray-900">Execution Receipt</h3>
          <p className="mt-1 max-w-2xl text-sm text-gray-500">
            Verifiable credential for proposal execution
          </p>
        </div>
        <div>
          {verificationResult ? (
            <div className={`flex items-center ${verificationResult.valid ? 'text-green-600' : 'text-red-600'}`}>
              {verificationResult.valid ? (
                <ShieldCheckIcon className="h-6 w-6 mr-2" />
              ) : (
                <ShieldExclamationIcon className="h-6 w-6 mr-2" />
              )}
              {verificationResult.message}
            </div>
          ) : (
            <button
              onClick={handleVerify}
              className="btn btn-primary"
            >
              Verify Receipt
            </button>
          )}
        </div>
      </div>
      <div className="border-t border-gray-200">
        <dl>
          <div className="bg-gray-50 px-4 py-5 sm:grid sm:grid-cols-3 sm:gap-4 sm:px-6">
            <dt className="text-sm font-medium text-gray-500">Proposal ID</dt>
            <dd className="mt-1 text-sm text-gray-900 sm:mt-0 sm:col-span-2">{proposalId}</dd>
          </div>
          <div className="bg-white px-4 py-5 sm:grid sm:grid-cols-3 sm:gap-4 sm:px-6">
            <dt className="text-sm font-medium text-gray-500">Outcome</dt>
            <dd className="mt-1 text-sm text-gray-900 sm:mt-0 sm:col-span-2">{outcome}</dd>
          </div>
          <div className="bg-gray-50 px-4 py-5 sm:grid sm:grid-cols-3 sm:gap-4 sm:px-6">
            <dt className="text-sm font-medium text-gray-500">Federation Scope</dt>
            <dd className="mt-1 text-sm text-gray-900 sm:mt-0 sm:col-span-2">{federationScope}</dd>
          </div>
          <div className="bg-white px-4 py-5 sm:grid sm:grid-cols-3 sm:gap-4 sm:px-6">
            <dt className="text-sm font-medium text-gray-500">Execution Time</dt>
            <dd className="mt-1 text-sm text-gray-900 sm:mt-0 sm:col-span-2">
              {executionTimestamp ? new Date(executionTimestamp).toLocaleString() : 'Unknown'}
            </dd>
          </div>
          <div className="bg-gray-50 px-4 py-5 sm:grid sm:grid-cols-3 sm:gap-4 sm:px-6">
            <dt className="text-sm font-medium text-gray-500">DAG Anchor</dt>
            <dd className="mt-1 text-sm text-gray-900 sm:mt-0 sm:col-span-2 flex items-center">
              <span className="font-mono">{dagAnchor}</span>
              <a
                href={`/dag/${dagAnchor}`}
                target="_blank"
                rel="noopener noreferrer"
                className="ml-2 text-agora-blue hover:underline"
              >
                <ArrowTopRightOnSquareIcon className="h-4 w-4" />
              </a>
            </dd>
          </div>
          {threadId && (
            <div className="bg-white px-4 py-5 sm:grid sm:grid-cols-3 sm:gap-4 sm:px-6">
              <dt className="text-sm font-medium text-gray-500">Thread ID</dt>
              <dd className="mt-1 text-sm text-gray-900 sm:mt-0 sm:col-span-2 flex items-center">
                <span>{threadId}</span>
                <a
                  href={`/threads/${threadId}`}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="ml-2 text-agora-blue hover:underline"
                >
                  <ArrowTopRightOnSquareIcon className="h-4 w-4" />
                </a>
              </dd>
            </div>
          )}
          <div className="bg-gray-50 px-4 py-5 sm:grid sm:grid-cols-3 sm:gap-4 sm:px-6">
            <dt className="text-sm font-medium text-gray-500">Resource Usage</dt>
            <dd className="mt-1 text-sm text-gray-900 sm:mt-0 sm:col-span-2">
              <ul className="border border-gray-200 rounded-md divide-y divide-gray-200">
                {resourceUsage && Object.entries(resourceUsage).map(([resource, amount]) => (
                  <li key={resource} className="pl-3 pr-4 py-3 flex items-center justify-between text-sm">
                    <div className="w-0 flex-1 flex items-center">
                      <span className="ml-2 flex-1 w-0 truncate">{resource}</span>
                    </div>
                    <div className="ml-4 flex-shrink-0 font-mono">
                      {amount}
                    </div>
                  </li>
                ))}
              </ul>
            </dd>
          </div>
          <div className="bg-white px-4 py-5 sm:grid sm:grid-cols-3 sm:gap-4 sm:px-6">
            <dt className="text-sm font-medium text-gray-500">
              <button
                onClick={() => setShowFullJson(!showFullJson)}
                className="text-agora-blue hover:underline"
              >
                {showFullJson ? 'Hide' : 'Show'} Full Receipt
              </button>
            </dt>
            <dd className="mt-1 text-sm text-gray-900 sm:mt-0 sm:col-span-2">
              {showFullJson && (
                <pre className="bg-gray-100 p-4 rounded-md overflow-auto max-h-96 text-xs">
                  {JSON.stringify(receipt, null, 2)}
                </pre>
              )}
            </dd>
          </div>
        </dl>
      </div>
    </div>
  );
} 