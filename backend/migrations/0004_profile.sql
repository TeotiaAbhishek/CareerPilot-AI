-- F4: your resume (with history), application facts, answer bank, and the password vault.

-- Every saved version of your master resume. The newest row is the current one.
CREATE TABLE resume_versions (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    content     TEXT NOT NULL CHECK (length(content) > 0),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX resume_versions_created_idx ON resume_versions (created_at DESC);

-- Which resume version produced each score.
ALTER TABLE job_scores ADD COLUMN resume_version_id UUID REFERENCES resume_versions (id) ON DELETE SET NULL;

-- The facts application forms always ask for. Exactly one row.
CREATE TABLE profile_facts (
    id                          BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (id),
    -- Contact (never sent to the scoring model)
    full_name                   TEXT NOT NULL DEFAULT '',
    email                       TEXT NOT NULL DEFAULT '',
    phone                       TEXT NOT NULL DEFAULT '',
    linkedin_url                TEXT NOT NULL DEFAULT '',
    website_url                 TEXT NOT NULL DEFAULT '',
    street_address              TEXT NOT NULL DEFAULT '',
    postcode                    TEXT NOT NULL DEFAULT '',
    -- Location
    city                        TEXT NOT NULL DEFAULT '',
    state                       TEXT NOT NULL DEFAULT '',
    country                     TEXT NOT NULL DEFAULT 'Australia',
    willing_to_relocate         BOOLEAN NOT NULL DEFAULT FALSE,
    -- Work rights
    australian_citizen          BOOLEAN NOT NULL DEFAULT FALSE,
    australian_permanent_resident BOOLEAN NOT NULL DEFAULT FALSE,
    new_zealand_citizen         BOOLEAN NOT NULL DEFAULT FALSE,
    australian_work_rights      BOOLEAN NOT NULL DEFAULT FALSE,
    visa_type                   TEXT NOT NULL DEFAULT '',
    work_rights_expiry          DATE,
    -- Application details
    notice_period               TEXT NOT NULL DEFAULT '',
    salary_expectation          TEXT NOT NULL DEFAULT '',
    available_from              DATE,
    updated_at                  TIMESTAMPTZ NOT NULL DEFAULT now()
);
INSERT INTO profile_facts DEFAULT VALUES;

-- Answers to screening questions. You add them here; in F6 the form-fill
-- agent looks them up, and saves the new answers you give it.
CREATE TABLE answers (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    question      TEXT NOT NULL CHECK (length(question) > 0),
    answer        TEXT NOT NULL CHECK (length(answer) > 0),
    tags          TEXT[] NOT NULL DEFAULT '{}',
    source        TEXT NOT NULL DEFAULT 'you' CHECK (source IN ('you', 'agent')),
    times_used    INTEGER NOT NULL DEFAULT 0,
    last_used_at  TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    search_vector TSVECTOR GENERATED ALWAYS AS (
        setweight(to_tsvector('english', question), 'A') ||
        setweight(to_tsvector('english', answer), 'B')
    ) STORED
);
CREATE INDEX answers_search_idx ON answers USING GIN (search_vector);

-- One login per site. The password is encrypted with AES-256-GCM by the API;
-- the key (VAULT_KEY) is never stored in the database.
CREATE TABLE credentials (
    id             UUID PRIMARY KEY,
    site           TEXT NOT NULL UNIQUE,
    login_url      TEXT NOT NULL DEFAULT '',
    username       TEXT NOT NULL,
    secret         BYTEA NOT NULL,
    nonce          BYTEA NOT NULL CHECK (length(nonce) = 12),
    key_version    SMALLINT NOT NULL DEFAULT 1,
    notes          TEXT NOT NULL DEFAULT '',
    created_by     TEXT NOT NULL DEFAULT 'you' CHECK (created_by IN ('you', 'agent')),
    last_used_at   TIMESTAMPTZ,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Every time a password is revealed, copied or used.
CREATE TABLE vault_access_log (
    id             BIGSERIAL PRIMARY KEY,
    credential_id  UUID REFERENCES credentials (id) ON DELETE SET NULL,
    site           TEXT NOT NULL,
    action         TEXT NOT NULL CHECK (action IN ('create', 'update', 'reveal', 'delete', 'agent_use')),
    at             TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX vault_access_log_at_idx ON vault_access_log (at DESC);
