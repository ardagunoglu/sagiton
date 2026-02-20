CREATE TABLE IF NOT EXISTS friend_requests (
    from_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    to_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'PENDING',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    responded_at TIMESTAMPTZ,
    PRIMARY KEY (from_user_id, to_user_id),
    CONSTRAINT friend_requests_not_self_check CHECK (from_user_id <> to_user_id),
    CONSTRAINT friend_requests_status_check CHECK (status IN ('PENDING', 'ACCEPTED', 'REJECTED'))
);

CREATE INDEX IF NOT EXISTS idx_friend_requests_to_status
    ON friend_requests(to_user_id, status);

CREATE TABLE IF NOT EXISTS friends (
    user_id_a UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_id_b UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id_a, user_id_b),
    CONSTRAINT friends_pair_order_check CHECK (user_id_a < user_id_b)
);

CREATE INDEX IF NOT EXISTS idx_friends_user_id_b ON friends(user_id_b);

DO $$
BEGIN
    IF to_regclass('friendships') IS NOT NULL THEN
        INSERT INTO friends (user_id_a, user_id_b, created_at)
        SELECT user_low_id, user_high_id, created_at
        FROM friendships
        ON CONFLICT DO NOTHING;
    END IF;
END $$;
