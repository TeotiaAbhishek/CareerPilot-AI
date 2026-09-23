import re
from urllib.parse import urljoin

from bs4 import BeautifulSoup


BASE_URL = "https://www.seek.com.au"


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