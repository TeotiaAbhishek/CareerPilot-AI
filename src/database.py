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


if __name__ == "__main__":
    create_tables()

    print(
        f"Database created at {DATABASE_PATH}"
    )