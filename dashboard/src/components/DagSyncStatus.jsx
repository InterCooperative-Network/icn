import React from 'react';
import { 
  ArrowPathIcon, 
  PauseIcon, 
  PlayIcon, 
  ExclamationTriangleIcon,
  ClockIcon
} from '@heroicons/react/24/outline';
import { useDagSync } from '../contexts/DagSyncContext';

export default function DagSyncStatus() {
  const {
    latestAnchor,
    syncStatus,
    lastSyncTime,
    error,
    isPaused,
    togglePause,
    syncNow
  } = useDagSync();

  // Format the time ago
  const getTimeAgo = () => {
    if (!lastSyncTime) return 'Never';
    
    const seconds = Math.floor((new Date() - lastSyncTime) / 1000);
    
    if (seconds < 60) return `${seconds}s ago`;
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
    return `${Math.floor(seconds / 3600)}h ago`;
  };

  return (
    <div className="flex items-center space-x-2 text-sm">
      {/* Sync Status Icon */}
      {syncStatus === 'syncing' ? (
        <ArrowPathIcon className="h-4 w-4 text-blue-500 animate-spin" />
      ) : isPaused ? (
        <PauseIcon className="h-4 w-4 text-yellow-500" />
      ) : error ? (
        <ExclamationTriangleIcon className="h-4 w-4 text-red-500" />
      ) : (
        <ClockIcon className="h-4 w-4 text-green-500" />
      )}
      
      {/* CID and Last Sync */}
      <div className="hidden md:block">
        <span className="text-gray-600">
          DAG Root: 
          <span className="font-mono text-xs ml-1">
            {latestAnchor ? `${latestAnchor.cid.substring(0, 8)}...` : 'None'}
          </span>
        </span>
      </div>
      
      <span className="text-gray-500 text-xs">
        Last sync: {getTimeAgo()}
      </span>
      
      {/* Error Message */}
      {error && (
        <span className="text-red-500 text-xs">{error}</span>
      )}
      
      {/* Controls */}
      <div className="flex space-x-1">
        <button
          onClick={syncNow}
          disabled={isPaused || syncStatus === 'syncing'}
          className="p-1 rounded-full hover:bg-gray-200 disabled:opacity-50"
          title="Sync now"
        >
          <ArrowPathIcon className="h-4 w-4 text-gray-700" />
        </button>
        
        <button
          onClick={togglePause}
          className="p-1 rounded-full hover:bg-gray-200"
          title={isPaused ? "Resume sync" : "Pause sync"}
        >
          {isPaused ? (
            <PlayIcon className="h-4 w-4 text-gray-700" />
          ) : (
            <PauseIcon className="h-4 w-4 text-gray-700" />
          )}
        </button>
      </div>
    </div>
  );
} 