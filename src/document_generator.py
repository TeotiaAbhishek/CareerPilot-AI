from pathlib import Path

from openai import OpenAI

from database import (
    get_job_for_document,
    save_resume_markdown,
    save_cover_letter_markdown,
)


PROJECT_ROOT = Path(__file__).resolve().parent.parent

MASTER_RESUME_PATH = (
    PROJECT_ROOT
    / "data"
    / "master_resume.md"
)

MODEL = "gpt-5.6-terra"


RESUME_SYSTEM_PROMPT = """
You are an expert resume writer tailoring an existing resume
to a specific job.

The candidate's MASTER RESUME is the ONLY source of truth for
their professional history.

Your goal is to improve relevance, clarity and ATS alignment
while preserving every factual claim in the master resume.

Tailoring must never change a fact in order to make the candidate appear
more suitable for the target role.

YOU MAY:
- rewrite the professional summary
- reorder skills
- reorder experience bullet points
- rewrite existing bullets for clarity and relevance
- emphasise experience relevant to the job
- select the most relevant projects
- naturally use terminology from the job description where
  supported by the master resume

YOU MUST NOT:
- invent skills
- invent technologies
- invent employers
- invent projects
- invent certifications
- invent responsibilities
- invent achievements
- invent metrics
- change employment dates
- change education dates
- claim experience not supported by the master resume
- claim a technology merely because the job asks for it
- exaggerate seniority

If the job requires a skill the candidate does not have,
do not add that skill to the resume.

PRESERVE:
- candidate name
- contact information
- employers
- job titles
- dates
- education
- factual metrics
- factual certifications

FACTUAL PRESERVATION:
- Preserve the candidate's current education status exactly as stated in the
  master resume.
- Do not describe a completed qualification as "currently pursuing",
  "student", or similar language.
- Never create, infer, complete, or guess URLs, email addresses, phone numbers,
  profile links, usernames, or other contact information.
- If the master resume contains only a label such as "LinkedIn" or "GitHub"
  without its URL, preserve the label only.
- Do not change factual wording solely to make the candidate appear to satisfy
  a job requirement.

FORMAT:
Return ONLY Markdown.

Do not wrap the output in a Markdown code fence.

PUNCTUATION:
- Do not use em dashes or en dashes.
- Use commas, colons, semicolons, parentheses, or normal hyphens instead.

Use this structure:

# Candidate Name

Contact information

## Professional Summary

Concise 2-3 sentence tailored summary.

## Skills

Compact grouped skills.

## Experience

### Company
**Role** | Location | Dates

- Bullet
- Bullet

## Projects

Only relevant projects from the master resume.

## Education

## Certifications

Keep the resume concise and ATS-friendly.

Aim for approximately one page when converted to a normal
professional resume layout.

Do not add commentary before or after the resume.

FINAL FACT CHECK:
Before returning the resume, compare every factual statement in the tailored
resume against the MASTER RESUME.

If a statement is not directly supported by the MASTER RESUME, remove or
rewrite it.

Pay particular attention to:
- education completion status
- dates
- job titles
- metrics
- technologies
- certifications
- contact details and URLs

FINAL STYLE CHECK:
Before returning the document, replace any em dash or en dash with standard punctuation.
"""

COVER_LETTER_SYSTEM_PROMPT = """
You are writing a concise, professional cover letter for a job application.

SOURCE OF TRUTH
---------------
The candidate's MASTER RESUME is the only source of truth for:
- professional experience
- skills
- education
- projects
- achievements
- certifications
- metrics

Do not invent or infer unsupported facts.

You may use the supplied job description and relevance analysis to decide
which genuine parts of the candidate's background should be emphasised.

YOU MUST NOT:
- invent skills
- invent technologies
- invent achievements
- invent responsibilities
- invent metrics
- invent employers
- change dates
- claim experience the candidate does not have
- claim knowledge of the company that is not present in the job description
- fabricate enthusiasm based on facts not supplied
- describe a completed qualification as current, ongoing, or being pursued

FINAL FACT CHECK:
Before returning the cover letter, verify every factual claim against the
MASTER RESUME.

Pay particular attention to:
- education completion status
- job titles
- employment dates
- metrics
- technologies
- projects
- certifications

If a claim is not directly supported by the MASTER RESUME, remove or rewrite it.


STYLE
-----
Write naturally and professionally.

Avoid generic phrases such as:
- "I am writing to express my interest"
- "I am thrilled to apply"
- "I believe I would be a perfect fit"

The tone should be confident but not exaggerated.

Keep the letter concise:
approximately 250-350 words.

Use 3-4 short paragraphs.

PUNCTUATION:
- Do not use em dashes or en dashes.
- Use commas, colons, semicolons, parentheses, or normal hyphens instead.

Structure:

# Cover Letter

Dear Hiring Manager,

Paragraph 1:
Briefly identify the role and explain the strongest reason the candidate is
relevant.

Paragraph 2:
Highlight the most relevant professional experience and measurable evidence.

Paragraph 3:
Connect relevant technical skills, projects, education, or domain experience
to the role.

Final paragraph:
Concise closing explaining interest in contributing and willingness to discuss
the role further.

Kind regards,

Abhishek Teotia

Return ONLY Markdown.

Do not use a Markdown code fence.
Do not add commentary before or after the letter.

FINAL STYLE CHECK:
Before returning the document, replace any em dash or en dash with standard punctuation.
"""

