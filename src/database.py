import os
from pathlib import Path
from psycopg.types.json import Jsonb
from psycopg.rows import dict_row

import psycopg
from dotenv import load_dotenv


PROJECT_ROOT = Path(__file__).resolve().parent.parent

load_dotenv(
    PROJECT_ROOT / ".env",
    override=True,
)


def get_connection():
    return psycopg.connect(
        host=os.getenv("DB_HOST"),
        port=os.getenv("DB_PORT"),
        dbname=os.getenv("DB_NAME"),
        user=os.getenv("DB_USER"),
        password=os.getenv("DB_PASSWORD"),
    )

def create_tables():
    connection = get_connection()
    cursor = connection.cursor()

    cursor.execute(
        """
        CREATE TABLE IF NOT EXISTS jobs (
            job_id TEXT PRIMARY KEY,
            title TEXT,
            company TEXT,
            location TEXT,
            work_type TEXT,
            classification TEXT,
            subclassification TEXT,
            short_description TEXT,
            description TEXT,
            posted TEXT,
            job_url TEXT,
            apply_url TEXT,
            apply_type TEXT,
            date_found TIMESTAMPTZ,
            scrape_status TEXT,
            scrape_error TEXT
        )
        """
    )
    cursor.execute(
        """
        CREATE TABLE IF NOT EXISTS job_analysis (
            job_id TEXT PRIMARY KEY
                REFERENCES jobs(job_id)
                ON DELETE CASCADE,

            relevance_score SMALLINT NOT NULL
                CHECK (relevance_score BETWEEN 0 AND 100),

            kept BOOLEAN NOT NULL DEFAULT TRUE,

            analysis JSONB NOT NULL,

            model TEXT NOT NULL,

            analysed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        """
    )

    connection.commit()
    cursor.close()
    connection.close()


def job_exists(job_id):
    connection = get_connection()
    cursor = connection.cursor()

    cursor.execute(
        """
        SELECT 1
        FROM jobs
        WHERE job_id = %s
          AND scrape_status = 'success'
        LIMIT 1
        """,
        (job_id,),
    )

    result = cursor.fetchone()

    cursor.close()
    connection.close()

    return result is not None


def insert_job(job):
    connection = get_connection()
    cursor = connection.cursor()

    cursor.execute(
        """
        INSERT INTO jobs (
            job_id,
            title,
            company,
            location,
            work_type,
            classification,
            subclassification,
            short_description,
            description,
            posted,
            job_url,
            apply_url,
            apply_type,
            date_found,
            scrape_status,
            scrape_error
        )
        VALUES (
            %s, %s, %s, %s,
            %s, %s, %s, %s,
            %s, %s, %s, %s,
            %s, %s, %s, %s
        )

        ON CONFLICT(job_id) DO UPDATE SET
            title = excluded.title,
            company = excluded.company,
            location = excluded.location,
            work_type = excluded.work_type,
            classification = excluded.classification,
            subclassification = excluded.subclassification,
            short_description = excluded.short_description,
            description = excluded.description,
            posted = excluded.posted,
            job_url = excluded.job_url,
            apply_url = excluded.apply_url,
            apply_type = excluded.apply_type,
            date_found = excluded.date_found,
            scrape_status = excluded.scrape_status,
            scrape_error = excluded.scrape_error
        """,
        (
            job.get("job_id"),
            job.get("title"),
            job.get("company"),
            job.get("location"),
            job.get("work_type"),
            job.get("classification"),
            job.get("subclassification"),
            job.get("short_description"),
            job.get("description"),
            job.get("posted"),
            job.get("job_url"),
            job.get("apply_url"),
            job.get("apply_type"),
            job.get("date_found"),
            job.get("scrape_status"),
            job.get("scrape_error"),
        ),
    )

    connection.commit()
    cursor.close()
    connection.close()


def get_unanalysed_jobs(limit=25):
    with get_connection() as connection:
        with connection.cursor(row_factory=dict_row) as cursor:
            cursor.execute(
                """
                SELECT
                    j.job_id,
                    j.title,
                    j.company,
                    j.location,
                    j.work_type,
                    j.description
                FROM jobs j
                LEFT JOIN job_analysis a
                    ON j.job_id = a.job_id
                WHERE j.scrape_status = 'success'
                  AND j.description IS NOT NULL
                  AND a.job_id IS NULL
                ORDER BY j.date_found DESC
                LIMIT %s
                """,
                (limit,),
            )

            return cursor.fetchall()

def save_job_analysis(
    job_id,
    relevance_score,
    kept,
    analysis,
    model,
):
    with get_connection() as connection:
        with connection.cursor() as cursor:
            cursor.execute(
                """
                INSERT INTO job_analysis (
                    job_id,
                    relevance_score,
                    kept,
                    analysis,
                    model
                )
                VALUES (%s, %s, %s, %s, %s)

                ON CONFLICT(job_id) DO UPDATE SET
                    relevance_score = excluded.relevance_score,
                    kept = excluded.kept,
                    analysis = excluded.analysis,
                    model = excluded.model,
                    analysed_at = NOW()
                """,
                (
                    job_id,
                    relevance_score,
                    kept,
                    Jsonb(analysis),
                    model,
                ),
            )

        connection.commit()

