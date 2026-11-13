//! Talking to SEEK: building URLs, fetching pages politely, and parsing HTML.
//! Selectors are the same `data-automation` attributes v1 used.

use std::time::Duration;

use reqwest::{StatusCode, header};
use scraper::{ElementRef, Html, Selector};

/// A job as it appears on a search results page (the "card").
#[derive(Debug, Clone, PartialEq)]
pub struct JobCard {
    pub id: String,
    pub title: Option<String>,
    pub company: Option<String>,
    pub location: Option<String>,
    pub short_description: Option<String>,
    pub classification: Option<String>,
    pub subclassification: Option<String>,
    pub job_url: String,
}

/// Everything on a job's own page.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct JobDetail {
    pub title: Option<String>,
    pub company: Option<String>,
    pub location: Option<String>,
    pub classification: Option<String>,
    pub work_type: Option<String>,
    pub salary: Option<String>,
    pub description_html: Option<String>,
    pub description_text: Option<String>,
    pub posted_label: Option<String>,
    pub apply_type: Option<String>,
    pub apply_url: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("SEEK is rate limiting us (HTTP 429)")]
    RateLimited,
    #[error("page not found (HTTP 404)")]
    NotFound,
    #[error("request failed: {0}")]
    Other(String),
}

/// How long to wait between requests. Tests set these to zero.
#[derive(Debug, Clone)]
pub struct Politeness {
    /// Random pause between two requests, in milliseconds (min, max).
    pub delay_ms: (u64, u64),
    /// How long to back off after a 429 before the single retry.
    pub rate_limit_wait: Duration,
    /// Attempts for network errors and 5xx responses.
    pub max_attempts: u32,
}

impl Default for Politeness {
    fn default() -> Self {
        Self {
            delay_ms: (3_000, 5_000),
            rate_limit_wait: Duration::from_secs(60),
            max_attempts: 3,
        }
    }
}

impl Politeness {
    pub fn none() -> Self {
        Self {
            delay_ms: (0, 0),
            rate_limit_wait: Duration::ZERO,
            max_attempts: 1,
        }
    }

    /// Sleep for a random time in the configured range.
    pub async fn pause(&self) {
        let (min, max) = self.delay_ms;
        if max == 0 {
            return;
        }
        let ms = if max > min {
            rand::random_range(min..max)
        } else {
            min
        };
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }
}

#[derive(Clone)]
pub struct SeekClient {
    http: reqwest::Client,
    base_url: String,
    pub politeness: Politeness,
}

impl SeekClient {
    pub fn new(base_url: impl Into<String>, politeness: Politeness) -> anyhow::Result<Self> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::ACCEPT_LANGUAGE,
            header::HeaderValue::from_static("en-AU,en;q=0.9"),
        );

        let http = reqwest::Client::builder()
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                 (KHTML, like Gecko) Chrome/140.0 Safari/537.36",
            )
            .default_headers(headers)
            .cookie_store(true)
            .gzip(true)
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self {
            http,
            base_url: base_url.into(),
            politeness,
        })
    }

    /// e.g. https://www.seek.com.au/data-analyst-jobs/in-Sydney-NSW?sortmode=ListedDate&page=2
    pub fn search_url(&self, keywords: &str, location: &str, page: u32) -> String {
        let mut url = format!(
            "{}/{}-jobs/in-{}?sortmode=ListedDate",
            self.base_url,
            slug(keywords).to_lowercase(),
            slug(location)
        );
        if page > 1 {
            url.push_str(&format!("&page={page}"));
        }
        url
    }

    pub fn job_url(&self, id: &str) -> String {
        format!("{}/job/{id}", self.base_url)
    }

    /// GET a page, retrying network errors and 5xx, and backing off once on 429.
    pub async fn fetch(&self, url: &str) -> Result<String, FetchError> {
        let mut attempt = 0;
        let mut rate_limited_once = false;

        loop {
            attempt += 1;
            let result = self.http.get(url).send().await;

            let retryable_error = match result {
                Ok(response) => match response.status() {
                    status if status.is_success() => {
                        return response
                            .text()
                            .await
                            .map_err(|e| FetchError::Other(e.to_string()));
                    }
                    StatusCode::TOO_MANY_REQUESTS => {
                        if rate_limited_once {
                            return Err(FetchError::RateLimited);
                        }
                        rate_limited_once = true;
                        tracing::warn!(url, "SEEK returned 429, backing off");
                        tokio::time::sleep(self.politeness.rate_limit_wait).await;
                        continue;
                    }
                    StatusCode::NOT_FOUND => return Err(FetchError::NotFound),
                    status if status.is_server_error() => format!("HTTP {status}"),
                    status => return Err(FetchError::Other(format!("HTTP {status}"))),
                },
                Err(error) => error.to_string(),
            };

            if attempt >= self.politeness.max_attempts {
                return Err(FetchError::Other(retryable_error));
            }
            let wait = Duration::from_secs(5 * attempt as u64);
            tracing::warn!(url, attempt, error = %retryable_error, "retrying in {wait:?}");
            if !self.politeness.rate_limit_wait.is_zero() {
                tokio::time::sleep(wait).await;
            }
        }
    }

    pub fn parse_search_page(&self, html: &str) -> Vec<JobCard> {
        let document = Html::parse_document(html);
        let card_sel = sel(r#"article[data-testid="job-card"]"#);
        let mut cards: Vec<JobCard> = Vec::new();

        for card in document.select(&card_sel) {
            let Some(id) = card.value().attr("data-job-id") else {
                continue;
            };
            if cards.iter().any(|c| c.id == id) {
                continue;
            }
            cards.push(JobCard {
                id: id.to_string(),
                title: automation_text(card, "jobTitle"),
                company: automation_text(card, "jobCompany"),
                location: automation_text(card, "jobLocation"),
                short_description: automation_text(card, "jobShortDescription"),
                classification: automation_text(card, "jobClassification"),
                subclassification: automation_text(card, "jobSubClassification"),
                job_url: self.job_url(id),
            });
        }
        cards
    }

    pub fn parse_job_detail(&self, html: &str) -> JobDetail {
        let document = Html::parse_document(html);
        let root = document.root_element();

        let description_html = automation_element(root, "jobAdDetails")
            .map(|el| ammonia::clean(&el.inner_html()))
            .filter(|h| !h.trim().is_empty());

        let description_text = description_html
            .as_deref()
            .and_then(|h| html2text::from_read(h.as_bytes(), 10_000).ok())
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());

        let apply = automation_element(root, "job-detail-apply");

        JobDetail {
            title: automation_text(root, "job-detail-title"),
            company: automation_text(root, "advertiser-name"),
            location: automation_text(root, "job-detail-location"),
            classification: automation_text(root, "job-detail-classifications"),
            work_type: automation_text(root, "job-detail-work-type"),
            salary: automation_text(root, "job-detail-salary"),
            description_html,
            description_text,
            posted_label: posted_label(root),
            apply_type: apply.map(|el| clean_text(el)).filter(|t| !t.is_empty()),
            apply_url: apply
                .and_then(|el| el.value().attr("href"))
                .map(|href| self.absolute(href)),
        }
    }

    fn absolute(&self, href: &str) -> String {
        if href.starts_with("http://") || href.starts_with("https://") {
            href.to_string()
        } else {
            format!("{}/{}", self.base_url, href.trim_start_matches('/'))
        }
    }
}

