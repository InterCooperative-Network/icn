import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Mock data for API endpoints
const mockProposals = [
  {
    id: "proposal-001",
    title: "Community Governance Framework",
    description: "Establish a comprehensive governance framework for our cooperative community.",
    status: "active",
    creatorDid: "did:icn:user:alice123",
    federationId: "fed:icn:community-alpha",
    createdAt: new Date().toISOString(),
    votesFor: 3,
    votesAgainst: 1,
    votesAbstain: 0,
    threadId: "thread-001"
  },
  {
    id: "proposal-002",
    title: "Resource Allocation Process",
    description: "Define a participatory process for resource allocation decisions.",
    status: "executed",
    creatorDid: "did:icn:user:bob456",
    federationId: "fed:icn:community-alpha",
    createdAt: new Date(Date.now() - 86400000).toISOString(),
    votesFor: 5,
    votesAgainst: 0,
    votesAbstain: 1
  },
  {
    id: "proposal-003",
    title: "Community Project Funding",
    description: "Allocate funds for community-driven initiatives and projects.",
    status: "deliberating",
    creatorDid: "did:icn:user:charlie789",
    federationId: "fed:icn:community-beta",
    createdAt: new Date(Date.now() - 172800000).toISOString(),
    votesFor: 2,
    votesAgainst: 2,
    votesAbstain: 1,
    threadId: "thread-002"
  }
];

const mockDagAnchor = {
  cid: "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
  timestamp: new Date().toISOString(),
  height: 42,
  previousCid: "bafybeihcqkmk7dqtvcfzosjv3uqdilid6z2qpdmxmzcecrerh5zblhwbbm"
};

const mockReceipts = {
  "proposal-002": {
    id: "receipt-002",
    proposalId: "proposal-002", 
    jwt: "eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJwcm9wb3NhbC0wMDIiLCJpc3MiOiJkaWQ6aWNuOm5vZGU6ZXhlY3V0b3IiLCJpYXQiOjE2ODMxMjM0NTYsImV4cCI6MTY4NDMzMzQ1NiwidmMiOnsiQGNvbnRleHQiOlsiaHR0cHM6Ly93d3cudzMub3JnLzIwMTgvY3JlZGVudGlhbHMvdjEiXSwidHlwZSI6WyJWZXJpZmlhYmxlQ3JlZGVudGlhbCIsIlByb3Bvc2FsRXhlY3V0aW9uUmVjZWlwdCJdLCJpc3N1YW5jZURhdGUiOiIyMDIzLTA1LTAzVDEyOjMwOjQ1WiIsImNyZWRlbnRpYWxTdWJqZWN0Ijp7ImlkIjoicHJvcG9zYWwtMDAyIiwidGl0bGUiOiJSZXNvdXJjZSBBbGxvY2F0aW9uIFByb2Nlc3MiLCJleGVjdXRpb25UaW1lc3RhbXAiOiIyMDIzLTA1LTAzVDEyOjMwOjQ1WiIsInN0YXR1cyI6ImV4ZWN1dGVkIiwiZXhlY3V0b3IiOiJkaWQ6aWNuOm5vZGU6ZXhlY3V0b3IiLCJkYWdSb290Q2lkIjoiYmFmeWJlaWhjeWZidDN5emVrM21yeGd6ZmJvZHF6NXk2bWh1dHhnbGJ3bWo0anQycGFtc25ha3JybnkifX19.AoexDCUSQPMxqDnr2HoKrT3QbKPn3xZdKvk2mGfUxuVtTnpVyHnDYz5uCgVy3PqLDFtRXf2zNGy5JhAA3AqgCw"
  }
};

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': '/src',
    },
  },
  server: {
    proxy: {
      '/api/runtime/proposals': {
        target: 'http://localhost:5173',
        changeOrigin: true,
        configure: (proxy, options) => {
          proxy.on('proxyReq', (proxyReq, req, res) => {
            // Intercept and handle the request ourselves
            res.writeHead(200, { 'Content-Type': 'application/json' });
            
            // Get specific proposal by ID
            const match = req.url.match(/\/api\/runtime\/proposals\/([^/]+)/);
            if (match) {
              const proposalId = match[1];
              const proposal = mockProposals.find(p => p.id === proposalId);
              if (proposal) {
                res.end(JSON.stringify(proposal));
              } else {
                res.writeHead(404, { 'Content-Type': 'application/json' });
                res.end(JSON.stringify({ error: "Proposal not found" }));
              }
              return;
            }
            
            // List all proposals
            res.end(JSON.stringify(mockProposals));
          });
        }
      },
      '/api/runtime/dag/anchors': {
        target: 'http://localhost:5173',
        changeOrigin: true,
        configure: (proxy, options) => {
          proxy.on('proxyReq', (proxyReq, req, res) => {
            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify({
              latestAnchor: mockDagAnchor,
              updatedProposals: []
            }));
          });
        }
      },
      '/api/runtime/receipts': {
        target: 'http://localhost:5173',
        changeOrigin: true,
        configure: (proxy, options) => {
          proxy.on('proxyReq', (proxyReq, req, res) => {
            // Parse proposal ID from query string
            const url = new URL(req.url, 'http://localhost');
            const proposalId = url.searchParams.get('proposalId');
            const receipt = proposalId && mockReceipts[proposalId];
            
            res.writeHead(200, { 'Content-Type': 'application/json' });
            if (receipt) {
              res.end(JSON.stringify([receipt]));
            } else {
              res.end(JSON.stringify([]));
            }
          });
        }
      },
      '/api/agoranet/threads': {
        target: 'http://localhost:5173',
        changeOrigin: true,
        configure: (proxy, options) => {
          proxy.on('proxyReq', (proxyReq, req, res) => {
            // Get specific thread by ID
            const match = req.url.match(/\/api\/agoranet\/threads\/([^/]+)/);
            if (match) {
              const threadId = match[1];
              res.writeHead(200, { 'Content-Type': 'application/json' });
              res.end(JSON.stringify({
                id: threadId,
                title: threadId === "thread-001" ? "Community Governance Discussion" : "Resource Allocation Discussion",
                posts: [
                  { id: "post-1", author: "did:icn:user:alice123", content: "This is a discussion post", timestamp: new Date().toISOString() }
                ]
              }));
              return;
            }
            
            // List all threads
            res.writeHead(200, { 'Content-Type': 'application/json' });
            res.end(JSON.stringify([
              { id: "thread-001", title: "Community Governance Discussion", postCount: 12 },
              { id: "thread-002", title: "Resource Allocation Discussion", postCount: 8 }
            ]));
          });
        }
      }
    }
  }
}); 