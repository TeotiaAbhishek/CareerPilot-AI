-- F3: relevance scoring with a local model, your labels, and settings.

-- App-wide settings, edited on the Settings page. Always exactly one row.
CREATE TABLE settings (
    id                   BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (id),
    scoring_enabled      BOOLEAN NOT NULL DEFAULT TRUE,
    scoring_model        TEXT NOT NULL DEFAULT 'qwen3:14b',
    relevance_threshold  INTEGER NOT NULL DEFAULT 60 CHECK (relevance_threshold BETWEEN 0 AND 100),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);
INSERT INTO settings DEFAULT VALUES;

-- Every score ever produced. A job can have many: one per model and prompt
-- version, and again when re-scored. The UI shows the latest for the active model.
CREATE TABLE job_scores (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id          TEXT NOT NULL REFERENCES jobs (id) ON DELETE CASCADE,
    model           TEXT NOT NULL,
    prompt_version  TEXT NOT NULL,
    score           SMALLINT NOT NULL CHECK (score BETWEEN 0 AND 100),
    summary         TEXT NOT NULL,
    matched         JSONB NOT NULL DEFAULT '[]',
    missing         JSONB NOT NULL DEFAULT '[]',
    unknown         JSONB NOT NULL DEFAULT '[]',
    hard_blockers   JSONB NOT NULL DEFAULT '[]',
    duration_ms     INTEGER NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX job_scores_lookup_idx ON job_scores (job_id, model, prompt_version, created_at DESC);

-- Jobs waiting to be scored with a specific model (Re-score button, eval runs).
-- New jobs don't need a row here: the worker finds unscored jobs by itself.
CREATE TABLE score_requests (
    job_id        TEXT NOT NULL REFERENCES jobs (id) ON DELETE CASCADE,
    model         TEXT NOT NULL,
    requested_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (job_id, model)
);

-- Your own judgement of a job. The ground truth for evals.
CREATE TABLE job_labels (
    job_id      TEXT PRIMARY KEY REFERENCES jobs (id) ON DELETE CASCADE,
    label       TEXT NOT NULL CHECK (label IN ('apply', 'maybe', 'no')),
    labelled_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Why the last scoring attempt failed, so the worker doesn't retry it in a loop.
ALTER TABLE jobs ADD COLUMN score_error TEXT;
ALTER TABLE jobs ADD COLUMN score_failed_at TIMESTAMPTZ;
