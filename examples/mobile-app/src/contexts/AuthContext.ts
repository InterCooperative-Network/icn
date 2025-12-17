import { createContext } from 'react';
import { ICNClient } from '../services/ICNClient';

interface AuthContextType {
  isAuthenticated: boolean;
  client: ICNClient | null;
  login: (token: string, apiUrl: string) => Promise<void>;
  logout: () => Promise<void>;
}

export const AuthContext = createContext<AuthContextType>({
  isAuthenticated: false,
  client: null,
  login: async () => {},
  logout: async () => {},
});