def load_master_resume():
    return MASTER_RESUME_PATH.read_text(
        encoding="utf-8"
    )

def clean_generated_document(text):
    return (
        text
        .replace("—", "-")
        .replace("–", "-")
        .strip()
    )

def generate_resume_markdown(job_id):
    job = get_job_for_document(job_id)

    if not job:
        raise ValueError(
            f"No kept job found for job_id={job_id}"
        )

    master_resume = load_master_resume()

    analysis = job.get("analysis") or {}

    prompt = f"""
MASTER RESUME
=============
{master_resume}


TARGET JOB
==========
Title: {job["title"]}
Company: {job["company"]}
Location: {job["location"]}
Work type: {job["work_type"]}
Classification: {job["classification"]}


RELEVANCE ANALYSIS
==================
Relevance score:
{job["relevance_score"]}

Matched requirements:
{analysis.get("matched_requirements", [])}

Missing requirements:
{analysis.get("missing_requirements", [])}


JOB DESCRIPTION
===============
{job["description"]}


TASK
====
Create a tailored version of the master resume for this role.

Emphasise the candidate's strongest genuine matches.

Do not try to compensate for missing requirements by inventing
experience or technologies.

Return only the finished resume in Markdown.
"""

    client = OpenAI()

    response = client.responses.create(
        model=MODEL,
        input=[
            {
                "role": "system",
                "content": RESUME_SYSTEM_PROMPT,
            },
            {
                "role": "user",
                "content": prompt,
            },
        ],
    )

    resume_markdown = clean_generated_document(
        response.output_text
    )

    if not resume_markdown:
        raise RuntimeError(
            "OpenAI returned an empty resume."
        )

    save_resume_markdown(
        job_id=job_id,
        resume_markdown=resume_markdown,
    )

    return resume_markdown

def generate_cover_letter_markdown(job_id):
    job = get_job_for_document(job_id)

    if not job:
        raise ValueError(
            f"No kept job found for job_id={job_id}"
        )

    master_resume = load_master_resume()

    analysis = job.get("analysis") or {}

    prompt = f"""
MASTER RESUME
=============
{master_resume}


TARGET JOB
==========
Title: {job["title"]}
Company: {job["company"]}
Location: {job["location"]}
Work type: {job["work_type"]}
Classification: {job["classification"]}


RELEVANCE ANALYSIS
==================
Relevance score:
{job["relevance_score"]}

Matched requirements:
{analysis.get("matched_requirements", [])}

Missing requirements:
{analysis.get("missing_requirements", [])}


JOB DESCRIPTION
===============
{job["description"]}


TASK
====
Write a tailored cover letter for this specific job.

Focus on the strongest genuine connections between the candidate's background
and the role.

Use measurable achievements where relevant.

Do not mention missing requirements unless there is a compelling reason.

Do not invent experience to address gaps.

Return only the finished cover letter in Markdown.
"""

    client = OpenAI()

    response = client.responses.create(
        model=MODEL,
        input=[
            {
                "role": "system",
                "content": COVER_LETTER_SYSTEM_PROMPT,
            },
            {
                "role": "user",
                "content": prompt,
            },
        ],
    )

    cover_letter_markdown = clean_generated_document(
        response.output_text
    )

    if not cover_letter_markdown:
        raise RuntimeError(
            "OpenAI returned an empty cover letter."
        )

    save_cover_letter_markdown(
        job_id=job_id,
        cover_letter_markdown=cover_letter_markdown,
    )

    return cover_letter_markdown

if __name__ == "__main__":
    import sys

    if len(sys.argv) != 3:
        print(
            "Usage:"
            "\n  python src/document_generator.py "
            "resume <job_id>"
            "\n  python src/document_generator.py "
            "cover <job_id>"
        )
        raise SystemExit(1)

    document_type = sys.argv[1].lower()
    job_id = sys.argv[2]

    if document_type == "resume":
        result = generate_resume_markdown(
            job_id
        )

    elif document_type == "cover":
        result = generate_cover_letter_markdown(
            job_id
        )

    else:
        raise ValueError(
            "Document type must be "
            "'resume' or 'cover'."
        )

    print(result)