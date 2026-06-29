-- OAuth device authorization grants for CLI login.

CREATE TABLE IF NOT EXISTS device_authorizations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    device_code VARCHAR(128) NOT NULL UNIQUE,
    user_code VARCHAR(32) NOT NULL UNIQUE,
    client_id VARCHAR(128) NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'expired', 'denied')),
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    interval_secs INTEGER NOT NULL DEFAULT 5,
    poll_count INTEGER NOT NULL DEFAULT 0,
    last_poll_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ NOT NULL,
    approved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_device_authorizations_device_code ON device_authorizations(device_code);
CREATE INDEX idx_device_authorizations_user_code ON device_authorizations(user_code);
CREATE INDEX idx_device_authorizations_status ON device_authorizations(status);
CREATE INDEX idx_device_authorizations_expires_at ON device_authorizations(expires_at);

CREATE TRIGGER update_device_authorizations_updated_at BEFORE UPDATE ON device_authorizations
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
