-- F2: jobs, saved searches, scrape runs, and tables for data imported from v1.

-- What to search SEEK for. Edited on the Settings page.
CREATE TABLE searches (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    keywords    TEXT NOT NULL CHECK (length(trim(keywords)) > 0),
    location    TEXT NOT NULL CHECK (length(trim(location)) > 0),
    max_pages   INTEGER NOT NULL DEFAULT 3 CHECK (max_pages BETWEEN 1 AND 10),
    enabled     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (keywords, location)
);

-- The same searches v1 had in config/searches.yaml.
INSERT INTO searches (keywords, location, max_pages) VALUES
    ('Data Analyst', 'Sydney NSW', 3),
    ('Data Scientist', 'Sydney NSW', 3);

-- One row per job. The id is SEEK's own job id, so re-scraping never duplicates.
CREATE TABLE jobs (
    id                 TEXT PRIMARY KEY,
    source             TEXT NOT NULL DEFAULT 'seek',
    title              TEXT NOT NULL,
    company            TEXT,
    location           TEXT,
    work_type          TEXT,
    classification     TEXT,
    subclassification  TEXT,
    salary             TEXT,
    short_description  TEXT,
    description_text   TEXT,          -- plain text, for search and (in F3) the model
    description_html   TEXT,          -- sanitised HTML, for display
    posted_label       TEXT,          -- what SEEK showed, e.g. "3d ago"
    listed_at          TIMESTAMPTZ,   -- our best estimate of when it was posted
    job_url            TEXT NOT NULL,
    apply_url          TEXT,
    apply_type         TEXT,          -- e.g. "Quick apply" or "Apply"
    first_seen_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    viewed_at          TIMESTAMPTZ,   -- first time you opened it in the UI
    saved              BOOLEAN NOT NULL DEFAULT FALSE,
    hidden             BOOLEAN NOT NULL DEFAULT FALSE,
    scrape_status      TEXT NOT NULL DEFAULT 'ok' CHECK (scrape_status IN ('ok', 'failed')),
    scrape_error       TEXT,

    -- Full-text search: title matters most, then company, then the description.
    search_vector TSVECTOR GENERATED ALWAYS AS (
        setweight(to_tsvector('english', coalesce(title, '')), 'A') ||
        setweight(to_tsvector('english', coalesce(company, '')), 'B') ||
        setweight(to_tsvector('english', coalesce(description_text, '')), 'C')
    ) STORED
);

CREATE INDEX jobs_search_idx ON jobs USING GIN (search_vector);
CREATE INDEX jobs_listed_idx ON jobs (hidden, (coalesce(listed_at, first_seen_at)) DESC);
CREATE INDEX jobs_work_type_idx ON jobs (work_type);

-- Every scrape, manual or scheduled, with live progress.
CREATE TABLE scrape_runs (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    trigger       TEXT NOT NULL CHECK (trigger IN ('manual', 'schedule')),
    status        TEXT NOT NULL DEFAULT 'running'
                  CHECK (status IN ('running', 'succeeded', 'failed', 'rate_limited')),
    message       TEXT,
    pages_fetched INTEGER NOT NULL DEFAULT 0,
    jobs_found    INTEGER NOT NULL DEFAULT 0,
    jobs_new      INTEGER NOT NULL DEFAULT 0,
    jobs_failed   INTEGER NOT NULL DEFAULT 0,
    started_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at   TIMESTAMPTZ
);

-- The database itself guarantees only one scrape runs at a time.
CREATE UNIQUE INDEX scrape_runs_one_running ON scrape_runs ((TRUE)) WHERE status = 'running';
CREATE INDEX scrape_runs_started_idx ON scrape_runs (started_at DESC);

-- Data imported from v1 (Supabase). Kept separate so v2 tables stay clean.
-- legacy_job_analysis becomes the baseline to beat in F3's evals.
CREATE TABLE legacy_job_analysis (
    job_id           TEXT PRIMARY KEY REFERENCES jobs (id) ON DELETE CASCADE,
    relevance_score  SMALLINT NOT NULL,
    kept             BOOLEAN NOT NULL,
    analysis         JSONB NOT NULL,
    model            TEXT NOT NULL,
    analysed_at      TIMESTAMPTZ NOT NULL
);

CREATE TABLE legacy_applications (
    job_id      TEXT PRIMARY KEY REFERENCES jobs (id) ON DELETE CASCADE,
    status      TEXT NOT NULL,
    applied_at  TIMESTAMPTZ,
    updated_at  TIMESTAMPTZ NOT NULL
);