def get_kept_jobs():
    with get_connection() as connection:
        with connection.cursor(row_factory=dict_row) as cursor:
            cursor.execute(
                """
                SELECT
                    j.job_id,
                    j.title,
                    j.company,
                    j.location,
                    j.work_type,
                    j.classification,
                    j.subclassification,
                    j.short_description,
                    j.description,
                    j.posted,
                    j.job_url,
                    j.apply_url,
                    j.date_found,

                    a.relevance_score,
                    a.analysis,
                    a.analysed_at

                FROM jobs j
                JOIN job_analysis a
                    ON j.job_id = a.job_id

                LEFT JOIN applications app
                    ON j.job_id = app.job_id

                WHERE a.kept = TRUE
                AND ( 
                    app.status is NULL
                    OR app.status != 'applied'
                )
                ORDER BY
                    a.relevance_score DESC,
                    j.date_found DESC
                """
            )

            return cursor.fetchall()

def get_job_for_document(job_id):
    with get_connection() as connection:
        with connection.cursor(row_factory=dict_row) as cursor:
            cursor.execute(
                """
                SELECT
                    j.job_id,
                    j.title,
                    j.company,
                    j.location,
                    j.work_type,
                    j.classification,
                    j.description,
                    j.job_url,
                    j.apply_url,
                    a.relevance_score,
                    a.analysis
                FROM jobs j
                JOIN job_analysis a
                    ON j.job_id = a.job_id
                WHERE j.job_id = %s
                  AND a.kept = TRUE
                LIMIT 1
                """,
                (job_id,),
            )

            return cursor.fetchone()


def get_job_documents(job_id):
    with get_connection() as connection:
        with connection.cursor(row_factory=dict_row) as cursor:
            cursor.execute(
                """
                SELECT *
                FROM job_documents
                WHERE job_id = %s
                LIMIT 1
                """,
                (job_id,),
            )

            return cursor.fetchone()


def save_resume_markdown(job_id, resume_markdown):
    with get_connection() as connection:
        with connection.cursor() as cursor:
            cursor.execute(
                """
                INSERT INTO job_documents (
                    job_id,
                    resume_markdown,
                    resume_generated_at,
                    updated_at
                )
                VALUES (
                    %s,
                    %s,
                    NOW(),
                    NOW()
                )

                ON CONFLICT(job_id) DO UPDATE SET
                    resume_markdown = excluded.resume_markdown,
                    resume_generated_at = NOW(),
                    updated_at = NOW()
                """,
                (
                    job_id,
                    resume_markdown,
                ),
            )

        connection.commit()

def save_cover_letter_markdown(
    job_id,
    cover_letter_markdown,
):
    with get_connection() as connection:
        with connection.cursor() as cursor:
            cursor.execute(
                """
                INSERT INTO job_documents (
                    job_id,
                    cover_letter_markdown,
                    cover_letter_generated_at,
                    updated_at
                )
                VALUES (
                    %s,
                    %s,
                    NOW(),
                    NOW()
                )

                ON CONFLICT(job_id) DO UPDATE SET
                    cover_letter_markdown = excluded.cover_letter_markdown,
                    cover_letter_generated_at = NOW(),
                    updated_at = NOW()
                """,
                (
                    job_id,
                    cover_letter_markdown,
                ),
            )

        connection.commit()


def save_document_drive_files(
    job_id,
    document_type,
    doc_id,
    doc_url,
    pdf_id,
    pdf_url,
):
    if document_type == "resume":

        query = """
            INSERT INTO job_documents (
                job_id,
                resume_doc_id,
                resume_doc_url,
                resume_pdf_id,
                resume_pdf_url,
                updated_at
            )
            VALUES (
                %s, %s, %s, %s, %s, NOW()
            )

            ON CONFLICT(job_id) DO UPDATE SET
                resume_doc_id = excluded.resume_doc_id,
                resume_doc_url = excluded.resume_doc_url,
                resume_pdf_id = excluded.resume_pdf_id,
                resume_pdf_url = excluded.resume_pdf_url,
                updated_at = NOW()
        """

    elif document_type == "cover":

        query = """
            INSERT INTO job_documents (
                job_id,
                cover_letter_doc_id,
                cover_letter_doc_url,
                cover_letter_pdf_id,
                cover_letter_pdf_url,
                updated_at
            )
            VALUES (
                %s, %s, %s, %s, %s, NOW()
            )

            ON CONFLICT(job_id) DO UPDATE SET
                cover_letter_doc_id = excluded.cover_letter_doc_id,
                cover_letter_doc_url = excluded.cover_letter_doc_url,
                cover_letter_pdf_id = excluded.cover_letter_pdf_id,
                cover_letter_pdf_url = excluded.cover_letter_pdf_url,
                updated_at = NOW()
        """

    else:
        raise ValueError(
            "document_type must be 'resume' or 'cover'"
        )

    with get_connection() as connection:
        with connection.cursor() as cursor:

            cursor.execute(
                query,
                (
                    job_id,
                    doc_id,
                    doc_url,
                    pdf_id,
                    pdf_url,
                ),
            )

        connection.commit()

def mark_job_applied(job_id):
    with get_connection() as connection:
        with connection.cursor() as cursor:
            cursor.execute(
                """
                INSERT INTO applications (
                    job_id,
                    status,
                    applied_at,
                    updated_at
                )
                VALUES (
                    %s,
                    'applied',
                    NOW(),
                    NOW()
                )

                ON CONFLICT(job_id) DO UPDATE SET
                    status = 'applied',
                    applied_at = COALESCE(
                        applications.applied_at,
                        NOW()
                    ),
                    updated_at = NOW()
                """,
                (job_id,),
            )

        connection.commit()


def get_application(job_id):
    with get_connection() as connection:
        with connection.cursor(
            row_factory=dict_row
        ) as cursor:

            cursor.execute(
                """
                SELECT
                    job_id,
                    status,
                    applied_at,
                    updated_at
                FROM applications
                WHERE job_id = %s
                """,
                (job_id,),
            )

            return cursor.fetchone()