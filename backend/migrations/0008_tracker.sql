-- F7: one-click Apply and the application tracker.

-- One row per job on your board. `active` means it hasn't been sent yet: the
-- board works out "Ready to apply", "Applying" or "Your turn" from its
-- documents and its latest run. After Submit, you move it along yourself.
CREATE TABLE tracker (
    job_id            TEXT PRIMARY KEY REFERENCES jobs (id) ON DELETE CASCADE,
    stage             TEXT NOT NULL DEFAULT 'active' CHECK (stage IN (
                          'active', 'submitted', 'interview', 'offer', 'rejected', 'withdrawn')),
    -- One-click Apply is waiting for the resume: apply as soon as it's approved.
    one_click         BOOLEAN NOT NULL DEFAULT FALSE,
    -- Why one-click stopped, for the card ("The resume draft failed").
    note              TEXT,
    applied_at        TIMESTAMPTZ,
    -- First reply: the first move from Submitted to Interview, Offer or Rejected.
    replied_at        TIMESTAMPTZ,
    stage_changed_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX tracker_stage_idx ON tracker (stage, stage_changed_at DESC);
CREATE INDEX tracker_one_click_idx ON tracker (job_id) WHERE one_click;

-- Every stage change, for the history on a card.
CREATE TABLE tracker_moves (
    id          BIGSERIAL PRIMARY KEY,
    job_id      TEXT NOT NULL REFERENCES jobs (id) ON DELETE CASCADE,
    from_stage  TEXT,
    to_stage    TEXT NOT NULL,
    at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX tracker_moves_job_idx ON tracker_moves (job_id, at);

-- One-click Apply approves a clean resume draft by itself.
ALTER TABLE settings ADD COLUMN auto_approve BOOLEAN NOT NULL DEFAULT TRUE;

-- Your history so far: v1's applied jobs, then the ones you submitted with the agent.
INSERT INTO tracker (job_id, stage, applied_at, stage_changed_at, created_at)
SELECT job_id, 'submitted', coalesce(applied_at, updated_at), coalesce(applied_at, updated_at),
       coalesce(applied_at, updated_at)
FROM legacy_applications
ON CONFLICT (job_id) DO NOTHING;

INSERT INTO tracker (job_id, stage, applied_at, stage_changed_at, created_at)
SELECT DISTINCT ON (job_id) job_id, 'submitted', submitted_at, submitted_at, created_at
FROM applications
WHERE status = 'submitted' AND NOT practice AND job_id <> 'practice' AND submitted_at IS NOT NULL
ORDER BY job_id, submitted_at
ON CONFLICT (job_id) DO NOTHING;

-- Jobs you've started on: prepared documents or an unfinished run.
INSERT INTO tracker (job_id, created_at)
SELECT job_id, min(created_at)
FROM (
    SELECT job_id, created_at FROM documents
    UNION ALL
    SELECT job_id, created_at FROM applications WHERE NOT practice
) started
WHERE job_id <> 'practice'
GROUP BY job_id
ON CONFLICT (job_id) DO NOTHING;
