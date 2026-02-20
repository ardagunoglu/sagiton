ALTER TABLE threads
ADD COLUMN IF NOT EXISTS direct_status TEXT,
ADD COLUMN IF NOT EXISTS requested_by UUID REFERENCES users(id) ON DELETE SET NULL,
ADD COLUMN IF NOT EXISTS accepted_at TIMESTAMPTZ;

UPDATE threads
SET direct_status = 'ACTIVE'
WHERE direct_status IS NULL;

ALTER TABLE threads
ALTER COLUMN direct_status SET DEFAULT 'ACTIVE',
ALTER COLUMN direct_status SET NOT NULL;

ALTER TABLE threads
DROP CONSTRAINT IF EXISTS threads_direct_pair_check;

ALTER TABLE threads
ADD CONSTRAINT threads_direct_pair_check CHECK (
    (
        kind = 'DIRECT'
        AND name IS NULL
        AND owner_id IS NULL
        AND direct_a_user_id IS NOT NULL
        AND direct_b_user_id IS NOT NULL
        AND direct_status IN ('PENDING', 'ACTIVE')
        AND (requested_by IS NULL OR requested_by = direct_a_user_id OR requested_by = direct_b_user_id)
    )
    OR
    (
        kind = 'GROUP'
        AND name IS NOT NULL
        AND owner_id IS NOT NULL
        AND direct_a_user_id IS NULL
        AND direct_b_user_id IS NULL
        AND direct_status = 'ACTIVE'
        AND requested_by IS NULL
    )
);

CREATE TABLE IF NOT EXISTS friendships (
    user_low_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_high_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_low_id, user_high_id),
    CONSTRAINT friendships_pair_order_check CHECK (user_low_id < user_high_id)
);

CREATE INDEX IF NOT EXISTS idx_friendships_user_high_id ON friendships(user_high_id);

CREATE TABLE IF NOT EXISTS group_invites (
    token TEXT PRIMARY KEY,
    thread_id UUID NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at TIMESTAMPTZ NOT NULL,
    max_uses INT,
    used_count INT NOT NULL DEFAULT 0,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT group_invites_max_uses_check CHECK (max_uses IS NULL OR max_uses > 0),
    CONSTRAINT group_invites_used_count_check CHECK (used_count >= 0)
);

CREATE INDEX IF NOT EXISTS idx_group_invites_thread_id ON group_invites(thread_id);
