import { Routes, Route } from 'react-router-dom';
import Layout from './components/Layout';
import Dashboard from './pages/Dashboard';
import ProposalList from './pages/ProposalListPage';
import ProposalDetail from './pages/ProposalDetailPage';
import { useCredentials } from './contexts/CredentialContext';

function App() {
  const { isAuthenticated, loading } = useCredentials();

  if (loading) {
    return (
      <div className="h-screen flex items-center justify-center">
        <div className="animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-agora-blue"></div>
      </div>
    );
  }

  return (
    <Layout>
      <Routes>
        <Route path="/" element={<Dashboard />} />
        <Route path="/proposals" element={<ProposalList />} />
        <Route path="/proposals/:id" element={<ProposalDetail />} />
      </Routes>
    </Layout>
  );
}

export default App; 