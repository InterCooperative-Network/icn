/**
 * ICN Timebank - Service Worker
 * Provides offline support, caching, and background sync
 */

const CACHE_VERSION = 'icn-timebank-v1.0';
const STATIC_CACHE = `${CACHE_VERSION}-static`;
const DYNAMIC_CACHE = `${CACHE_VERSION}-dynamic`;
const API_CACHE = `${CACHE_VERSION}-api`;

// Files to cache immediately on install
const STATIC_ASSETS = [
    '/',
    '/index.html',
    '/style.css',
    '/app.js',
    '/manifest.json',
    '/offline.html' // Fallback page for offline
];

// Max cache sizes
const MAX_DYNAMIC_CACHE_SIZE = 50;
const MAX_API_CACHE_SIZE = 30;

// ============================================================================
// Install Event - Cache static assets
// ============================================================================
self.addEventListener('install', (event) => {
    console.log('[Service Worker] Installing...');

    event.waitUntil(
        caches.open(STATIC_CACHE)
            .then((cache) => {
                console.log('[Service Worker] Caching static assets');
                return cache.addAll(STATIC_ASSETS);
            })
            .then(() => self.skipWaiting()) // Activate immediately
            .catch((err) => {
                console.error('[Service Worker] Failed to cache static assets:', err);
            })
    );
});

// ============================================================================
// Activate Event - Clean up old caches
// ============================================================================
self.addEventListener('activate', (event) => {
    console.log('[Service Worker] Activating...');

    event.waitUntil(
        caches.keys()
            .then((cacheNames) => {
                return Promise.all(
                    cacheNames
                        .filter((name) => name.startsWith('icn-timebank-') && !name.includes(CACHE_VERSION))
                        .map((name) => {
                            console.log('[Service Worker] Deleting old cache:', name);
                            return caches.delete(name);
                        })
                );
            })
            .then(() => self.clients.claim()) // Take control of all pages
    );
});

// ============================================================================
// Fetch Event - Serve from cache, fallback to network
// ============================================================================
self.addEventListener('fetch', (event) => {
    const { request } = event;
    const url = new URL(request.url);

    // Skip non-GET requests
    if (request.method !== 'GET') {
        return;
    }

    // API requests - Network first, then cache
    if (url.pathname.startsWith('/v1/')) {
        event.respondWith(networkFirstStrategy(request, API_CACHE));
        return;
    }

    // Static assets - Cache first, then network
    if (STATIC_ASSETS.includes(url.pathname) || url.pathname.match(/\.(js|css|png|jpg|jpeg|svg|woff2?|ttf)$/)) {
        event.respondWith(cacheFirstStrategy(request, STATIC_CACHE));
        return;
    }

    // HTML pages - Network first, then cache
    if (request.headers.get('accept').includes('text/html')) {
        event.respondWith(networkFirstStrategy(request, DYNAMIC_CACHE));
        return;
    }

    // Everything else - Try cache, then network
    event.respondWith(
        caches.match(request)
            .then((response) => response || fetch(request))
            .catch(() => caches.match('/offline.html'))
    );
});

// ============================================================================
// Caching Strategies
// ============================================================================

// Cache First: Check cache first, fallback to network
async function cacheFirstStrategy(request, cacheName) {
    try {
        const cachedResponse = await caches.match(request);

        if (cachedResponse) {
            console.log('[Service Worker] Serving from cache:', request.url);
            return cachedResponse;
        }

        // Not in cache, fetch from network
        const networkResponse = await fetch(request);

        // Cache the response for future use
        if (networkResponse.ok) {
            const cache = await caches.open(cacheName);
            cache.put(request, networkResponse.clone());
        }

        return networkResponse;
    } catch (error) {
        console.error('[Service Worker] Cache first strategy failed:', error);
        return caches.match('/offline.html');
    }
}

