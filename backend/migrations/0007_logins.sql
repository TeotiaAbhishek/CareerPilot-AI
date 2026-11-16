-- F6b: logins, sign-ups and email codes; the reviewer.

-- Connected accounts the agent uses, like your Gmail (read-only). The token
-- is encrypted with VAULT_KEY, like the passwords in `credentials`.
CREATE TABLE integrations (
    name        TEXT PRIMARY KEY,
    account     TEXT NOT NULL DEFAULT '',
    secret      BYTEA NOT NULL,
    nonce       BYTEA NOT NULL CHECK (length(nonce) = 12),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- New run log kinds: `login` (signed in, created an account, used an email
-- code) and `review` (the reviewer's checks).
ALTER TABLE application_events DROP CONSTRAINT application_events_kind_check;
ALTER TABLE application_events ADD CONSTRAINT application_events_kind_check
    CHECK (kind IN ('info', 'fill', 'model', 'written', 'answer', 'click', 'warn', 'error', 'login', 'review'));

-- A second model call checks the model's answers before they go in.
ALTER TABLE settings ADD COLUMN apply_review BOOLEAN NOT NULL DEFAULT TRUE;
