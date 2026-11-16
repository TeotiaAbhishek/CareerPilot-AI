-- F8b: the suggestions assistant. Conversations you can come back to.

CREATE TABLE chats (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title       TEXT NOT NULL CHECK (length(trim(title)) > 0),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX chats_updated_idx ON chats (updated_at DESC);

-- Your questions and the assistant's answers. `tools` lists what the
-- assistant looked at for an answer, shown as chips under it.
CREATE TABLE chat_messages (
    id          BIGSERIAL PRIMARY KEY,
    chat_id     UUID NOT NULL REFERENCES chats (id) ON DELETE CASCADE,
    role        TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
    content     TEXT NOT NULL,
    tools       JSONB NOT NULL DEFAULT '[]',
    failed      BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX chat_messages_chat_idx ON chat_messages (chat_id, id);

ALTER TABLE settings ADD COLUMN assistant_model TEXT NOT NULL DEFAULT 'qwen3:14b';
ALTER TABLE settings ADD COLUMN assistant_think BOOLEAN NOT NULL DEFAULT FALSE;
