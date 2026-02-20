CREATE TABLE IF NOT EXISTS guild_invites (
    token TEXT PRIMARY KEY,
    guild_id UUID NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at TIMESTAMPTZ NOT NULL,
    max_uses INT,
    uses_count INT NOT NULL DEFAULT 0,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT guild_invites_max_uses_check CHECK (max_uses IS NULL OR max_uses > 0),
    CONSTRAINT guild_invites_uses_count_check CHECK (uses_count >= 0),
    CONSTRAINT guild_invites_uses_cap_check CHECK (max_uses IS NULL OR uses_count <= max_uses)
);

CREATE INDEX IF NOT EXISTS idx_guild_invites_guild_id ON guild_invites(guild_id);
CREATE INDEX IF NOT EXISTS idx_guild_invites_expires_at ON guild_invites(expires_at);
