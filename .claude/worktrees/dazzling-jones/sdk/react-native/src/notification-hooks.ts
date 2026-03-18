/**
 * Notification Hooks for ICN React Native SDK
 *
 * React hooks for managing in-app notifications.
 */

import { useState, useEffect, useCallback, useRef } from 'react';
import { ICNMobileClient } from './client';

// ============================================================================
// Types
// ============================================================================

/**
 * In-app notification
 */
export interface InAppNotification {
  /** Notification ID */
  id: string;
  /** Recipient DID */
  recipient: string;
  /** Cooperative ID */
  coop_id: string;
  /** Title */
  title: string;
  /** Body */
  body: string;
  /** Custom data */
  data?: Record<string, unknown>;
  /** Notification type */
  notification_type: string;
  /** Created timestamp (Unix seconds) */
  created_at: number;
  /** Whether the notification has been read */
  read: boolean;
  /** Read timestamp (Unix seconds) */
  read_at?: number;
}

/**
 * Notification list response
 */
export interface NotificationListResponse {
  notifications: InAppNotification[];
  total: number;
  unread_count: number;
}

/**
 * Notification count response
 */
export interface NotificationCountResponse {
  total: number;
  unread: number;
}

/**
 * Options for listing notifications
 */
export interface ListNotificationsOptions {
  /** Only return unread notifications */
  unreadOnly?: boolean;
  /** Maximum number to return */
  limit?: number;
  /** Offset for pagination */
  offset?: number;
  /** Filter by notification type */
  notificationType?: string;
}

// ============================================================================
// API Functions
// ============================================================================

/**
 * Create notification API methods
 */
