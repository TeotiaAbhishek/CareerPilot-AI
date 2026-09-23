from pathlib import Path

import requests
from bs4 import BeautifulSoup

from job_detail_parser import extract_job_details


JOB_URL = "https://www.seek.com.au/job/94479224"


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


def fetch_job_page(url):
    response = requests.get(
        url,
        headers=HEADERS,
        timeout=30,
    )

    print("Status:", response.status_code)
    print("Response size:", len(response.text))

    response.raise_for_status()

    return response.text


def save_html(html):
    path = Path(
        "data/raw/job_detail.html"
    )

    path.write_text(
        html,
        encoding="utf-8",
    )

    print(
        f"Saved HTML to {path}"
    )


def inspect_automation_fields(html):
    soup = BeautifulSoup(
        html,
        "html.parser",
    )

    elements = soup.find_all(
        attrs={"data-automation": True}
    )

    print("\nDATA-AUTOMATION FIELDS")
    print("=" * 80)

    for element in elements:
        automation = element.get(
            "data-automation"
        )

        text = element.get_text(
            " ",
            strip=True,
        )

        if len(text) > 300:
            text = text[:300] + "..."

        print(
            f"{automation:45} -> {repr(text)}"
        )

def inspect_apply_element(html):
    soup = BeautifulSoup(html, "html.parser")

    element = soup.find(
        attrs={"data-automation": "job-detail-apply"}
    )

    print("\nAPPLY ELEMENT")
    print("=" * 80)

    if element:
        print(element.prettify()[:3000])
        print("\nATTRIBUTES:")
        print(element.attrs)
    else:
        print("Apply element not found.")


def inspect_posted_elements(html):
    soup = BeautifulSoup(html, "html.parser")

    elements = soup.find_all(
        string=lambda text: text and "Posted" in text
    )

    print("\nPOSTED DATE CANDIDATES")
    print("=" * 80)

    for text in elements:
        print(text.parent.prettify()[:2000])
        print("-" * 80)

def main():

    job_id = "94479224"

    job_url = f"https://www.seek.com.au/job/{job_id}"

    html = fetch_job_page(job_url)

    save_html(html)

    job = extract_job_details(
        html=html,
        job_id=job_id,
        job_url=job_url,
    )

    print("\nJOB DETAILS")
    print("=" * 80)

    for key, value in job.items():

        if key == "description":
            print(
                f"{key}: "
                f"{value[:500] if value else None}..."
            )
        else:
            print(f"{key}: {value}")

    # Debug the remaining fields
    inspect_apply_element(html)
    inspect_posted_elements(html)


if __name__ == "__main__":
    main()