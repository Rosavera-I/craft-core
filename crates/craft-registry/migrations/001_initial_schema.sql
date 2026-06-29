-- Initial schema for CRAFT Cloud Harness Registry
-- Organizations, teams, members, harnesses, versions, and access tokens

-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Organizations table
CREATE TABLE IF NOT EXISTS organizations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(64) NOT NULL UNIQUE,
    display_name VARCHAR(128),
    description TEXT,
    avatar_url TEXT,
    website_url TEXT,
    visibility VARCHAR(20) NOT NULL DEFAULT 'public' CHECK (visibility IN ('public', 'internal', 'private')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX idx_orgs_name ON organizations(name);
CREATE INDEX idx_orgs_visibility ON organizations(visibility) WHERE deleted_at IS NULL;

-- Teams table (within organizations)
CREATE TABLE IF NOT EXISTS teams (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    org_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name VARCHAR(64) NOT NULL,
    description TEXT,
    visibility VARCHAR(20) NOT NULL DEFAULT 'private' CHECK (visibility IN ('public', 'internal', 'private')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ,
    UNIQUE(org_id, name)
);

CREATE INDEX idx_teams_org_id ON teams(org_id);
CREATE INDEX idx_teams_visibility ON teams(visibility) WHERE deleted_at IS NULL;

-- Users table (linked to external auth, but we store minimal info)
CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    username VARCHAR(64) NOT NULL UNIQUE,
    email VARCHAR(255) NOT NULL UNIQUE,
    display_name VARCHAR(128),
    avatar_url TEXT,
    password_hash TEXT, -- Nullable for SSO-only users
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    is_admin BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_login_at TIMESTAMPTZ
);

CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_email ON users(email);

-- Organization membership
CREATE TABLE IF NOT EXISTS org_members (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    org_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role VARCHAR(20) NOT NULL DEFAULT 'member' CHECK (role IN ('member', 'maintainer', 'admin')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(org_id, user_id)
);

CREATE INDEX idx_org_members_org_id ON org_members(org_id);
CREATE INDEX idx_org_members_user_id ON org_members(user_id);

-- Team membership
CREATE TABLE IF NOT EXISTS team_members (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    team_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role VARCHAR(20) NOT NULL DEFAULT 'member' CHECK (role IN ('member', 'maintainer', 'admin')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(team_id, user_id)
);

CREATE INDEX idx_team_members_team_id ON team_members(team_id);
CREATE INDEX idx_team_members_user_id ON team_members(user_id);

-- Harnesses (packages)
CREATE TABLE IF NOT EXISTS harnesses (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    org_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    team_id UUID REFERENCES teams(id) ON DELETE SET NULL,
    name VARCHAR(128) NOT NULL,
    description TEXT,
    visibility VARCHAR(20) NOT NULL DEFAULT 'private' CHECK (visibility IN ('private', 'internal', 'public')),
    keywords TEXT[], -- Array of keywords for search
    metadata JSONB, -- Flexible metadata storage
    git_repository_url TEXT,
    git_default_branch VARCHAR(64) DEFAULT 'main',
    total_downloads BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ,
    UNIQUE(org_id, name)
);

CREATE INDEX idx_harnesses_org_id ON harnesses(org_id);
CREATE INDEX idx_harnesses_team_id ON harnesses(team_id);
CREATE INDEX idx_harnesses_visibility ON harnesses(visibility) WHERE deleted_at IS NULL;
CREATE INDEX idx_harnesses_keywords ON harnesses USING GIN(keywords);
CREATE INDEX idx_harnesses_metadata ON harnesses USING GIN(metadata);

-- Harness versions
CREATE TABLE IF NOT EXISTS harness_versions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    harness_id UUID NOT NULL REFERENCES harnesses(id) ON DELETE CASCADE,
    version VARCHAR(64) NOT NULL, -- Semver string
    major INTEGER NOT NULL,
    minor INTEGER NOT NULL,
    patch INTEGER NOT NULL,
    prerelease TEXT,
    build_metadata TEXT,
    git_ref TEXT, -- Git tag or commit
    git_commit_sha VARCHAR(64),
    description TEXT,
    readme_content TEXT,
    package_size_bytes BIGINT,
    content_sha256 VARCHAR(64) NOT NULL, -- SHA-256 of tarball
    storage_path TEXT NOT NULL, -- Path to stored tarball
    download_count BIGINT NOT NULL DEFAULT 0,
    is_yanked BOOLEAN NOT NULL DEFAULT FALSE,
    yanked_reason TEXT,
    published_by UUID REFERENCES users(id),
    published_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(harness_id, version)
);

CREATE INDEX idx_harness_versions_harness_id ON harness_versions(harness_id);
CREATE INDEX idx_harness_versions_version ON harness_versions(version);
CREATE INDEX idx_harness_versions_semver ON harness_versions(major, minor, patch);
CREATE INDEX idx_harness_versions_git_ref ON harness_versions(git_ref);
CREATE INDEX idx_harness_versions_content_sha256 ON harness_versions(content_sha256);
CREATE INDEX idx_harness_versions_published_at ON harness_versions(published_at);

-- Access tokens for CI/CD and CLI publishing
CREATE TABLE IF NOT EXISTS access_tokens (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    org_id UUID REFERENCES organizations(id) ON DELETE CASCADE, -- Optional: org-scoped token
    name VARCHAR(128) NOT NULL,
    token_hash VARCHAR(64) NOT NULL UNIQUE, -- SHA-256 hash of the token
    token_prefix VARCHAR(16) NOT NULL, -- First few chars for identification
    scopes TEXT[] NOT NULL DEFAULT ARRAY['read'], -- Array of scopes: read, write, admin
    expires_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at TIMESTAMPTZ
);

CREATE INDEX idx_access_tokens_user_id ON access_tokens(user_id);
CREATE INDEX idx_access_tokens_org_id ON access_tokens(org_id);
CREATE INDEX idx_access_tokens_token_hash ON access_tokens(token_hash);

-- Audit logs for all significant actions
CREATE TABLE IF NOT EXISTS audit_logs (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    org_id UUID REFERENCES organizations(id) ON DELETE SET NULL,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    action VARCHAR(64) NOT NULL, -- e.g., 'harness.publish', 'team.invite', 'token.create'
    resource_type VARCHAR(64) NOT NULL, -- e.g., 'harness', 'team', 'user'
    resource_id UUID, -- ID of the affected resource
    details JSONB, -- Additional context
    ip_address INET,
    user_agent TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_audit_logs_org_id ON audit_logs(org_id);
CREATE INDEX idx_audit_logs_user_id ON audit_logs(user_id);
CREATE INDEX idx_audit_logs_action ON audit_logs(action);
CREATE INDEX idx_audit_logs_created_at ON audit_logs(created_at);
CREATE INDEX idx_audit_logs_details ON audit_logs USING GIN(details);

-- Webhooks for CI/CD integration
CREATE TABLE IF NOT EXISTS webhooks (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    org_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    harness_id UUID REFERENCES harnesses(id) ON DELETE CASCADE, -- Optional: specific to a harness
    name VARCHAR(128) NOT NULL,
    url TEXT NOT NULL,
    secret TEXT, -- HMAC secret for signature verification
    events TEXT[] NOT NULL, -- Array of events: harness.published, harness.yanked, etc.
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    last_triggered_at TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_webhooks_org_id ON webhooks(org_id);
CREATE INDEX idx_webhooks_harness_id ON webhooks(harness_id);
CREATE INDEX idx_webhooks_events ON webhooks USING GIN(events);

-- Rate limiting table (for API rate limiting)
CREATE TABLE IF NOT EXISTS rate_limit_entries (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    key VARCHAR(255) NOT NULL UNIQUE, -- Composite key: "user:{id}" or "ip:{ip}"
    window_start TIMESTAMPTZ NOT NULL,
    request_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_rate_limit_entries_key ON rate_limit_entries(key);
CREATE INDEX idx_rate_limit_entries_window_start ON rate_limit_entries(window_start);

-- Update triggers for updated_at
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

CREATE TRIGGER update_organizations_updated_at BEFORE UPDATE ON organizations
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_teams_updated_at BEFORE UPDATE ON teams
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_users_updated_at BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_org_members_updated_at BEFORE UPDATE ON org_members
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_team_members_updated_at BEFORE UPDATE ON team_members
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_harnesses_updated_at BEFORE UPDATE ON harnesses
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_access_tokens_updated_at BEFORE UPDATE ON access_tokens
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_webhooks_updated_at BEFORE UPDATE ON webhooks
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_rate_limit_entries_updated_at BEFORE UPDATE ON rate_limit_entries
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
