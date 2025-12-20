# ICN Gateway API Documentation

Interactive API documentation powered by Swagger UI for the ICN Gateway REST API.

## Quick Start

### Option 1: Python HTTP Server (Simplest)

```bash
cd web/api-docs
python3 -m http.server 3000
```

Then visit: http://localhost:3000

### Option 2: Node.js HTTP Server

```bash
cd web/api-docs
npx http-server -p 3000
```

### Option 3: Docker

```bash
cd web/api-docs
docker build -t icn-api-docs .
docker run -p 3000:80 icn-api-docs
```

## Features

- **Interactive API Explorer**: Try API calls directly from your browser
- **Complete API Reference**: All endpoints, parameters, and responses documented
- **Authentication Flow**: Step-by-step guide for DID-based auth
- **Request/Response Examples**: See example requests and responses for each endpoint
- **Schema Documentation**: Detailed schema definitions for all data types
- **Search & Filter**: Quickly find the endpoint you need
- **Persistent Authorization**: Save your JWT token for testing

## API Overview

The ICN Gateway provides REST and WebSocket APIs for:

### Core Features
- **Authentication**: Challenge-response with DID and Ed25519 signatures
- **Identity**: DID resolution and identity management
- **Ledger**: Mutual credit transactions and balance management
- **Governance**: Proposals, voting, and governance domains
- **Cooperatives**: Lifecycle management for cooperatives
- **Compute**: Distributed CCL contract execution
- **Federation**: Inter-cooperative coordination
- **WebSocket**: Real-time event streaming

### Endpoints Summary

| Category | Endpoints | Description |
|----------|-----------|-------------|
| Health | 4 | Liveness, readiness, and health checks |
| Auth | 2 | Challenge-response authentication |
| Identity | 3 | DID resolution and management |
| Ledger | 5 | Payments, balances, transaction history |
| Governance | 10+ | Domains, proposals, voting |
| Cooperatives | 8+ | CRUD operations for coops |
| Members | 5+ | Member profiles and membership |
| Compute | 6+ | Task submission and monitoring |
| Federation | 4+ | Cross-cooperative operations |
| SDIS | 10+ | Social DID issuance (steward-based) |
| WebSocket | 1 | Real-time event streaming |

**Total**: 60+ documented endpoints

## Authentication

Most endpoints require JWT authentication. To authenticate:

1. **Request Challenge**
   ```bash
   curl -X POST http://localhost:8000/v1/auth/challenge \
     -H "Content-Type: application/json" \
     -d '{"did": "did:icn:your-did-here"}'
   ```

2. **Sign Challenge** (with your private key)
   ```bash
   # Use icnctl or your preferred signing tool
   icnctl auth sign --challenge <challenge>
   ```

3. **Verify and Get Token**
   ```bash
   curl -X POST http://localhost:8000/v1/auth/verify \
     -H "Content-Type: application/json" \
     -d '{
       "did": "did:icn:your-did-here",
       "challenge": "<challenge>",
       "signature": "<signature>"
     }'
   ```

4. **Use Token**
   ```bash
   curl http://localhost:8000/v1/ledger/balance \
     -H "Authorization: Bearer <your-jwt-token>"
   ```

## Try It Out

The Swagger UI includes a "Try it out" feature:

1. Click "Authorize" button (top right)
2. Enter your JWT token
3. Navigate to any endpoint
4. Click "Try it out"
5. Fill in parameters
6. Click "Execute"
7. See the response

## OpenAPI Specification

The complete OpenAPI 3.1 specification is available at:

- **Raw YAML**: [openapi.yaml](./openapi.yaml)
- **Interactive UI**: Visit the index page

You can use this spec to:
- Generate client SDKs in any language
- Import into Postman/Insomnia
- Set up automated testing
- Build custom tooling

## Development

### Updating the API Docs

1. Edit `/docs/api/openapi.yaml` in the main repository
2. Copy to web directory:
   ```bash
   cp docs/api/openapi.yaml web/api-docs/openapi.yaml
   ```
3. Refresh browser to see changes

### Local Testing

```bash
# Start the ICN Gateway
cd icn
cargo run --bin icnd

# In another terminal, start API docs
cd web/api-docs
python3 -m http.server 3000

# Visit http://localhost:3000
```

### Customization

The Swagger UI can be customized by editing `index.html`:
- Change theme colors
- Add/remove features
- Customize layout
- Add custom plugins

## Production Deployment

For production, serve the API documentation alongside your Gateway:

### Nginx Configuration

```nginx
server {
    listen 443 ssl;
    server_name api.example.com;
    
    # API endpoints
    location /v1/ {
        proxy_pass http://localhost:8000;
    }
    
    # API documentation
    location /docs {
        alias /var/www/icn-api-docs;
        index index.html;
    }
}
```

### Docker Compose

```yaml
version: '3'
services:
  icn-gateway:
    image: icn-gateway
    ports:
      - "8000:8000"
  
  api-docs:
    image: nginx:alpine
    volumes:
      - ./web/api-docs:/usr/share/nginx/html
    ports:
      - "3000:80"
```

## Security Notes

**⚠️ Important**: API documentation should be publicly accessible, but:

1. **Production Gateway**: Should use HTTPS/TLS
2. **CORS**: Configure properly for your domain
3. **Rate Limiting**: Already implemented in Gateway
4. **Authentication**: Required for most endpoints

The Swagger UI "Try it out" feature makes real API calls to your configured server.

## Browser Support

- ✅ Chrome 90+
- ✅ Firefox 88+
- ✅ Safari 14+
- ✅ Edge 90+

## Troubleshooting

### CORS Errors

If you see CORS errors when trying API calls:

1. Check Gateway CORS configuration
2. Ensure `Access-Control-Allow-Origin` is set
3. Use `--cors-allow-origin` flag when starting Gateway

### "Failed to load API definition"

1. Ensure `openapi.yaml` is in same directory as `index.html`
2. Check browser console for errors
3. Validate YAML syntax at https://editor.swagger.io

### API Calls Not Working

1. Verify Gateway is running: `curl http://localhost:8000/health`
2. Check server URL in Swagger UI settings
3. Ensure JWT token is valid (not expired)

## Resources

- [OpenAPI Specification](https://swagger.io/specification/)
- [Swagger UI Documentation](https://swagger.io/docs/open-source-tools/swagger-ui/)
- [ICN Gateway Source](../../icn/crates/icn-gateway/)
- [API Implementation](../../icn/crates/icn-gateway/src/api/)

## License

MIT - See LICENSE file

## Support

- Documentation: https://github.com/InterCooperative-Network/icn/tree/main/docs
- Issues: https://github.com/InterCooperative-Network/icn/issues
- Discussions: https://github.com/InterCooperative-Network/icn/discussions
