const express = require('express');
const cors = require('cors');
const bodyParser = require('body-parser');
const app = express();

// Middleware
app.use(cors());
app.use(bodyParser.json());

// Test data
const threads = [
  {
    id: 'thread1',
    title: 'Discussion on Treasury Proposal',
    proposal_id: 'proposal1',
    topic: 'governance',
    author: 'did:icn:alice',
    created_at: '2023-05-01T12:00:00Z',
    post_count: 3,
    credential_links: []
  },
  {
    id: 'thread2',
    title: 'Community Guidelines Update',
    proposal_id: 'proposal2',
    topic: 'community',
    author: 'did:icn:bob',
    created_at: '2023-05-02T10:00:00Z',
    post_count: 5,
    credential_links: []
  }
];

const posts = {
  'thread1': [
    {
      id: 'post1',
      thread_id: 'thread1',
      content: 'I think this treasury proposal makes sense.',
      author: 'did:icn:alice',
      created_at: '2023-05-01T12:00:00Z'
    },
    {
      id: 'post2',
      thread_id: 'thread1',
      content: 'I agree, the allocation seems fair.',
      author: 'did:icn:bob',
      created_at: '2023-05-01T12:30:00Z'
    },
    {
      id: 'post3',
      thread_id: 'thread1',
      content: 'Let\'s finalize and vote on this.',
      author: 'did:icn:charlie',
      created_at: '2023-05-01T13:00:00Z'
    }
  ],
  'thread2': [
    {
      id: 'post4',
      thread_id: 'thread2',
      content: 'The new guidelines need more clarity.',
      author: 'did:icn:bob',
      created_at: '2023-05-02T10:00:00Z'
    },
    {
      id: 'post5',
      thread_id: 'thread2',
      content: 'I suggest adding examples.',
      author: 'did:icn:alice',
      created_at: '2023-05-02T10:15:00Z'
    }
  ]
};

const credential_links = [];

// Routes
app.get('/api/health', (req, res) => {
  res.json({ status: 'ok' });
});

app.get('/api/threads', (req, res) => {
  const { proposal_id, topic } = req.query;
  
  let filteredThreads = threads;
  
  if (proposal_id) {
    filteredThreads = filteredThreads.filter(t => t.proposal_id === proposal_id);
  }
  
  if (topic) {
    filteredThreads = filteredThreads.filter(t => t.topic === topic);
  }
  
  res.json(filteredThreads);
});

app.get('/api/threads/:id', (req, res) => {
  const thread = threads.find(t => t.id === req.params.id);
  
  if (!thread) {
    return res.status(404).json({ error: 'Thread not found' });
  }
  
  // Get posts for this thread
  const threadPosts = posts[thread.id] || [];
  
  // Get credential links for this thread
  const threadCredentialLinks = credential_links.filter(cl => cl.thread_id === thread.id);
  
  // Combine data
  const threadDetail = {
    ...thread,
    posts: threadPosts,
    credential_links: threadCredentialLinks
  };
  
  res.json(threadDetail);
});

app.post('/api/threads/credential-link', (req, res) => {
  const { thread_id, credential } = req.body;
  
  // Validate request
  if (!thread_id || !credential) {
    return res.status(400).json({ error: 'Missing required fields' });
  }
  
  // Check if thread exists
  const thread = threads.find(t => t.id === thread_id);
  if (!thread) {
    return res.status(404).json({ error: 'Thread not found' });
  }
  
  // Create credential link
  const credentialLink = {
    id: `link-${Date.now()}`,
    thread_id,
    credential_id: credential.id || `cred-${Date.now()}`,
    credential_type: (credential.credential_type || credential.type || ['VerifiableCredential'])[0],
    issuer: credential.issuer || 'unknown',
    subject: credential.credentialSubject?.id || 'unknown',
    created_at: new Date().toISOString()
  };
  
  credential_links.push(credentialLink);
  
  // Add to thread's credential_links array for thread summaries
  const threadIndex = threads.findIndex(t => t.id === thread_id);
  if (threadIndex !== -1) {
    threads[threadIndex].credential_links.push(credentialLink);
  }
  
  res.status(201).json(credentialLink);
});

app.get('/api/threads/:id/credential-links', (req, res) => {
  const threadId = req.params.id;
  
  // Check if thread exists
  const thread = threads.find(t => t.id === threadId);
  if (!thread) {
    return res.status(404).json({ error: 'Thread not found' });
  }
  
  // Get credential links for this thread
  const threadCredentialLinks = credential_links.filter(cl => cl.thread_id === threadId);
  
  res.json(threadCredentialLinks);
});

app.post('/api/proposals/:id/events', (req, res) => {
  const proposalId = req.params.id;
  const { event_type, details, timestamp } = req.body;
  
  // Validate request
  if (!event_type) {
    return res.status(400).json({ error: 'Missing event_type' });
  }
  
  // Just log the event (in a real system, this would update state and trigger notifications)
  console.log(`[AgoraNet] New event for proposal ${proposalId}: ${event_type}`);
  
  res.status(200).json({
    success: true,
    proposal_id: proposalId,
    event_type,
    timestamp: timestamp || new Date().toISOString()
  });
});

// Start server
const PORT = process.env.PORT || 8080;
app.listen(PORT, () => {
  console.log(`Mock AgoraNet server running on port ${PORT}`);
  console.log(`Health check: http://localhost:${PORT}/api/health`);
});

// Handle graceful shutdown
process.on('SIGINT', () => {
  console.log('Shutting down mock AgoraNet server');
  process.exit(0);
}); 