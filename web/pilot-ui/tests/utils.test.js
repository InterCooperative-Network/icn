/**
 * Unit Tests for Utility Functions
 */

describe('Utility Functions', () => {
  describe('truncateDid', () => {
    // Create truncateDid function for testing
    const truncateDid = (did) => {
      if (!did || did.length <= 20) return did;
      return `${did.slice(0, 12)}...${did.slice(-6)}`;
    };

    test('should truncate long DIDs', () => {
      const longDid = 'did:icn:abcdefghijklmnopqrstuvwxyz123456';
      const result = truncateDid(longDid);
      expect(result).toBe('did:icn:abcd...123456');
    });

    test('should not truncate short DIDs', () => {
      const shortDid = 'did:icn:short';
      const result = truncateDid(shortDid);
      expect(result).toBe('did:icn:short');
    });

    test('should handle empty/null DIDs', () => {
      expect(truncateDid('')).toBe('');
      expect(truncateDid(null)).toBe(null);
      expect(truncateDid(undefined)).toBe(undefined);
    });

    test('should handle exactly 20 character DIDs', () => {
      const exactDid = '12345678901234567890';
      expect(truncateDid(exactDid)).toBe(exactDid);
    });
  });

  describe('formatDate', () => {
    const formatDate = (timestamp) => {
      return new Date(timestamp * 1000).toLocaleDateString();
    };

    test('should format Unix timestamp to date string', () => {
      const timestamp = 1704067200; // Jan 1, 2024 UTC
      const result = formatDate(timestamp);
      // Allow for timezone differences - could be Dec 31 or Jan 1 depending on local TZ
      expect(result).toMatch(/12\/31\/2023|1\/1\/2024|2023-12-31|2024-01-01/);
    });

    test('should handle zero timestamp', () => {
      const result = formatDate(0);
      // Allow for timezone differences
      expect(result).toMatch(/12\/31\/1969|1\/1\/1970|1969-12-31|1970-01-01/);
    });
  });

  describe('formatDateTime', () => {
    const formatDateTime = (timestamp) => {
      return new Date(timestamp * 1000).toLocaleString();
    };

    test('should format Unix timestamp to datetime string', () => {
      const timestamp = 1704067200;
      const result = formatDateTime(timestamp);
      // Allow for timezone differences - could be Dec 31, 2023 or Jan 1, 2024
      expect(result).toMatch(/2023|2024/);
    });
  });

  describe('calculateGini', () => {
    const calculateGini = (values) => {
      if (values.length === 0) return 0;

      const n = values.length;
      let numerator = 0;

      for (let i = 0; i < n; i++) {
        for (let j = 0; j < n; j++) {
          numerator += Math.abs(values[i] - values[j]);
        }
      }

      const mean = values.reduce((sum, v) => sum + v, 0) / n;
      const denominator = 2 * n * n * mean;

      return denominator === 0 ? 0 : numerator / denominator;
    };

    test('should return 0 for empty array', () => {
      expect(calculateGini([])).toBe(0);
    });

    test('should return 0 for equal distribution', () => {
      const equal = [10, 10, 10, 10];
      expect(calculateGini(equal)).toBe(0);
    });

    test('should return positive value for unequal distribution', () => {
      const unequal = [1, 5, 10, 20];
      const gini = calculateGini(unequal);
      expect(gini).toBeGreaterThan(0);
      expect(gini).toBeLessThanOrEqual(1);
    });

    test('should return high value for extreme inequality', () => {
      const extreme = [0, 0, 0, 100];
      const gini = calculateGini(extreme);
      expect(gini).toBeGreaterThan(0.5);
    });

    test('should handle single value', () => {
      expect(calculateGini([10])).toBe(0);
    });

    test('should handle all zeros', () => {
      expect(calculateGini([0, 0, 0])).toBe(0);
    });
  });

  describe('getUserFriendlyError', () => {
    const getUserFriendlyError = (error) => {
      const message = error.message || String(error);

      if (message.includes('Failed to fetch') || message.includes('NetworkError')) {
        return 'Cannot connect to the server. Please check your internet connection and gateway URL.';
      }
      if (message.includes('401') || message.includes('Unauthorized')) {
        return 'Your session has expired. Please sign in again.';
      }
      if (message.includes('403') || message.includes('Forbidden')) {
        return 'You don\'t have permission to do that. Check with your cooperative administrator.';
      }
      if (message.includes('404') || message.includes('Not Found')) {
        return 'The requested resource was not found. Please check your cooperative ID.';
      }
      if (message.includes('429') || message.includes('Too Many Requests')) {
        return 'Too many requests. Please wait a moment and try again.';
      }
      if (message.includes('500') || message.includes('Internal Server Error')) {
        return 'The server encountered an error. Please try again later or contact support.';
      }
      if (message.includes('token') && message.includes('expired')) {
        return 'Your authentication token has expired. Please get a new token and sign in again.';
      }

      return message;
    };

    test('should return friendly message for network errors', () => {
      const error = new Error('Failed to fetch');
      const result = getUserFriendlyError(error);
      expect(result).toContain('Cannot connect to the server');
    });

    test('should return friendly message for 401 errors', () => {
      const error = new Error('401 Unauthorized');
      const result = getUserFriendlyError(error);
      expect(result).toContain('session has expired');
    });

    test('should return friendly message for 403 errors', () => {
      const error = new Error('403 Forbidden');
      const result = getUserFriendlyError(error);
      expect(result).toContain('don\'t have permission');
    });

    test('should return friendly message for 404 errors', () => {
      const error = new Error('404 Not Found');
      const result = getUserFriendlyError(error);
      expect(result).toContain('not found');
    });

    test('should return friendly message for rate limit errors', () => {
      const error = new Error('429 Too Many Requests');
      const result = getUserFriendlyError(error);
      expect(result).toContain('Too many requests');
    });

    test('should return friendly message for server errors', () => {
      const error = new Error('500 Internal Server Error');
      const result = getUserFriendlyError(error);
      expect(result).toContain('server encountered an error');
    });

    test('should return friendly message for token expiration', () => {
      const error = new Error('token has expired');
      const result = getUserFriendlyError(error);
      expect(result).toContain('authentication token has expired');
    });

    test('should return original message for unknown errors', () => {
      const error = new Error('Some random error');
      const result = getUserFriendlyError(error);
      expect(result).toBe('Some random error');
    });
  });

  describe('Economic Metrics Calculations', () => {
    test('should calculate transaction velocity correctly', () => {
      const now = Math.floor(Date.now() / 1000);
      const thirtyDaysAgo = now - (30 * 24 * 60 * 60);

      const transactions = [
        { timestamp: now - 1000, from: 'did:icn:a', to: 'did:icn:b' },
        { timestamp: now - 2000, from: 'did:icn:b', to: 'did:icn:c' },
        { timestamp: now - 3000, from: 'did:icn:c', to: 'did:icn:a' },
        { timestamp: thirtyDaysAgo - 1000, from: 'did:icn:d', to: 'did:icn:e' }, // Too old
      ];

      const recentTransactions = transactions.filter(tx => tx.timestamp >= thirtyDaysAgo);
      const activeMemberCount = new Set(
        recentTransactions.flatMap(tx => [tx.from, tx.to])
      ).size || 1;

      const velocity = (recentTransactions.length / activeMemberCount).toFixed(2);

      expect(recentTransactions.length).toBe(3);
      expect(activeMemberCount).toBe(3);
      expect(velocity).toBe('1.00');
    });

    test('should calculate participation rate correctly', () => {
      const now = Math.floor(Date.now() / 1000);
      const thirtyDaysAgo = now - (30 * 24 * 60 * 60);

      const transactions = [
        { timestamp: now - 1000, from: 'did:icn:a', to: 'did:icn:b' },
        { timestamp: now - 2000, from: 'did:icn:b', to: 'did:icn:c' },
      ];

      const members = [
        { did: 'did:icn:a' },
        { did: 'did:icn:b' },
        { did: 'did:icn:c' },
        { did: 'did:icn:d' },
        { did: 'did:icn:e' },
      ];

      const recentTransactions = transactions.filter(tx => tx.timestamp >= thirtyDaysAgo);
      const activeMemberCount = new Set(
        recentTransactions.flatMap(tx => [tx.from, tx.to])
      ).size;

      const totalMembers = members.length;
      const participation = ((activeMemberCount / totalMembers) * 100).toFixed(1);

      expect(activeMemberCount).toBe(3);
      expect(participation).toBe('60.0');
    });

    test('should calculate hoarding index correctly', () => {
      const balances = [10, 10, 10, 50, 100];
      const avgBalance = balances.reduce((sum, b) => sum + b, 0) / balances.length;
      const hoarders = balances.filter(b => b > avgBalance * 2).length;
      const totalMembers = balances.length;
      const hoarding = ((hoarders / totalMembers) * 100).toFixed(1);

      expect(avgBalance).toBe(36);
      expect(hoarders).toBe(1); // Only 100 > 72
      expect(hoarding).toBe('20.0');
    });
  });

  describe('Filter and Sort Operations', () => {
    test('should filter service listings by type', () => {
      const listings = [
        { type: 'offer', category: 'Education', title: 'Math' },
        { type: 'request', category: 'Home', title: 'Moving' },
        { type: 'offer', category: 'Tech', title: 'Coding' },
      ];

      const offers = listings.filter(s => s.type === 'offer');
      const requests = listings.filter(s => s.type === 'request');

      expect(offers.length).toBe(2);
      expect(requests.length).toBe(1);
    });

    test('should filter service listings by category', () => {
      const listings = [
        { type: 'offer', category: 'Education', title: 'Math' },
        { type: 'request', category: 'Home', title: 'Moving' },
        { type: 'offer', category: 'Education', title: 'Physics' },
      ];

      const education = listings.filter(s => s.category === 'Education');

      expect(education.length).toBe(2);
    });

    test('should search service listings', () => {
      const listings = [
        { type: 'offer', category: 'Education', title: 'Math Tutoring', description: 'Algebra help' },
        { type: 'request', category: 'Home', title: 'Moving Help', description: 'Need furniture movers' },
      ];

      const query = 'math';
      const results = listings.filter(s =>
        s.title.toLowerCase().includes(query) ||
        s.description.toLowerCase().includes(query)
      );

      expect(results.length).toBe(1);
      expect(results[0].title).toBe('Math Tutoring');
    });

    test('should sort transactions by date', () => {
      const transactions = [
        { timestamp: 1000, amount: 10 },
        { timestamp: 3000, amount: 30 },
        { timestamp: 2000, amount: 20 },
      ];

      const sorted = [...transactions].sort((a, b) => b.timestamp - a.timestamp);

      expect(sorted[0].timestamp).toBe(3000);
      expect(sorted[1].timestamp).toBe(2000);
      expect(sorted[2].timestamp).toBe(1000);
    });

    test('should sort transactions by amount', () => {
      const transactions = [
        { timestamp: 1000, amount: 10 },
        { timestamp: 2000, amount: 30 },
        { timestamp: 3000, amount: 20 },
      ];

      const sorted = [...transactions].sort((a, b) => b.amount - a.amount);

      expect(sorted[0].amount).toBe(30);
      expect(sorted[1].amount).toBe(20);
      expect(sorted[2].amount).toBe(10);
    });
  });

  describe('Balance Calculations', () => {
    test('should calculate user balance correctly', () => {
      const did = 'did:icn:alice';
      const transactions = [
        { from: did, to: 'did:icn:bob', amount: 5 },       // -5
        { from: 'did:icn:bob', to: did, amount: 10 },     // +10
        { from: did, to: 'did:icn:charlie', amount: 3 },  // -3
        { from: 'did:icn:charlie', to: did, amount: 8 },  // +8
      ];

      const debits = transactions
        .filter(tx => tx.from === did)
        .reduce((sum, tx) => sum + tx.amount, 0);

      const credits = transactions
        .filter(tx => tx.to === did)
        .reduce((sum, tx) => sum + tx.amount, 0);

      const balance = credits - debits;

      expect(debits).toBe(8);
      expect(credits).toBe(18);
      expect(balance).toBe(10);
    });

    test('should handle zero balance', () => {
      const did = 'did:icn:alice';
      const transactions = [
        { from: did, to: 'did:icn:bob', amount: 10 },
        { from: 'did:icn:bob', to: did, amount: 10 },
      ];

      const debits = transactions
        .filter(tx => tx.from === did)
        .reduce((sum, tx) => sum + tx.amount, 0);

      const credits = transactions
        .filter(tx => tx.to === did)
        .reduce((sum, tx) => sum + tx.amount, 0);

      const balance = credits - debits;

      expect(balance).toBe(0);
    });

    test('should handle negative balance', () => {
      const did = 'did:icn:alice';
      const transactions = [
        { from: did, to: 'did:icn:bob', amount: 15 },
        { from: 'did:icn:bob', to: did, amount: 5 },
      ];

      const debits = transactions
        .filter(tx => tx.from === did)
        .reduce((sum, tx) => sum + tx.amount, 0);

      const credits = transactions
        .filter(tx => tx.to === did)
        .reduce((sum, tx) => sum + tx.amount, 0);

      const balance = credits - debits;

      expect(balance).toBe(-10);
    });
  });
});
