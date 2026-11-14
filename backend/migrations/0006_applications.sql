-- F6a: apply runs. One row per time you click Apply on a job.
--
--   queued -> running -> ready_to_submit -> submitted
--                     -> waiting_for_you (a question only you can answer) -> queued
--                     -> needs_manual    (CAPTCHA, login, a form it can't handle)
--                     -> failed / cancelled
CREATE TABLE applications (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id             TEXT NOT NULL REFERENCES jobs (id) ON DELETE CASCADE,
    status             TEXT NOT NULL DEFAULT 'queued' CHECK (status IN (
                           'queued', 'running', 'waiting_for_you', 'ready_to_submit',
                           'needs_manual', 'submitted', 'failed', 'cancelled')),
    -- Why it needs you, e.g. "captcha" or "login", plus a sentence for the UI.
    manual_reason      TEXT CHECK (manual_reason IN ('captcha', 'login', 'stuck', 'cover_letter', 'step_limit')),
    message            TEXT,
    start_url          TEXT NOT NULL,
    current_url        TEXT,
    -- The browser tab it runs in, so a run can carry on after your answers.
    tab_id             TEXT,
    resume_id          UUID REFERENCES documents (id) ON DELETE SET NULL,
    cover_letter_id    UUID REFERENCES documents (id) ON DELETE SET NULL,
    steps              INTEGER NOT NULL DEFAULT 0,
    practice           BOOLEAN NOT NULL DEFAULT FALSE,
    started_at         TIMESTAMPTZ,
    ready_at           TIMESTAMPTZ,
    submitted_at       TIMESTAMPTZ,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX applications_status_idx ON applications (status, created_at);
CREATE INDEX applications_job_idx ON applications (job_id, created_at DESC);
-- At most one active run per job.
CREATE UNIQUE INDEX applications_one_active_idx ON applications (job_id)
    WHERE status IN ('queued', 'running', 'waiting_for_you', 'ready_to_submit');

-- What happened, step by step. The run view streams these live.
CREATE TABLE application_events (
    id              BIGSERIAL PRIMARY KEY,
    application_id  UUID NOT NULL REFERENCES applications (id) ON DELETE CASCADE,
    kind            TEXT NOT NULL CHECK (kind IN ('info', 'fill', 'model', 'written', 'answer', 'click', 'warn', 'error')),
    message         TEXT NOT NULL,
    at              TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX application_events_app_idx ON application_events (application_id, id);

-- Questions the agent couldn't answer from your facts, answer bank or resume.
CREATE TABLE application_questions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    application_id  UUID NOT NULL REFERENCES applications (id) ON DELETE CASCADE,
    field_id        TEXT NOT NULL,
    label           TEXT NOT NULL,
    kind            TEXT NOT NULL,
    options         TEXT[] NOT NULL DEFAULT '{}',
    answer          TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    answered_at     TIMESTAMPTZ
);
CREATE INDEX application_questions_app_idx ON application_questions (application_id);

-- Apply settings (Settings page).
ALTER TABLE settings
    ADD COLUMN apply_parallel   INTEGER NOT NULL DEFAULT 3 CHECK (apply_parallel BETWEEN 1 AND 5),
    ADD COLUMN apply_daily_cap  INTEGER NOT NULL DEFAULT 10 CHECK (apply_daily_cap BETWEEN 1 AND 50),
    ADD COLUMN apply_model      TEXT NOT NULL DEFAULT 'qwen3:14b',
    ADD COLUMN apply_think      BOOLEAN NOT NULL DEFAULT FALSE;

-- A hidden job for practice runs against the worker's practice form.
INSERT INTO jobs (id, source, title, company, location, work_type, description_text, job_url, apply_type, hidden)
VALUES ('practice', 'practice', 'Data Analyst (practice)', 'Practice Co', 'Sydney NSW', 'Full time',
        'A practice job. Data Analyst working with SQL, Python and Power BI dashboards for stakeholders.',
        'http://127.0.0.1:8090/practice/quick-apply', 'Quick apply', TRUE)
ON CONFLICT (id) DO NOTHING;
