-- How many pages a tailored resume can fill (its PDF is fitted to this).
ALTER TABLE settings ADD COLUMN resume_pages INT NOT NULL DEFAULT 2
    CHECK (resume_pages IN (1, 2));

-- The models I chose after my evals. Without an API key, a paid model falls
-- back to the local model, so a fresh install still works.
ALTER TABLE settings ALTER COLUMN scoring_model SET DEFAULT 'gpt-6-luna';
ALTER TABLE settings ALTER COLUMN drafting_model SET DEFAULT 'claude-sonnet-5-5';
ALTER TABLE settings ALTER COLUMN assistant_model SET DEFAULT 'claude-sonnet-5-5';

-- The settings row already exists. On a fresh install (no jobs yet) give it
-- the new defaults too; an install in use keeps the models you picked.
UPDATE settings
SET scoring_model = 'gpt-6-luna',
    drafting_model = 'claude-sonnet-5-5',
    assistant_model = 'claude-sonnet-5-5'
WHERE NOT EXISTS (SELECT 1 FROM jobs);
