import { createContext } from 'react';
import { ICNClient } from '../services/ICNClient';

interface AuthContextType {
  isAuthenticated: boolean;
  client: ICNClient | null;
  /**
   * Login with credentials
   * @param token - JWT authentication token
   * @param apiUrl - Gateway API URL
   * @param did - Optional user DID (for display)
   * @param coopId - Optional cooperative ID
   */
  login: (token: string, apiUrl: string, did?: string, coopId?: string) => Promise<void>;
  logout: () => Promise<void>;
}

export const AuthContext = createContext<AuthContextType>({
  isAuthenticated: false,
  client: null,
  login: async () => {},
  logout: async () => {},
});
