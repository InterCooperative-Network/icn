/**
 * Custom Jest resolver to handle @noble package subpath exports
 *
 * @noble packages use ESM with subpath exports (e.g., @noble/hashes/sha2)
 * which Jest's default resolver doesn't handle well.
 */

const path = require('path');
const fs = require('fs');

module.exports = (request, options) => {
  // Handle @noble subpath exports
  if (request.startsWith('@noble/')) {
    const parts = request.split('/');
    if (parts.length >= 3) {
      // e.g., @noble/hashes/sha2 -> @noble/hashes/sha2.js
      const packageName = parts.slice(0, 2).join('/');
      const subpath = parts.slice(2).join('/');

      // Try resolving with .js extension
      const withJs = `${packageName}/${subpath}.js`;
      const packageDir = path.join(options.basedir, 'node_modules', packageName);

      if (fs.existsSync(path.join(packageDir, `${subpath}.js`))) {
        return options.defaultResolver(withJs, options);
      }

      // Try resolving the original path with the package's exports
      try {
        return options.defaultResolver(request, {
          ...options,
          packageFilter: (pkg) => {
            // If the package has exports, use them
            if (pkg.exports) {
              const exportKey = `./${subpath}`;
              const exportKeyJs = `./${subpath}.js`;
              if (pkg.exports[exportKey]) {
                pkg.main = typeof pkg.exports[exportKey] === 'string'
                  ? pkg.exports[exportKey]
                  : pkg.exports[exportKey].default || pkg.exports[exportKey].require || pkg.exports[exportKey].import;
              } else if (pkg.exports[exportKeyJs]) {
                pkg.main = typeof pkg.exports[exportKeyJs] === 'string'
                  ? pkg.exports[exportKeyJs]
                  : pkg.exports[exportKeyJs].default || pkg.exports[exportKeyJs].require || pkg.exports[exportKeyJs].import;
              }
            }
            return pkg;
          },
        });
      } catch (e) {
        // Fall through to default resolver
      }
    }
  }

  // Default resolver for everything else
  return options.defaultResolver(request, options);
};
