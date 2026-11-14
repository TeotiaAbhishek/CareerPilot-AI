-- F5: tailored resumes and cover letters, drafted by a local model from your
-- master resume and reviewed by you before any agent may use them.

-- Every draft ever made. "Prepare application" queues one row per document;
-- regenerating adds a new row, so earlier drafts are kept.
CREATE TABLE documents (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id             TEXT NOT NULL REFERENCES jobs (id) ON DELETE CASCADE,
    kind               TEXT NOT NULL CHECK (kind IN ('resume', 'cover_letter')),
    -- queued -> drafting -> ready -> approved, or failed.
    status             TEXT NOT NULL DEFAULT 'queued'
                       CHECK (status IN ('queued', 'drafting', 'ready', 'approved', 'failed')),
    -- Extra guidance for the model, e.g. "focus on Power BI".
    note               TEXT,
    model              TEXT,
    prompt_version     TEXT,
    resume_version_id  UUID REFERENCES resume_versions (id) ON DELETE SET NULL,
    -- The draft itself (structured: sections, bullets and the evidence each cites).
    content            JSONB,
    -- Problems the checker found, recomputed on every save.
    flags              JSONB NOT NULL DEFAULT '[]',
    -- Flags you looked at and accepted ("this is true, keep it").
    dismissed_flags    TEXT[] NOT NULL DEFAULT '{}',
    edited             BOOLEAN NOT NULL DEFAULT FALSE,
    error              TEXT,
    duration_ms        INTEGER,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    approved_at        TIMESTAMPTZ
);
CREATE INDEX documents_job_idx ON documents (job_id, kind, created_at DESC);
CREATE INDEX documents_queue_idx ON documents (created_at) WHERE status = 'queued';

-- Which model drafts documents, and whether it may "think" first (slower, better).
ALTER TABLE settings ADD COLUMN drafting_model TEXT NOT NULL DEFAULT 'qwen3:14b';
ALTER TABLE settings ADD COLUMN drafting_think BOOLEAN NOT NULL DEFAULT TRUE;
