const createExpoWebpackConfigAsync = require('@expo/webpack-config');
const path = require('path');

module.exports = async function (env, argv) {
  const config = await createExpoWebpackConfigAsync(env, argv);

  // Resolve symlinks for @icn/react-native and @icn/client
  config.resolve = config.resolve || {};
  config.resolve.symlinks = true;

  // Add aliases for web-incompatible native modules and local packages
  const sdkReactNative = path.resolve(__dirname, '../../');
  const sdkTypescript = path.resolve(__dirname, '../../../../sdk/typescript');

  config.resolve.alias = {
    ...config.resolve.alias,
    '@icn/react-native': path.resolve(sdkReactNative, 'dist'),
    '@icn/client': path.resolve(sdkTypescript, 'dist'),
  };

  // Ensure the SDK directories are included in babel-loader
  config.module.rules.forEach(rule => {
    if (rule.oneOf) {
      rule.oneOf.forEach(oneOfRule => {
        if (oneOfRule.include && Array.isArray(oneOfRule.include)) {
          oneOfRule.include.push(sdkReactNative);
          oneOfRule.include.push(sdkTypescript);
        }
      });
    }
  });

  return config;
};
