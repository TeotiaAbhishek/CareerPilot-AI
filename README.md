# CareerPilot AI

CareerPilot AI is a personal job-search automation platform that discovers relevant data roles, evaluates job fit against a master resume, generates tailored application documents, and provides a Streamlit interface for reviewing and managing applications.

## Features

- Scrapes configured SEEK searches for new jobs
- Stores job data and analysis in Supabase Postgres
- Uses OpenAI to score professional relevance
- Separates professional fit from eligibility constraints
- Generates tailored resumes and cover letters on demand
- Converts generated Markdown into formatted DOCX documents
- Publishes Google Docs and PDF copies to Google Drive
- Provides a Streamlit dashboard for reviewing jobs and documents
- Tracks applied jobs and removes them from the active queue
- Supports a scheduled local daily pipeline with email success/failure reports

## Architecture

```text
CareerPilot-AI/
├── app/
│   └── app.py
├── config/
│   ├── candidate_profile.yaml
│   ├── searches.yaml
│   ├── google_credentials.json
│   └── google_token.json
├── data/
│   └── master_resume.md
├── src/
│   ├── __init__.py
│   ├── config.py
│   ├── database.py
│   ├── scraper/
│   │   ├── __init__.py
│   │   ├── seek_scraper.py
│   │   └── run_scraper.py
│   ├── intelligence/
│   │   ├── __init__.py
│   │   ├── relevance.py
│   │   ├── document_generator.py
│   │   └── document_formatter.py
│   └── integrations/
│       ├── __init__.py
│       ├── emailer.py
│       └── google_drive.py
├── .env.example
├── .gitignore
├── requirements.txt
├── run_daily.py
└── README.md
```

## Tech Stack

- Python 3.11
- Streamlit
- Supabase Postgres
- OpenAI API
- BeautifulSoup / Requests
- Pydantic
- Google Drive API
- python-docx
- PyYAML

## Setup

Clone the repository and create a virtual environment:

```bash
python3.11 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
```

Create your local environment file:

```bash
cp .env.example .env
```

Then populate `.env` with your own database, OpenAI, and email credentials.

## Configuration

Job-search settings are defined in:

```text
config/searches.yaml
```

Candidate eligibility and job-search constraints are defined in:

```text
config/candidate_profile.yaml
```

The resume used as the source of truth for relevance analysis and document generation is:

```text
data/master_resume.md
```

Google OAuth files are stored locally as:

```text
config/google_credentials.json
config/google_token.json
```

These files are excluded from Git.

## Run the Application

Start the Streamlit interface:

```bash
python -m streamlit run app/app.py
```

Run the job scraper:

```bash
python -m src.scraper.run_scraper
```

Run relevance analysis:

```bash
python -m src.intelligence.relevance
```

Generate a tailored resume:

```bash
python -m src.intelligence.document_generator resume <job_id>
```

Generate a tailored cover letter:

```bash
python -m src.intelligence.document_generator cover <job_id>
```

Publish an existing generated document to Google Drive:

```bash
python -m src.integrations.google_drive resume <job_id>
```

or:

```bash
python -m src.integrations.google_drive cover <job_id>
```

Run the full daily pipeline:

```bash
python run_daily.py
```

## Daily Pipeline

The automated workflow runs:

```text
SEEK discovery
    ↓
Supabase storage
    ↓
OpenAI relevance analysis
    ↓
Relevant jobs available in Streamlit
    ↓
Resume / cover letter generation on demand
    ↓
Google Docs + PDF publishing
    ↓
Application tracking
```

The daily worker can be scheduled locally, while the Streamlit interface can be deployed separately.

## Security

Sensitive files and credentials are excluded from version control, including:

- `.env`
- `.venv/`
- Google OAuth credentials and tokens
- Streamlit secrets
- logs and generated documents

Use `.env.example` as the template for local configuration.

## Status

CareerPilot AI V1 is functional end to end and includes job discovery, relevance scoring, document generation, Google Drive publishing, application tracking, and scheduled pipeline reporting.