// Network First: Try network first, fallback to cache
async function networkFirstStrategy(request, cacheName) {
    try {
        // Try network first
        const networkResponse = await fetch(request);

        if (networkResponse.ok) {
            // Cache the fresh response
            const cache = await caches.open(cacheName);
            cache.put(request, networkResponse.clone());

            // Limit cache size
            limitCacheSize(cacheName, cacheName === API_CACHE ? MAX_API_CACHE_SIZE : MAX_DYNAMIC_CACHE_SIZE);

            return networkResponse;
        }

        throw new Error('Network response was not ok');
    } catch (error) {
        console.log('[Service Worker] Network failed, serving from cache:', request.url);

        // Fallback to cache
        const cachedResponse = await caches.match(request);

        if (cachedResponse) {
            return cachedResponse;
        }

        // No cache, return offline page
        if (request.headers.get('accept').includes('text/html')) {
            return caches.match('/offline.html');
        }

        // For API requests, return error response
        if (request.url.includes('/v1/')) {
            return new Response(
                JSON.stringify({ error: 'offline', message: 'You are offline. Please check your connection.' }),
                {
                    status: 503,
                    headers: { 'Content-Type': 'application/json' }
                }
            );
        }

        throw error;
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

// Limit cache size by removing oldest entries
async function limitCacheSize(cacheName, maxSize) {
    const cache = await caches.open(cacheName);
    const keys = await cache.keys();

    if (keys.length > maxSize) {
        // Delete the oldest entry
        await cache.delete(keys[0]);

        // Recursively limit size
        limitCacheSize(cacheName, maxSize);
    }
}

// ============================================================================
// Background Sync (for offline transactions)
// ============================================================================
self.addEventListener('sync', (event) => {
    console.log('[Service Worker] Background sync:', event.tag);

    if (event.tag === 'sync-transactions') {
        event.waitUntil(syncPendingTransactions());
    }
});

async function syncPendingTransactions() {
    // Get pending transactions from IndexedDB (if implemented)
    // This would sync transactions that were created while offline

    console.log('[Service Worker] Syncing pending transactions...');

    // TODO: Implement IndexedDB storage for pending transactions
    // TODO: Send pending transactions to server
    // TODO: Clear pending transactions after successful sync
}

// ============================================================================
// Push Notifications (for future use)
// ============================================================================
self.addEventListener('push', (event) => {
    console.log('[Service Worker] Push notification received');

    const data = event.data ? event.data.json() : {};
    const title = data.title || 'ICN Timebank';
    const options = {
        body: data.body || 'You have a new notification',
        icon: '/icons/icon-192x192.png',
        badge: '/icons/badge-72x72.png',
        data: data.url || '/',
        tag: data.tag || 'default',
        requireInteraction: data.requireInteraction || false
    };

    event.waitUntil(
        self.registration.showNotification(title, options)
    );
});

// Handle notification clicks
self.addEventListener('notificationclick', (event) => {
    console.log('[Service Worker] Notification clicked');

    event.notification.close();

    const urlToOpen = event.notification.data || '/';

    event.waitUntil(
        clients.matchAll({ type: 'window', includeUncontrolled: true })
            .then((windowClients) => {
                // Check if there's already a window open
                for (const client of windowClients) {
                    if (client.url === urlToOpen && 'focus' in client) {
                        return client.focus();
                    }
                }

                // No window open, open a new one
                if (clients.openWindow) {
                    return clients.openWindow(urlToOpen);
                }
            })
    );
});

// ============================================================================
// Message Handling (communicate with main thread)
// ============================================================================
self.addEventListener('message', (event) => {
    console.log('[Service Worker] Message received:', event.data);

    if (event.data.action === 'skipWaiting') {
        self.skipWaiting();
    }

    if (event.data.action === 'clearCache') {
        event.waitUntil(
            caches.keys().then((cacheNames) => {
                return Promise.all(
                    cacheNames.map((name) => caches.delete(name))
                );
            })
        );
    }
});
