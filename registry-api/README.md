# StarForge Remote Template Registry API

A centralized remote template registry API that allows global template sharing, versioning, and community contributions. Creates a template marketplace similar to npm or crates.io.

## Features

- ✓ Remote template search with filters (tags, verified, quality score)
- ✓ Template download and installation from remote
- ✓ User authentication with JWT tokens
- ✓ Template publishing with versioning
- ✓ Template rating and review system
- ✓ Web interface for template browsing
- ✓ RESTful API for CLI integration

## Quick Start

```bash
npm install
cp .env.example .env
npm run dev
```

Server runs on `http://localhost:3000`

## Docker

```bash
docker-compose up
```

Starts Registry API + MongoDB

## API Endpoints

The complete OpenAPI 3.0 contract is available in [`openapi.json`](./openapi.json)
and as an asset on each GitHub release at
[releases/latest/download/openapi.json](https://github.com/Nanle-code/StarForge/releases/latest/download/openapi.json).

### Authentication

- `POST /api/auth/signup` - Create account
- `POST /api/auth/login` - Login (returns JWT token)
- `POST /api/auth/verify` - Verify token

### Templates

- `POST /api/templates/search` - Search registry
- `GET /api/templates/:name/:version` - Get template details
- `POST /api/templates/publish` - Publish template (auth required)
- `GET /api/templates/:name/:version/download` - Download template

### Reviews

- `GET /api/reviews/template/:templateId` - Get reviews
- `POST /api/reviews/template/:templateId/reviews` - Post review (auth required)

## Request Examples

### Search Templates

```bash
curl -X POST http://localhost:3000/api/templates/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "counter",
    "tags": ["example"],
    "verified": true,
    "limit": 20,
    "offset": 0
  }'
```

### Signup

```bash
curl -X POST http://localhost:3000/api/auth/signup \
  -H "Content-Type: application/json" \
  -d '{
    "email": "user@example.com",
    "username": "user",
    "password": "password123"
  }'
```

### Publish Template

```bash
curl -X POST http://localhost:3000/api/templates/publish \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <token>" \
  -d '{
    "name": "my-template",
    "version": "1.0.0",
    "description": "My template",
    "author": "Your Name",
    "tags": ["example"],
    "content": "<base64-encoded-zip>"
  }'
```

## CLI Integration

```bash
starforge registry search counter
starforge registry login
starforge registry publish ./my-template
starforge registry install my-template
starforge registry review my-template --rating 5
```

## Production Deployment

```bash
npm run build
NODE_ENV=production npm start
```

With Docker:

```bash
docker build -t starforge-registry:latest .
docker run -d -p 3000:3000 \
  -e NODE_ENV=production \
  -e JWT_SECRET=your-secret \
  -e MONGODB_URI=your-db \
  starforge-registry:latest
```

## Development

```bash
npm run dev      # Development server
npm run lint     # Lint code
npm run build    # Build TypeScript
npm test         # Run tests
```

## Documentation

- [Quick Start Guide](./QUICK_START.md)
- [Implementation Guide](../REMOTE_REGISTRY_IMPLEMENTATION.md)
- [Developer Guide](../DEVELOPER_GUIDE.md)
- [Architecture](../ARCHITECTURE.md)

## Support

- **Issues:** https://github.com/Nanle-code/StarForge/issues
- **Discussions:** https://github.com/Nanle-code/StarForge/discussions

## License

MIT
