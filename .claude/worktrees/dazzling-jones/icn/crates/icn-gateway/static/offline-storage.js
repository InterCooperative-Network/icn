/**
 * ICN Pilot UI - Offline Storage using IndexedDB
 * 
 * Provides persistent storage for:
 * - Pending transactions (to sync when online)
 * - Cached data (members, transactions, proposals)
 * - User preferences
 */

const DB_NAME = 'icn-pilot-db';
const DB_VERSION = 1;

// Store names
const STORES = {
    PENDING_TRANSACTIONS: 'pending_transactions',
    CACHED_MEMBERS: 'cached_members',
    CACHED_TRANSACTIONS: 'cached_transactions',
    CACHED_PROPOSALS: 'cached_proposals',
    USER_PREFERENCES: 'user_preferences',
};

/**
 * Initialize IndexedDB database
 */
function initDB() {
    return new Promise((resolve, reject) => {
        const request = indexedDB.open(DB_NAME, DB_VERSION);

        request.onerror = () => reject(request.error);
        request.onsuccess = () => resolve(request.result);

        request.onupgradeneeded = (event) => {
            const db = event.target.result;

            // Create pending_transactions store
            if (!db.objectStoreNames.contains(STORES.PENDING_TRANSACTIONS)) {
                const store = db.createObjectStore(STORES.PENDING_TRANSACTIONS, {
                    keyPath: 'id',
                    autoIncrement: true,
                });
                store.createIndex('timestamp', 'timestamp', { unique: false });
                store.createIndex('status', 'status', { unique: false });
            }

            // Create cached_members store
            if (!db.objectStoreNames.contains(STORES.CACHED_MEMBERS)) {
                const store = db.createObjectStore(STORES.CACHED_MEMBERS, {
                    keyPath: 'did',
                });
                store.createIndex('updated_at', 'updated_at', { unique: false });
            }

            // Create cached_transactions store
            if (!db.objectStoreNames.contains(STORES.CACHED_TRANSACTIONS)) {
                const store = db.createObjectStore(STORES.CACHED_TRANSACTIONS, {
                    keyPath: 'hash',
                });
                store.createIndex('timestamp', 'timestamp', { unique: false });
                store.createIndex('from_did', 'from_did', { unique: false });
                store.createIndex('to_did', 'to_did', { unique: false });
            }

            // Create cached_proposals store
            if (!db.objectStoreNames.contains(STORES.CACHED_PROPOSALS)) {
                const store = db.createObjectStore(STORES.CACHED_PROPOSALS, {
                    keyPath: 'id',
                });
                store.createIndex('state', 'state', { unique: false });
            }

            // Create user_preferences store
            if (!db.objectStoreNames.contains(STORES.USER_PREFERENCES)) {
                db.createObjectStore(STORES.USER_PREFERENCES, {
                    keyPath: 'key',
                });
            }
        };
    });
}

/**
 * Generic DB operation helper
 */
async function dbOperation(storeName, operation, mode = 'readonly') {
    const db = await initDB();
    return new Promise((resolve, reject) => {
        const transaction = db.transaction(storeName, mode);
        const store = transaction.objectStore(storeName);
        const request = operation(store);

        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
    });
}

// ============================================================================
// Pending Transactions (for offline support)
// ============================================================================

