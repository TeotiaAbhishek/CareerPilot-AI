import time
from datetime import datetime

import requests

from database import (
    create_tables,
    insert_job,
    job_exists,
)
from job_detail_parser import extract_job_details
from seek_scraper import (
    HEADERS,
    scrape_search_pages,
)


DETAIL_DELAY_SECONDS = 2


def fetch_detail_page(session, url):
    response = session.get(
        url,
        timeout=30,
    )

    response.raise_for_status()

    return response.text


def main():
    create_tables()

    session = requests.Session()
    session.headers.update(
        HEADERS
    )

    print(
        "Searching SEEK...\n"
    )

    search_jobs = scrape_search_pages(
        keyword="Data Analyst",
        location="Sydney NSW",
        max_pages=3,
        session=session,
    )

    print(
        f"\nFound "
        f"{len(search_jobs)} "
        f"unique jobs."
    )


    new_jobs = 0
    skipped_jobs = 0
    failed_jobs = 0

    for index, job in enumerate(
        search_jobs,
        start=1,
    ):
        job_id = job["job_id"]

        print(
            f"\n[{index}/{len(search_jobs)}] "
            f"{job['title']} | "
            f"{job['company']}"
        )

        # -----------------------------
        # STEP 1: Check database
        # -----------------------------
        if job_exists(job_id):
            print("    → Already in database. Skipping.")

            skipped_jobs += 1
            continue

        print("    → NEW JOB")

        try:
            # -----------------------------
            # STEP 2: Fetch detail page
            # -----------------------------
            html = fetch_detail_page(
                session=session,
                url=job["job_url"],
            )

            # -----------------------------
            # STEP 3: Parse detail page
            # -----------------------------
            details = extract_job_details(
                html=html,
                job_id=job_id,
                job_url=job["job_url"],
            )

            # -----------------------------
            # STEP 4: Merge search + detail data
            # -----------------------------
            complete_job = {
                **job,
                **details,
            }

            complete_job["date_found"] = (
                datetime.now()
                .astimezone()
                .isoformat(timespec="seconds")
            )

            complete_job["scrape_status"] = "success"
            complete_job["scrape_error"] = None

            # -----------------------------
            # STEP 5: Store in SQLite
            # -----------------------------
            insert_job(
                complete_job
            )

            print(
                f"    ✓ Saved | "
                f"{complete_job['posted']} | "
                f"{complete_job['work_type']}"
            )

            new_jobs += 1

        except requests.RequestException as error:
            print(
                f"    ✗ Request failed: {error}"
            )

            failed_job = {
                **job,
                "date_found": (
                    datetime.now()
                    .astimezone()
                    .isoformat(timespec="seconds")
                ),
                "scrape_status": "failed",
                "scrape_error": str(error),
            }

            insert_job(
                failed_job
            )

            failed_jobs += 1

        time.sleep(
            DETAIL_DELAY_SECONDS
        )

    print("\n" + "=" * 60)
    print("SCRAPE COMPLETE")
    print("=" * 60)

    print(
        f"Jobs found:     {len(search_jobs)}"
    )

    print(
        f"New jobs:       {new_jobs}"
    )

    print(
        f"Already known:  {skipped_jobs}"
    )

    print(
        f"Failed:         {failed_jobs}"
    )


if __name__ == "__main__":
    main()