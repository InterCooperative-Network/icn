import Dexie from 'dexie';

// Define our database
export const db = new Dexie('AgoraNetDashboard');

// Define database schema
db.version(1).stores({
  credentials: 'id, subject, issuer, type, issuanceDate',
  proposals: 'id, title, status, creatorDid, federationId',
  threads: 'id, title, proposalId',
  receipts: 'id, proposalId, threadId'
});

// Export database instance
export default db; 