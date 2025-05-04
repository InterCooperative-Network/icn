import React from 'react';
import ReactDOM from 'react-dom/client';
import { BrowserRouter } from 'react-router-dom';
import App from './App';
import './index.css';
import { CredentialProvider } from './contexts/CredentialContext';
import { DagSyncProvider } from './contexts/DagSyncContext';

ReactDOM.createRoot(document.getElementById('root')).render(
  <React.StrictMode>
    <BrowserRouter>
      <CredentialProvider>
        <DagSyncProvider>
          <App />
        </DagSyncProvider>
      </CredentialProvider>
    </BrowserRouter>
  </React.StrictMode>,
); 