const OfflineStorage = {
    /**
     * Add a pending transaction to sync later
     */
    async addPendingTransaction(transaction) {
        return dbOperation(STORES.PENDING_TRANSACTIONS, (store) => {
            return store.add({
                ...transaction,
                timestamp: Date.now(),
                status: 'pending',
            });
        }, 'readwrite');
    },

    /**
     * Get all pending transactions
     */
    async getPendingTransactions() {
        return dbOperation(STORES.PENDING_TRANSACTIONS, (store) => {
            return store.getAll();
        });
    },

    /**
     * Mark transaction as synced
     */
    async markTransactionSynced(id) {
        return dbOperation(STORES.PENDING_TRANSACTIONS, (store) => {
            const request = store.get(id);
            request.onsuccess = () => {
                const transaction = request.result;
                transaction.status = 'synced';
                transaction.synced_at = Date.now();
                store.put(transaction);
            };
            return request;
        }, 'readwrite');
    },

    /**
     * Mark transaction as failed
     */
    async markTransactionFailed(id, error) {
        return dbOperation(STORES.PENDING_TRANSACTIONS, (store) => {
            const request = store.get(id);
            request.onsuccess = () => {
                const transaction = request.result;
                transaction.status = 'failed';
                transaction.error = error;
                transaction.failed_at = Date.now();
                store.put(transaction);
            };
            return request;
        }, 'readwrite');
    },

    /**
     * Delete a transaction
     */
    async deletePendingTransaction(id) {
        return dbOperation(STORES.PENDING_TRANSACTIONS, (store) => {
            return store.delete(id);
        }, 'readwrite');
    },

    /**
     * Clear all synced transactions
     */
    async clearSyncedTransactions() {
        const transactions = await this.getPendingTransactions();
        const syncedIds = transactions
            .filter(tx => tx.status === 'synced')
            .map(tx => tx.id);

        for (const id of syncedIds) {
            await this.deletePendingTransaction(id);
        }
    },

    // ========================================================================
    // Cached Members
    // ========================================================================

    /**
     * Cache members list
     */
    async cacheMembers(members) {
        return dbOperation(STORES.CACHED_MEMBERS, (store) => {
            members.forEach(member => {
                store.put({
                    ...member,
                    updated_at: Date.now(),
                });
            });
        }, 'readwrite');
    },

    /**
     * Get cached members
     */
    async getCachedMembers() {
        return dbOperation(STORES.CACHED_MEMBERS, (store) => {
            return store.getAll();
        });
    },

    /**
     * Clear cached members
     */
    async clearCachedMembers() {
        return dbOperation(STORES.CACHED_MEMBERS, (store) => {
            return store.clear();
        }, 'readwrite');
    },

    // ========================================================================
    // Cached Transactions
    // ========================================================================

    /**
     * Cache transactions
     */
    async cacheTransactions(transactions) {
        return dbOperation(STORES.CACHED_TRANSACTIONS, (store) => {
            transactions.forEach(tx => {
                store.put(tx);
            });
        }, 'readwrite');
    },

    /**
     * Get cached transactions
     */
    async getCachedTransactions(filters = {}) {
        const allTransactions = await dbOperation(STORES.CACHED_TRANSACTIONS, (store) => {
            return store.getAll();
        });

        let filtered = allTransactions;

        if (filters.from_did) {
            filtered = filtered.filter(tx => tx.from_did === filters.from_did);
        }

        if (filters.to_did) {
            filtered = filtered.filter(tx => tx.to_did === filters.to_did);
        }

        if (filters.limit) {
            filtered = filtered.slice(0, filters.limit);
        }

        return filtered;
    },

    /**
     * Clear cached transactions
     */
    async clearCachedTransactions() {
        return dbOperation(STORES.CACHED_TRANSACTIONS, (store) => {
            return store.clear();
        }, 'readwrite');
    },

    // ========================================================================
    // Cached Proposals
    // ========================================================================

    /**
     * Cache proposals
     */
    async cacheProposals(proposals) {
        return dbOperation(STORES.CACHED_PROPOSALS, (store) => {
            proposals.forEach(proposal => {
                store.put(proposal);
            });
        }, 'readwrite');
    },

    /**
     * Get cached proposals
     */
    async getCachedProposals(state = null) {
        const allProposals = await dbOperation(STORES.CACHED_PROPOSALS, (store) => {
            return store.getAll();
        });

        if (state) {
            return allProposals.filter(p => p.state === state);
        }

        return allProposals;
    },

    /**
     * Clear cached proposals
     */
    async clearCachedProposals() {
        return dbOperation(STORES.CACHED_PROPOSALS, (store) => {
            return store.clear();
        }, 'readwrite');
    },

    // ========================================================================
    // User Preferences
    // ========================================================================

    /**
     * Set a preference
     */
    async setPreference(key, value) {
        return dbOperation(STORES.USER_PREFERENCES, (store) => {
            return store.put({ key, value });
        }, 'readwrite');
    },

    /**
     * Get a preference
     */
    async getPreference(key, defaultValue = null) {
        try {
            const result = await dbOperation(STORES.USER_PREFERENCES, (store) => {
                return store.get(key);
            });
            return result ? result.value : defaultValue;
        } catch (error) {
            return defaultValue;
        }
    },

    /**
     * Clear all preferences
     */
    async clearPreferences() {
        return dbOperation(STORES.USER_PREFERENCES, (store) => {
            return store.clear();
        }, 'readwrite');
    },

    // ========================================================================
    // Maintenance
    // ========================================================================

    /**
     * Clear all data (for logout)
     */
    async clearAll() {
        await this.clearCachedMembers();
        await this.clearCachedTransactions();
        await this.clearCachedProposals();
        await this.clearPreferences();
        // Note: Don't clear pending transactions - they should sync first
    },

    /**
     * Get database statistics
     */
    async getStats() {
        const [
            pendingTransactions,
            cachedMembers,
            cachedTransactions,
            cachedProposals,
        ] = await Promise.all([
            this.getPendingTransactions(),
            this.getCachedMembers(),
            this.getCachedTransactions(),
            this.getCachedProposals(),
        ]);

        return {
            pending_transactions: pendingTransactions.length,
            pending_status: {
                pending: pendingTransactions.filter(tx => tx.status === 'pending').length,
                synced: pendingTransactions.filter(tx => tx.status === 'synced').length,
                failed: pendingTransactions.filter(tx => tx.status === 'failed').length,
            },
            cached_members: cachedMembers.length,
            cached_transactions: cachedTransactions.length,
            cached_proposals: cachedProposals.length,
        };
    },
};

// Make it available globally
if (typeof window !== 'undefined') {
    window.OfflineStorage = OfflineStorage;
}

// Also export for service worker
if (typeof self !== 'undefined' && self instanceof ServiceWorkerGlobalScope) {
    self.OfflineStorage = OfflineStorage;
}
