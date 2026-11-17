-- F10: Claude and OpenAI next to Ollama, what they cost, and how they compare.

-- One row per paid model call: what it used and what it cost.
CREATE TABLE llm_usage (
    id            BIGSERIAL PRIMARY KEY,
    provider      TEXT NOT NULL CHECK (provider IN ('claude', 'openai')),
    model         TEXT NOT NULL,
    input_tokens  INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    cost_usd      NUMERIC(10, 6) NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX llm_usage_month_idx ON llm_usage (provider, created_at);

-- The most each provider may cost per calendar month. Past it, calls go to
-- the local apply model instead.
CREATE TABLE llm_caps (
    provider          TEXT PRIMARY KEY CHECK (provider IN ('claude', 'openai')),
    monthly_cap_usd   NUMERIC(8, 2) NOT NULL DEFAULT 10
);
INSERT INTO llm_caps (provider) VALUES ('claude'), ('openai');

-- What a score cost, and the model's own score before the seniority cap.
ALTER TABLE job_scores ADD COLUMN cost_usd NUMERIC(10, 6);
ALTER TABLE job_scores ADD COLUMN model_score SMALLINT;
ALTER TABLE job_scores ADD COLUMN seniority_note TEXT;

-- Tailored resumes drafted only to compare models. Kept apart from
-- `documents`, so they're never approved or used to apply.
CREATE TABLE draft_comparisons (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    batch_id     UUID NOT NULL,
    job_id       TEXT NOT NULL REFERENCES jobs (id) ON DELETE CASCADE,
    model        TEXT NOT NULL,
    -- Job order, then model order, as you picked them.
    position     INTEGER NOT NULL,
    status       TEXT NOT NULL DEFAULT 'queued'
                 CHECK (status IN ('queued', 'drafting', 'ready', 'failed')),
    content      JSONB,
    flags        JSONB NOT NULL DEFAULT '[]',
    words        INTEGER,
    duration_ms  INTEGER,
    cost_usd     NUMERIC(10, 6),
    error        TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX draft_comparisons_batch_idx ON draft_comparisons (batch_id, position);
