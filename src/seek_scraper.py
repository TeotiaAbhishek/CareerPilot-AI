import json
import re
from pathlib import Path
from urllib.parse import urljoin

import requests
from bs4 import BeautifulSoup


BASE_URL = "https://www.seek.com.au"

SEARCH_URL = (
    "https://www.seek.com.au/"
    "data-analyst-jobs/in-Sydney-NSW"
)


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

def fetch_page(url):
    response = requests.get(
        url,
        headers=HEADERS,
        timeout=30,
    )

    print(f"Status code: {response.status_code}")
    print(f"Response size: {len(response.text):,} characters")

    response.raise_for_status()

    return response.text


def save_raw_html(html):
    path = Path("data/raw/seek_search.html")

    path.write_text(
        html,
        encoding="utf-8",
    )

    print(f"Raw HTML saved to {path}")

def inspect_card_fields(card):

    print("\n" + "=" * 80)
    print("FIELDS INSIDE JOB CARD")
    print("=" * 80)

    elements = card.find_all(
        attrs={"data-automation": True}
    )

    for element in elements:

        automation = element.get(
            "data-automation"
        )

        text = element.get_text(
            " ",
            strip=True,
        )

        print(
            f"{automation:40} → {repr(text)}"
        )

def inspect_ancestor_chain(link):
    current = link

    for level in range(8):

        if current is None:
            break

        print("\n" + "=" * 80)
        print(f"LEVEL {level}")
        print("=" * 80)

        print("TAG:")
        print(current.name)

        print("\nATTRIBUTES:")
        print(current.attrs)

        print("\nTEXT:")
        print(
            repr(
                current.get_text(
                    " ",
                    strip=True
                )[:500]
            )
        )

        current = current.parent

def extract_jobs(html):
    soup = BeautifulSoup(
        html,
        "html.parser",
    )

    cards = soup.find_all(
        "article",
        attrs={"data-testid": "job-card"},
    )

    print(f"Found {len(cards)} job cards.")

    jobs = {}

    for card in cards:

        job_id = card.get("data-job-id")

        if not job_id:
            continue

        # Already processed this job
        if job_id in jobs:
            continue

        def get_text(automation_name):
            element = card.find(
                attrs={
                    "data-automation": automation_name
                }
            )

            if element:
                return element.get_text(
                    " ",
                    strip=True,
                )

            return None

        jobs[job_id] = {
            "job_id": job_id,
            "title": get_text("jobTitle"),
            "company": get_text("jobCompany"),
            "location": get_text("jobLocation"),
            "short_description": get_text(
                "jobShortDescription"
            ),
            "subclassification": get_text(
                "jobSubClassification"
            ),
            "classification": get_text(
                "jobClassification"
            ),
            "listing_date": get_text(
                "jobListingDate"
            ),
            "job_url": (
                f"{BASE_URL}/job/{job_id}"
            ),
        }

    return list(jobs.values())

# def remove_duplicates(jobs):
#     unique_jobs = {}

#     for job in jobs:
#         job_id = job["job_id"]

#         if job_id not in unique_jobs:
#             unique_jobs[job_id] = job

#     return list(unique_jobs.values())


def save_jobs(jobs):
    path = Path(
        "data/processed/jobs.json"
    )

    with path.open(
        "w",
        encoding="utf-8",
    ) as file:

        json.dump(
            jobs,
            file,
            indent=4,
            ensure_ascii=False,
        )

    print(
        f"Saved {len(jobs)} jobs to {path}"
    )


def main():

    print("Fetching SEEK...")
    print()

    html = fetch_page(
        SEARCH_URL
    )

    save_raw_html(html)

    jobs = extract_jobs(html)

    # jobs = remove_duplicates(jobs)

    print()
    print(f"Found {len(jobs)} jobs.")
    print()

    for job in jobs[:10]:
        print(
            job["job_id"],
            "|",
            job["title"],
            "|",
            job["company"],
            "|",
            job["location"],
        )

    save_jobs(jobs)


if __name__ == "__main__":
    main()