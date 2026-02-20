CREATE TABLE IF NOT EXISTS threads (
    id UUID PRIMARY KEY,
    kind TEXT NOT NULL,
    name TEXT,
    owner_id UUID REFERENCES users(id) ON DELETE CASCADE,
    direct_a_user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    direct_b_user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT threads_kind_check CHECK (kind IN ('DIRECT', 'GROUP')),
    CONSTRAINT threads_direct_pair_check CHECK (
        (kind = 'DIRECT' AND name IS NULL AND owner_id IS NULL AND direct_a_user_id IS NOT NULL AND direct_b_user_id IS NOT NULL)
        OR
        (kind = 'GROUP' AND name IS NOT NULL AND owner_id IS NOT NULL AND direct_a_user_id IS NULL AND direct_b_user_id IS NULL)
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_threads_direct_pair_unique
    ON threads(kind, direct_a_user_id, direct_b_user_id);

CREATE TABLE IF NOT EXISTS thread_members (
    thread_id UUID NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (thread_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_thread_members_user_id ON thread_members(user_id);

CREATE TABLE IF NOT EXISTS thread_messages (
    id UUID PRIMARY KEY,
    thread_id UUID NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    author_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    edited_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_thread_messages_thread_created_id
    ON thread_messages(thread_id, created_at DESC, id DESC);
