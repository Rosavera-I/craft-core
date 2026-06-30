-- Organization ownership and stricter org/team naming.

ALTER TABLE organizations
    ADD COLUMN IF NOT EXISTS owner_id UUID REFERENCES users(id) ON DELETE SET NULL;

ALTER TABLE teams
    ADD COLUMN IF NOT EXISTS display_name VARCHAR(128);

ALTER TABLE org_members
    DROP CONSTRAINT IF EXISTS org_members_role_check,
    ADD CONSTRAINT org_members_role_check
        CHECK (role IN ('owner', 'admin', 'maintainer', 'member'));

ALTER TABLE team_members
    DROP CONSTRAINT IF EXISTS team_members_role_check,
    ADD CONSTRAINT team_members_role_check
        CHECK (role IN ('maintainer', 'member', 'admin', 'owner'));

CREATE INDEX IF NOT EXISTS idx_orgs_owner_id ON organizations(owner_id);

CREATE TABLE IF NOT EXISTS org_invitations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    org_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    email VARCHAR(255) NOT NULL,
    role VARCHAR(20) NOT NULL DEFAULT 'member'
        CHECK (role IN ('owner', 'admin', 'maintainer', 'member')),
    invited_by UUID REFERENCES users(id) ON DELETE SET NULL,
    accepted_by UUID REFERENCES users(id) ON DELETE SET NULL,
    accepted_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(org_id, email)
);

CREATE INDEX IF NOT EXISTS idx_org_invitations_org_id ON org_invitations(org_id);
CREATE INDEX IF NOT EXISTS idx_org_invitations_email ON org_invitations(email);
