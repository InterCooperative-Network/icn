import React, { createContext, useContext, ReactNode } from 'react';

interface ICNContextType {
  client: any | null;
}

const ICNContext = createContext<ICNContextType | undefined>(undefined);

export function ICNProvider({ 
  children, 
  client 
}: { 
  children: ReactNode; 
  client: any | null;
}) {
  return (
    <ICNContext.Provider value={{ client }}>
      {children}
    </ICNContext.Provider>
  );
}

export function useICNContext() {
  const context = useContext(ICNContext);
  if (context === undefined) {
    throw new Error('useICNContext must be used within an ICNProvider');
  }
  return context;
}
