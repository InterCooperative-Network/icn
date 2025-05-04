import React, { useState } from 'react';
import { Link, useLocation } from 'react-router-dom';
import { useCredentials } from '../contexts/CredentialContext';
import {
  Squares2X2Icon,
  DocumentTextIcon,
  UserGroupIcon,
  IdentificationIcon,
  ArrowLeftOnRectangleIcon,
  Bars3Icon,
  XMarkIcon,
} from '@heroicons/react/24/outline';

export default function Layout({ children }) {
  const location = useLocation();
  const { isAuthenticated, userDid, federationId, clearCredentials } = useCredentials();
  const [sidebarOpen, setSidebarOpen] = useState(false);
  
  const navigation = [
    { name: 'Dashboard', href: '/', icon: Squares2X2Icon, current: location.pathname === '/' },
    { name: 'Proposals', href: '/proposals', icon: DocumentTextIcon, current: location.pathname.startsWith('/proposals') },
    { name: 'Federations', href: '/federations', icon: UserGroupIcon, current: location.pathname.startsWith('/federations') },
  ];

  return (
    <div className="min-h-screen bg-gray-100">
      {/* Mobile sidebar */}
      <div className={`fixed inset-0 z-40 ${sidebarOpen ? 'block' : 'hidden'} lg:hidden`}>
        <div className="fixed inset-0 bg-gray-600 bg-opacity-75" onClick={() => setSidebarOpen(false)} />
        <div className="fixed inset-y-0 left-0 flex max-w-xs w-full bg-white">
          <div className="flex-1 flex flex-col pt-5 pb-4 overflow-y-auto">
            <div className="flex items-center flex-shrink-0 px-4">
              <h1 className="text-2xl font-bold text-agora-blue">AgoraNet</h1>
              <button onClick={() => setSidebarOpen(false)} className="ml-auto">
                <XMarkIcon className="h-6 w-6 text-gray-500" />
              </button>
            </div>
            <nav className="mt-5 px-2 space-y-1">
              {navigation.map((item) => (
                <Link
                  key={item.name}
                  to={item.href}
                  className={`group flex items-center px-2 py-2 text-base font-medium rounded-md ${
                    item.current
                      ? 'bg-gray-100 text-agora-blue'
                      : 'text-gray-600 hover:bg-gray-50 hover:text-gray-900'
                  }`}
                >
                  <item.icon
                    className={`mr-4 h-6 w-6 ${
                      item.current ? 'text-agora-blue' : 'text-gray-400 group-hover:text-gray-500'
                    }`}
                  />
                  {item.name}
                </Link>
              ))}
            </nav>
          </div>
        </div>
      </div>

      {/* Static sidebar for desktop */}
      <div className="hidden lg:flex lg:w-64 lg:flex-col lg:fixed lg:inset-y-0">
        <div className="flex flex-col flex-grow border-r border-gray-200 pt-5 bg-white overflow-y-auto">
          <div className="flex items-center flex-shrink-0 px-4">
            <h1 className="text-2xl font-bold text-agora-blue">AgoraNet</h1>
          </div>
          <div className="mt-5 flex-grow flex flex-col">
            <nav className="flex-1 px-2 pb-4 space-y-1">
              {navigation.map((item) => (
                <Link
                  key={item.name}
                  to={item.href}
                  className={`group flex items-center px-2 py-2 text-sm font-medium rounded-md ${
                    item.current
                      ? 'bg-gray-100 text-agora-blue'
                      : 'text-gray-600 hover:bg-gray-50 hover:text-gray-900'
                  }`}
                >
                  <item.icon
                    className={`mr-3 h-6 w-6 ${
                      item.current ? 'text-agora-blue' : 'text-gray-400 group-hover:text-gray-500'
                    }`}
                  />
                  {item.name}
                </Link>
              ))}
            </nav>
          </div>
          {isAuthenticated && (
            <div className="flex-shrink-0 flex border-t border-gray-200 p-4">
              <div className="flex-shrink-0 w-full group block">
                <div className="flex items-center">
                  <div className="ml-1">
                    <p className="text-sm font-medium text-gray-700">{userDid ? userDid.substring(0, 16) + '...' : 'Anonymous'}</p>
                    <p className="text-xs font-medium text-gray-500">{federationId ? `Federation: ${federationId.substring(0, 8)}...` : 'No Federation'}</p>
                  </div>
                  <button
                    onClick={clearCredentials}
                    className="ml-auto p-1 rounded-full text-gray-400 hover:text-gray-500"
                  >
                    <ArrowLeftOnRectangleIcon className="h-6 w-6" />
                  </button>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Main content */}
      <div className="lg:pl-64 flex flex-col">
        <div className="sticky top-0 z-10 flex-shrink-0 flex h-16 bg-white shadow">
          <button
            type="button"
            className="px-4 border-r border-gray-200 text-gray-500 lg:hidden"
            onClick={() => setSidebarOpen(true)}
          >
            <Bars3Icon className="h-6 w-6" />
          </button>
          <div className="flex-1 px-4 flex justify-between">
            <div className="flex-1 flex items-center">
              <h1 className="text-2xl font-semibold text-gray-900">
                {navigation.find(item => item.current)?.name || 'AgoraNet Dashboard'}
              </h1>
            </div>
            <div className="flex items-center">
              {!isAuthenticated && (
                <Link
                  to="/login"
                  className="ml-6 inline-flex items-center px-4 py-2 border border-transparent text-sm font-medium rounded-md shadow-sm text-white bg-agora-blue hover:bg-blue-700"
                >
                  <IdentificationIcon className="-ml-1 mr-2 h-5 w-5" />
                  Connect Wallet
                </Link>
              )}
            </div>
          </div>
        </div>
        <main className="flex-1">
          <div className="py-6 px-4 sm:px-6 lg:px-8">{children}</div>
        </main>
      </div>
    </div>
  );
} 