export function createNotificationAPI(client: ICNMobileClient) {
  const baseUrl = (client as unknown as { baseUrl: string }).baseUrl;

  const getAuthHeaders = () => {
    const token = (client as unknown as { authToken?: string }).authToken;
    if (!token) {
      throw new Error('Not authenticated');
    }
    return {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${token}`,
    };
  };

  return {
    /**
     * List notifications
     */
    async list(options: ListNotificationsOptions = {}): Promise<NotificationListResponse> {
      const params = new URLSearchParams();
      if (options.unreadOnly) params.set('unread_only', 'true');
      if (options.limit) params.set('limit', options.limit.toString());
      if (options.offset) params.set('offset', options.offset.toString());
      if (options.notificationType) params.set('notification_type', options.notificationType);

      const queryString = params.toString();
      const url = `${baseUrl}/v1/notifications${queryString ? `?${queryString}` : ''}`;

      const response = await fetch(url, {
        method: 'GET',
        headers: getAuthHeaders(),
      });

      if (!response.ok) {
        throw new Error(`Failed to list notifications: ${await response.text()}`);
      }

      return response.json();
    },

    /**
     * Get notification count
     */
    async getCount(): Promise<NotificationCountResponse> {
      const response = await fetch(`${baseUrl}/v1/notifications/count`, {
        method: 'GET',
        headers: getAuthHeaders(),
      });

      if (!response.ok) {
        throw new Error(`Failed to get notification count: ${await response.text()}`);
      }

      return response.json();
    },

    /**
     * Mark a notification as read
     */
    async markRead(notificationId: string): Promise<void> {
      const response = await fetch(
        `${baseUrl}/v1/notifications/${encodeURIComponent(notificationId)}/read`,
        {
          method: 'POST',
          headers: getAuthHeaders(),
        }
      );

      if (!response.ok) {
        throw new Error(`Failed to mark notification as read: ${await response.text()}`);
      }
    },

    /**
     * Mark all notifications as read
     */
    async markAllRead(): Promise<number> {
      const response = await fetch(`${baseUrl}/v1/notifications/read-all`, {
        method: 'POST',
        headers: getAuthHeaders(),
      });

      if (!response.ok) {
        throw new Error(`Failed to mark all notifications as read: ${await response.text()}`);
      }

      const result = await response.json();
      return result.count || 0;
    },

    /**
     * Delete a notification
     */
    async delete(notificationId: string): Promise<void> {
      const response = await fetch(
        `${baseUrl}/v1/notifications/${encodeURIComponent(notificationId)}`,
        {
          method: 'DELETE',
          headers: getAuthHeaders(),
        }
      );

      if (!response.ok) {
        throw new Error(`Failed to delete notification: ${await response.text()}`);
      }
    },
  };
}

// ============================================================================
// Hooks
// ============================================================================

/**
 * Hook for managing notifications
 *
 * @example
 * ```tsx
 * function NotificationCenter() {
 *   const {
 *     notifications,
 *     unreadCount,
 *     isLoading,
 *     markRead,
 *     markAllRead,
 *     deleteNotification,
 *     refresh,
 *   } = useNotifications(client);
 *
 *   return (
 *     <View>
 *       <Text>Unread: {unreadCount}</Text>
 *       <FlatList
 *         data={notifications}
 *         renderItem={({ item }) => (
 *           <TouchableOpacity onPress={() => markRead(item.id)}>
 *             <Text style={{ fontWeight: item.read ? 'normal' : 'bold' }}>
 *               {item.title}
 *             </Text>
 *             <Text>{item.body}</Text>
 *           </TouchableOpacity>
 *         )}
 *       />
 *       <Button title="Mark All Read" onPress={markAllRead} />
 *     </View>
 *   );
 * }
 * ```
 */
export function useNotifications(
  client: ICNMobileClient,
  options: ListNotificationsOptions = {}
) {
  const [notifications, setNotifications] = useState<InAppNotification[]>([]);
  const [total, setTotal] = useState(0);
  const [unreadCount, setUnreadCount] = useState(0);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const api = createNotificationAPI(client);
  const optionsRef = useRef(options);
  optionsRef.current = options;

  const refresh = useCallback(async () => {
    if (!client.authState.isAuthenticated) {
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const result = await api.list(optionsRef.current);
      setNotifications(result.notifications);
      setTotal(result.total);
      setUnreadCount(result.unread_count);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [client.authState.isAuthenticated, api]);

  // Initial load
  useEffect(() => {
    if (client.authState.isAuthenticated) {
      refresh();
    }
  }, [client.authState.isAuthenticated, refresh]);

  const markRead = useCallback(
    async (notificationId: string) => {
      try {
        await api.markRead(notificationId);
        // Optimistic update
        setNotifications((prev) =>
          prev.map((n) =>
            n.id === notificationId ? { ...n, read: true, read_at: Date.now() / 1000 } : n
          )
        );
        setUnreadCount((prev) => Math.max(0, prev - 1));
      } catch (err) {
        setError((err as Error).message);
        throw err;
      }
    },
    [api]
  );

  const markAllRead = useCallback(async () => {
    try {
      await api.markAllRead();
      // Optimistic update
      setNotifications((prev) =>
        prev.map((n) => ({ ...n, read: true, read_at: Date.now() / 1000 }))
      );
      setUnreadCount(0);
    } catch (err) {
      setError((err as Error).message);
      throw err;
    }
  }, [api]);

  const deleteNotification = useCallback(
    async (notificationId: string) => {
      try {
        await api.delete(notificationId);
        // Optimistic update
        setNotifications((prev) => {
          const notif = prev.find((n) => n.id === notificationId);
          if (notif && !notif.read) {
            setUnreadCount((c) => Math.max(0, c - 1));
          }
          return prev.filter((n) => n.id !== notificationId);
        });
        setTotal((prev) => Math.max(0, prev - 1));
      } catch (err) {
        setError((err as Error).message);
        throw err;
      }
    },
    [api]
  );

  return {
    /** List of notifications */
    notifications,
    /** Total notification count */
    total,
    /** Unread notification count */
    unreadCount,
    /** Loading state */
    isLoading,
    /** Error message */
    error,
    /** Mark a notification as read */
    markRead,
    /** Mark all notifications as read */
    markAllRead,
    /** Delete a notification */
    deleteNotification,
    /** Refresh notifications */
    refresh,
  };
}

/**
 * Hook for just the unread count (lightweight)
 *
 * @example
 * ```tsx
 * function NotificationBadge() {
 *   const { unreadCount, refresh } = useUnreadCount(client);
 *
 *   return unreadCount > 0 ? (
 *     <Badge count={unreadCount} />
 *   ) : null;
 * }
 * ```
 */
export function useUnreadCount(client: ICNMobileClient, pollIntervalMs?: number) {
  const [unreadCount, setUnreadCount] = useState(0);
  const [isLoading, setIsLoading] = useState(false);

  const api = createNotificationAPI(client);

  const refresh = useCallback(async () => {
    if (!client.authState.isAuthenticated) {
      return;
    }

    setIsLoading(true);
    try {
      const result = await api.getCount();
      setUnreadCount(result.unread);
    } catch {
      // Silently fail for badge updates
    } finally {
      setIsLoading(false);
    }
  }, [client.authState.isAuthenticated, api]);

  // Initial load
  useEffect(() => {
    if (client.authState.isAuthenticated) {
      refresh();
    }
  }, [client.authState.isAuthenticated, refresh]);

  // Optional polling
  useEffect(() => {
    if (!pollIntervalMs || !client.authState.isAuthenticated) {
      return;
    }

    const interval = setInterval(refresh, pollIntervalMs);
    return () => clearInterval(interval);
  }, [pollIntervalMs, client.authState.isAuthenticated, refresh]);

  return {
    unreadCount,
    isLoading,
    refresh,
  };
}
