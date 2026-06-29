# CRAFT Cloud Harness Registry

Private harness registries for teams with Git-backed package management.

## Overview

This crate provides a complete cloud-based registry solution for the CRAFT project:

- **Server (Axum)**: RESTful API for organization, team, and harness management
- **Database (PostgreSQL)**: Full schema with audit logging and rate limiting
- **Storage**: Content-addressed package storage with local filesystem or S3 backend
- **Authentication**: JWT with RS256 signing and access tokens for CI/CD
- **CLI**: Client commands for login, publish, install, and team management

## Features

### Multi-Tenancy
- Organizations with visibility levels (public, internal, private)
- Teams within organizations
- Role-based access control (member, maintainer, admin)

### Package Management
- Harness publishing with Git refs
- Semantic version support with semver matching
- Content-addressed storage (SHA-256)
- Version yanking and unyanking
- Download tracking

### Security
- JWT authentication with RS256
- API tokens for CI/CD (with scoping support)
- Audit logging for all actions
- Rate limiting hooks
- Password hashing with Argon2

### Developer Experience
- CLI with intuitive commands
- Search functionality
- Webhook support for CI/CD integration

## Quick Start

### Using Docker Compose

```bash
# Start dependencies
docker-compose up -d

# The server will be available at http://localhost:8080
```

### Running Tests

```bash
# Run all tests
cargo test -p craft-registry

# Run specific test
cargo test -p craft-registry version_tests

# Run integration tests (requires PostgreSQL)
docker-compose up -d postgres
cargo test -p craft-registry --test integration_tests
```

### CLI Usage

```bash
# Login to registry
craft registry login https://registry.craft.dev

# Create organization
craft registry org create my-org --visibility private

# Create team
craft registry team create my-org/developers --visibility internal

# Publish a harness
craft registry publish --path ./my-harness

# Install a harness
craft registry install my-org/my-harness@1.0.0

# Search
craft registry search "game engine"
```

## API Endpoints

### Authentication
- `POST /api/v1/auth/login` - Login
- `POST /api/v1/auth/register` - Register

### Organizations
- `GET /api/v1/orgs` - List organizations
- `POST /api/v1/orgs` - Create organization
- `GET /api/v1/orgs/:name` - Get organization
- `PUT /api/v1/orgs/:name` - Update organization
- `DELETE /api/v1/orgs/:name` - Delete organization
- `GET /api/v1/orgs/:name/members` - List members
- `POST /api/v1/orgs/:name/members` - Invite member
- `DELETE /api/v1/orgs/:name/members/:username` - Remove member

### Teams
- `GET /api/v1/orgs/:name/teams` - List teams
- `POST /api/v1/orgs/:name/teams` - Create team
- `GET /api/v1/teams/:org/:name` - Get team
- `PUT /api/v1/teams/:org/:name` - Update team
- `DELETE /api/v1/teams/:org/:name` - Delete team
- `GET /api/v1/teams/:org/:name/members` - List members
- `POST /api/v1/teams/:org/:name/members` - Invite member

### Harnesses
- `GET /api/v1/harnesses/search` - Search harnesses
- `POST /api/v1/harnesses` - Create harness
- `GET /api/v1/harnesses/:org/:name` - Get harness
- `PUT /api/v1/harnesses/:org/:name` - Update harness
- `DELETE /api/v1/harnesses/:org/:name` - Delete harness
- `GET /api/v1/harnesses/:org/:name/versions` - List versions
- `POST /api/v1/harnesses/:org/:name/versions` - Publish version
- `GET /api/v1/harnesses/:org/:name/versions/:version` - Get version
- `POST /api/v1/harnesses/:org/:name/versions/:version/yank` - Yank version
- `POST /api/v1/harnesses/:org/:name/versions/:version/unyank` - Unyank version
- `GET /api/v1/harnesses/:org/:name/download/:version` - Download package

### Access Tokens
- `POST /api/v1/user/tokens` - Create token
- `GET /api/v1/user/tokens` - List tokens
- `DELETE /api/v1/user/tokens/:id` - Revoke token

## Configuration

### Environment Variables

```bash
# Required
DATABASE_URL=postgres://user:password@localhost/craft_registry
JWT_PRIVATE_KEY="-----BEGIN RSA PRIVATE KEY-----\n..."
JWT_PUBLIC_KEY="-----BEGIN PUBLIC KEY-----\n..."

# Optional
BIND_ADDRESS=0.0.0.0
PORT=8080
RUST_LOG=info
MAX_PACKAGE_SIZE=104857600  # 100MB
STORAGE_TYPE=local  # or s3
STORAGE_BASE_PATH=/var/lib/craft-registry/packages
```

### Generating JWT Keys

```bash
# Generate RSA private key
openssl genrsa -out private.pem 3072

# Generate public key
openssl rsa -in private.pem -pubout -out public.pem

# For use in environment variables (escape newlines)
export JWT_PRIVATE_KEY=$(cat private.pem | tr '\n' '\\' | sed 's/\\/\\n/g')
export JWT_PUBLIC_KEY=$(cat public.pem | tr '\n' '\\' | sed 's/\\/\\n/g')
```

## Architecture

```text
craft-registry/
├── src/
│   ├── lib.rs           # Library exports
│   ├── auth/            # Authentication (JWT, tokens, passwords)
│   ├── cli/             # CLI client commands
│   ├── db/              # Database models and queries
│   ├── error.rs         # Error types
│   ├── git/             # Git operations
│   ├── server/          # Axum server and handlers
│   ├── storage/         # Storage backends
│   └── version.rs       # Version resolution
├── migrations/          # Database schema migrations
├── tests/               # Integration tests
└── docker-compose.yml   # Local development stack
```

## Testing

### Unit Tests
```bash
cargo test -p craft-registry --lib
```

### Integration Tests
Requires PostgreSQL running locally or via Docker:
```bash
docker-compose up -d postgres
export TEST_DATABASE_URL=postgres://craft:craft_secret@localhost/craft_registry_test
cargo test -p craft-registry --test integration_tests
```

All tests:- Version resolution (5 tests)
- Integration tests (8 tests): publish, install, team ACL, yanking, rate limiting, multi-tenancy, visibility, token lifecycle, audit logging

## License

MIT
