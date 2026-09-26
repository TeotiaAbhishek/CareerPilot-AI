import time
import re
from urllib.parse import urljoin

import requests
from bs4 import BeautifulSoup


BASE_URL = "https://www.seek.com.au"

SEARCH_PAGE_DELAY_SECONDS = 3


class RateLimitError(Exception):
    pass

def build_search_url(
    keyword,
    location,
    page=1,
):
    keyword_slug = keyword.lower().replace(" ", "-")
    location_slug = location.replace(" ", "-")

    url = (
        f"{BASE_URL}/"
        f"{keyword_slug}-jobs/"
        f"in-{location_slug}"
        f"?sortmode=ListedDate"
    )

    if page > 1:
        url += f"&page={page}"

    return url


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

def fetch_page(
    url,
    session=None,
    max_retries=3,
):
    client = session or requests

    for attempt in range(
        1,
        max_retries + 1,
    ):
        try:
            response = client.get(
                url,
                headers=HEADERS,
                timeout=30,
            )

            print(
                f"Status code: "
                f"{response.status_code}"
            )

            if response.status_code == 429:
                raise RateLimitError(
                    "SEEK rate limit active "
                    "(HTTP 429)."
                )

            response.raise_for_status()

            return response.text

        except RateLimitError:
            raise

        except requests.RequestException as error:
            if attempt == max_retries:
                raise

            wait_seconds = (
                5 * attempt
            )

            print(
                f"Request failed "
                f"(attempt "
                f"{attempt}/"
                f"{max_retries}): "
                f"{error}"
            )

            print(
                f"Retrying in "
                f"{wait_seconds}s..."
            )

            time.sleep(
                wait_seconds
            )

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
            "job_url": (
                f"{BASE_URL}/job/{job_id}"
            ),
        }

    return list(jobs.values())

def scrape_search_pages(
    keyword,
    location,
    max_pages=3,
    session=None,
):
    all_jobs = {}

    for page in range(
        1,
        max_pages + 1,
    ):
        print(
            f"\nFetching page {page}..."
        )

        url = build_search_url(
            keyword=keyword,
            location=location,
            page=page,
        )

        html = fetch_page(
            url=url,
            session=session,
        )

        jobs = extract_jobs(
            html
        )

        print(
            f"Page {page}: "
            f"{len(jobs)} jobs found."
        )

        new_on_page = 0

        for job in jobs:
            job_id = job["job_id"]

            if job_id not in all_jobs:
                all_jobs[job_id] = job
                new_on_page += 1

        print(
            f"Page {page}: "
            f"{new_on_page} new unique jobs."
        )

        if new_on_page == 0:
            print(
                "No new jobs on this page. "
                "Stopping pagination."
            )
            break

        # Wait before requesting the next search page
        if page < max_pages:
            print(
                f"Waiting "
                f"{SEARCH_PAGE_DELAY_SECONDS}s "
                f"before next page..."
            )

            time.sleep(
                SEARCH_PAGE_DELAY_SECONDS
            )

    return list(
        all_jobs.values()
    )
def get_text_by_automation(soup, automation_name):
    element = soup.find(
        attrs={"data-automation": automation_name}
    )

    if element is None:
        return None

    return element.get_text(
        " ",
        strip=True,
    )


def extract_apply_url(soup):
    apply_element = soup.find(
        attrs={"data-automation": "job-detail-apply"}
    )

    if apply_element is None:
        return None

    href = apply_element.get("href")

    if not href:
        return None

    return urljoin(
        BASE_URL,
        href,
    )


def extract_apply_type(soup):
    apply_element = soup.find(
        attrs={"data-automation": "job-detail-apply"}
    )

    if apply_element is None:
        return None

    return apply_element.get_text(
        " ",
        strip=True,
    )


def extract_posted_date(soup):
    job_page = soup.find(
        attrs={"data-automation": "jobDetailsPage"}
    )

    if job_page is None:
        return None

    posted_text = job_page.find(
        string=re.compile(r"^\s*Posted\s+")
    )

    if posted_text is None:
        return None

    text = posted_text.strip()

    return re.sub(
        r"^Posted\s+",
        "",
        text,
    )


def extract_job_details(
    html,
    job_id=None,
    job_url=None,
):
    soup = BeautifulSoup(
        html,
        "html.parser",
    )

    job = {
        "job_id": job_id,

        "title": get_text_by_automation(
            soup,
            "job-detail-title",
        ),

        "company": get_text_by_automation(
            soup,
            "advertiser-name",
        ),

        "location": get_text_by_automation(
            soup,
            "job-detail-location",
        ),

        "classification": get_text_by_automation(
            soup,
            "job-detail-classifications",
        ),

        "work_type": get_text_by_automation(
            soup,
            "job-detail-work-type",
        ),

        "description": get_text_by_automation(
            soup,
            "jobAdDetails",
        ),

        "posted": extract_posted_date(
            soup
        ),

        "apply_type": extract_apply_type(
            soup
        ),

        "apply_url": extract_apply_url(
            soup
        ),

        "job_url": job_url,
    }

    return job