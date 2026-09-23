import json
import time
from pathlib import Path

import requests

from job_detail_parser import extract_job_details


INPUT_FILE = Path("data/processed/jobs.json")
OUTPUT_FILE = Path("data/processed/jobs_detailed.json")

HEADERS = {
    "User-Agent": (
        "Mozilla/5.0 "
        "(Windows NT 10.0; Win64; x64) "
        "AppleWebKit/537.36 "
        "(KHTML, like Gecko) "
        "Chrome/140.0 Safari/537.36"
    ),
    "Accept-Language": "en-AU,en;q=0.9",
}


def load_jobs():
    with INPUT_FILE.open(
        "r",
        encoding="utf-8",
    ) as file:
        return json.load(file)


def save_jobs(jobs):
    with OUTPUT_FILE.open(
        "w",
        encoding="utf-8",
    ) as file:
        json.dump(
            jobs,
            file,
            indent=4,
            ensure_ascii=False,
        )


def fetch_job_page(session, url):
    response = session.get(
        url,
        timeout=30,
    )

    response.raise_for_status()

    return response.text


def main():
    jobs = load_jobs()

    print(f"Loaded {len(jobs)} jobs.")
    print()

    detailed_jobs = []

    session = requests.Session()
    session.headers.update(HEADERS)

    for index, job in enumerate(
        jobs,
        start=1,
    ):
        print(
            f"[{index}/{len(jobs)}] "
            f"{job['title']} | "
            f"{job['company']}"
        )

        try:
            html = fetch_job_page(
                session=session,
                url=job["url"],
            )

            details = extract_job_details(
                html=html,
                job_id=job["job_id"],
                job_url=job["url"],
            )

            merged_job = {
                **job,
                **details,
            }

            detailed_jobs.append(
                merged_job
            )

            print(
                f"    ✓ {details['posted']} | "
                f"{details['work_type']}"
            )

        except requests.RequestException as error:
            print(
                f"    ✗ Request failed: {error}"
            )

            job["scrape_error"] = str(error)

            detailed_jobs.append(job)

        save_jobs(detailed_jobs)

        time.sleep(2)

    print()
    print(
        f"Saved {len(detailed_jobs)} jobs "
        f"to {OUTPUT_FILE}"
    )


if __name__ == "__main__":
    main()