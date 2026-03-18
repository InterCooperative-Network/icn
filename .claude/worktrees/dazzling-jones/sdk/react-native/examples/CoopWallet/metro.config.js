const { getDefaultConfig } = require('expo/metro-config');
const path = require('path');

const projectRoot = __dirname;
const workspaceRoot = path.resolve(projectRoot, '../../../..');

const config = getDefaultConfig(projectRoot);

// Watch the local SDK packages
config.watchFolders = [
  path.resolve(workspaceRoot, 'sdk/react-native'),
  path.resolve(workspaceRoot, 'sdk/typescript'),
];

// Resolve modules from project and workspace
config.resolver.nodeModulesPaths = [
  path.resolve(projectRoot, 'node_modules'),
  path.resolve(workspaceRoot, 'sdk/react-native/node_modules'),
  path.resolve(workspaceRoot, 'sdk/typescript/node_modules'),
];

// Ensure we can resolve the local packages
config.resolver.extraNodeModules = {
  '@icn/react-native': path.resolve(workspaceRoot, 'sdk/react-native'),
  '@icn/client': path.resolve(workspaceRoot, 'sdk/typescript'),
};

module.exports = config;
