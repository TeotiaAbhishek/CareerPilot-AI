-- F1: enable the extensions every later feature relies on.
-- pgvector: embeddings (F3, F5). pgcrypto: gen_random_uuid() for ids.
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS pgcrypto;
