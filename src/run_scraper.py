import time
from datetime import datetime

import requests

from database import (
    create_tables,
    insert_job,
    job_exists,
)

from seek_scraper import (
    HEADERS,
    RateLimitError,
    fetch_page,
    scrape_search_pages,
    extract_job_details,
)
from config import load_search_config

DETAIL_DELAY_SECONDS = 2


def main():
    create_tables()

    session = requests.Session()
    session.headers.update(HEADERS)

    # Load search settings
    config = load_search_config()

    location = config["location"]
    max_pages = config["max_pages"]
    keywords = config["keywords"]

    all_search_jobs = {}

    for keyword in keywords:

        print("\n" + "=" * 60)
        print(f"SEARCHING: {keyword}")
        print("=" * 60)

        try:
            jobs = scrape_search_pages(
                keyword=keyword,
                location=location,
                max_pages=max_pages,
                session=session,
            )

        except RateLimitError as error:
            print(
                f"\n⚠ Search stopped for '{keyword}': {error}"
            )

            print(
                "SEEK is rate limiting requests. "
                "Stopping this scraper run."
            )

            return

        except requests.RequestException as error:
            print(
                f"\n✗ Search request failed for "
                f"'{keyword}': {error}"
            )

            print(
                "Stopping this scraper run."
            )

            return

        print(
            f"{keyword}: "
            f"{len(jobs)} unique jobs found"
        )

        for job in jobs:
            job_id = job["job_id"]

            if job_id not in all_search_jobs:
                all_search_jobs[job_id] = job

    search_jobs = list(
        all_search_jobs.values()
    )

    print("\n" + "=" * 60)
    print("SEARCH COLLECTION COMPLETE")
    print("=" * 60)

    print(
        f"Total unique jobs across all searches: "
        f"{len(search_jobs)}"
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

        if job_exists(job_id):
            print(
                "    → Already in database. Skipping."
            )

            skipped_jobs += 1
            continue

        print("    → NEW JOB")

        try:
            html = fetch_page(
                url=job["job_url"],
                session=session,
            )

            details = extract_job_details(
                html=html,
                job_id=job_id,
                job_url=job["job_url"],
            )

            if not details.get("description"):
                raise ValueError(
                "Job description could not be parsed."
            )
            
            complete_job = job.copy()

            for key, value in details.items():
                if value is not None:
                    complete_job[key] = value

            complete_job["date_found"] = (
                datetime.now()
                .astimezone()
            )

            complete_job["scrape_status"] = (
                "success"
            )

            complete_job["scrape_error"] = None

            insert_job(
                complete_job
            )

            print(
                f"    ✓ Saved | "
                f"{complete_job['posted']} | "
                f"{complete_job['work_type']}"
            )

            new_jobs += 1
        except RateLimitError as error:
            print(
                f"\n⚠ Rate limit reached: {error}"
            )

            print(
                "Stopping detail collection."
            )

            break
        except (
            requests.RequestException,
            ValueError,
        ) as error:

            print(
                f"    ✗ Request failed: {error}"
            )

            failed_job = {
                **job,
                "date_found": (
                    datetime.now()
                    .astimezone()
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