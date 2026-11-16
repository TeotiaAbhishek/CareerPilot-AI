-- F8: email drafts and follow-ups. Drafts only: you send them yourself.

CREATE TABLE emails (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id           TEXT NOT NULL REFERENCES jobs (id) ON DELETE CASCADE,
    kind             TEXT NOT NULL CHECK (kind IN ('application', 'follow_up_1', 'follow_up_2', 'thank_you')),
    -- scheduled: waiting for due_at. queued/drafting: the model is on it.
    -- ready: yours to check and send. sent: you marked it sent.
    -- cancelled: no longer needed (the card moved on, or you skipped it).
    status           TEXT NOT NULL DEFAULT 'scheduled' CHECK (status IN (
                         'scheduled', 'queued', 'drafting', 'ready', 'sent', 'cancelled', 'failed')),
    due_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Found in the job ad by code, or typed by you. Never sent to the model.
    to_address       TEXT,
    subject          TEXT NOT NULL DEFAULT '',
    -- [{ "text": "...", "evidence": ["W1.2"] }, ...]
    paragraphs       JSONB NOT NULL DEFAULT '[]',
    flags            JSONB NOT NULL DEFAULT '[]',
    dismissed_flags  TEXT[] NOT NULL DEFAULT '{}',
    model            TEXT,
    error            TEXT,
    sent_at          TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (job_id, kind)
);
CREATE INDEX emails_status_idx ON emails (status, due_at);

-- The second follow-up (14 days after applying) can be switched off.
ALTER TABLE settings ADD COLUMN follow_up_second BOOLEAN NOT NULL DEFAULT TRUE;