/// "Sydney NSW" -> "Sydney-NSW", "C++ Developer" -> "C-Developer".
pub fn slug(text: &str) -> String {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("hard-coded selector is valid")
}

fn automation_element<'a>(scope: ElementRef<'a>, name: &str) -> Option<ElementRef<'a>> {
    scope
        .select(&sel(&format!(r#"[data-automation="{name}"]"#)))
        .next()
}

fn automation_text(scope: ElementRef<'_>, name: &str) -> Option<String> {
    automation_element(scope, name)
        .map(clean_text)
        .filter(|t| !t.is_empty())
}

/// All text inside an element, whitespace collapsed (like BeautifulSoup's get_text(" ", strip=True)).
fn clean_text(el: ElementRef<'_>) -> String {
    el.text()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The "Posted 3d ago" text somewhere on the job page.
fn posted_label(root: ElementRef<'_>) -> Option<String> {
    let page = automation_element(root, "jobDetailsPage").unwrap_or(root);
    page.text().map(str::trim).find_map(|t| {
        t.strip_prefix("Posted ")
            .map(|rest| rest.trim().to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> SeekClient {
        SeekClient::new("https://www.seek.com.au", Politeness::none()).unwrap()
    }

    #[test]
    fn builds_search_urls_like_v1() {
        let c = client();
        assert_eq!(
            c.search_url("Data Analyst", "Sydney NSW", 1),
            "https://www.seek.com.au/data-analyst-jobs/in-Sydney-NSW?sortmode=ListedDate"
        );
        assert_eq!(
            c.search_url("Data Analyst", "Sydney NSW", 2),
            "https://www.seek.com.au/data-analyst-jobs/in-Sydney-NSW?sortmode=ListedDate&page=2"
        );
    }

    #[test]
    fn parses_search_cards_and_skips_duplicates() {
        let html = include_str!("../../tests/fixtures/seek_search.html");
        let cards = client().parse_search_page(html);

        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].id, "80000001");
        assert_eq!(cards[0].title.as_deref(), Some("Data Analyst"));
        assert_eq!(cards[0].company.as_deref(), Some("Acme Analytics"));
        assert_eq!(cards[0].job_url, "https://www.seek.com.au/job/80000001");
    }

    #[test]
    fn parses_job_detail() {
        let html = include_str!("../../tests/fixtures/seek_job.html");
        let job = client().parse_job_detail(html);

        assert_eq!(job.title.as_deref(), Some("Data Analyst"));
        assert_eq!(job.work_type.as_deref(), Some("Full time"));
        assert_eq!(job.salary.as_deref(), Some("$95,000 – $110,000 per year"));
        assert_eq!(job.posted_label.as_deref(), Some("3d ago"));
        assert_eq!(job.apply_type.as_deref(), Some("Quick apply"));
        assert_eq!(
            job.apply_url.as_deref(),
            Some("https://www.seek.com.au/job/80000001/apply")
        );

        let html = job.description_html.unwrap();
        assert!(html.contains("<li>SQL</li>"));
        assert!(!html.contains("<script"), "scripts must be stripped");

        let text = job.description_text.unwrap();
        assert!(text.contains("SQL"));
        assert!(!text.contains("alert"));
    }
}
