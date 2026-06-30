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

