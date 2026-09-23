import sqlite3
from pathlib import Path


DATABASE_PATH = Path(
    "data/jobhelp.db"
)


def get_connection():
    return sqlite3.connect(
        DATABASE_PATH
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

            date_found TEXT,

            scrape_status TEXT,

            scrape_error TEXT
        )
        """
    )

    connection.commit()

    connection.close()

def job_exists(job_id):

    connection = get_connection()

    cursor = connection.cursor()

    cursor.execute(
        """
        SELECT 1
        FROM jobs
        WHERE job_id = ?
        LIMIT 1
        """,
        (job_id,),
    )

    result = cursor.fetchone()

    connection.close()

    return result is not None

def insert_job(job):

    connection = get_connection()

    cursor = connection.cursor()

    cursor.execute(
        """
        INSERT OR IGNORE INTO jobs (

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
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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

    connection.close()

if __name__ == "__main__":
    create_tables()

    print(
        f"Database created at {DATABASE_PATH}"
    )