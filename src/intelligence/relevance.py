from pathlib import Path
import yaml
from openai import OpenAI
from pydantic import BaseModel, Field

from src.database import (
    get_unanalysed_jobs,
    save_job_analysis,
)


PROJECT_ROOT = Path(__file__).resolve().parents[2]
CANDIDATE_PROFILE_PATH = PROJECT_ROOT / "config" / "candidate_profile.yaml"
MASTER_RESUME_PATH = PROJECT_ROOT / "data" / "master_resume.md"

MODEL = "gpt-5.6-luna"
RELEVANCE_THRESHOLD = 60
BATCH_SIZE = 100


class JobAnalysis(BaseModel):
    score: int = Field(ge=0, le=100)
    summary: str
    matched_requirements: list[str]
    missing_requirements: list[str]
    unknown_requirements: list[str]
    hard_blockers: list[str]

SYSTEM_PROMPT = """
You are evaluating how relevant a job opportunity is for a candidate.

Your job is to compare the candidate's MASTER RESUME against the JOB DESCRIPTION
and produce an accurate, conservative, and explainable assessment.

SOURCE OF TRUTH
---------------
Use ONLY information explicitly provided in:
1. the candidate's MASTER RESUME,
2. the CANDIDATE PROFILE,
3. the supplied JOB DESCRIPTION.

The MASTER RESUME is the source of truth for professional skills,
experience, education, projects, and achievements.

The CANDIDATE PROFILE is the source of truth for eligibility information
such as citizenship, residency, visa status, work rights, location,
licences, availability, and other job-search constraints.

Do not invent or infer information that is absent from both sources.

Do NOT invent, assume, infer, or fabricate:
- skills
- technologies
- qualifications
- certifications
- employment history
- years of experience
- achievements
- licences
- citizenship
- residency status
- security clearances
- work rights
- personal circumstances

If information is not available, classify it appropriately as unknown rather
than assuming the candidate does or does not satisfy the requirement.


ASSESSMENT PRINCIPLES
---------------------
Evaluate the candidate against the actual responsibilities and requirements
of the job, not merely the job title.

Give greater importance to:
1. Mandatory requirements
2. Core technical skills
3. Relevant professional experience
4. Seniority and required years of experience
5. Domain experience where genuinely important to the role

Give less importance to:
- nice-to-have requirements
- preferred technologies
- minor tooling differences
- location alignment
- generic soft skills unless explicitly critical

Equivalent or transferable technologies and experience should receive
reasonable credit.

Do not heavily penalise the candidate for missing a tool when their resume
demonstrates closely related experience that could reasonably transfer.

However, do not treat vaguely related experience as equivalent to specialised
professional expertise.


SCORING
-------
The numerical relevance score must measure PROFESSIONAL FIT only.

Base the score on:
- skills
- experience
- seniority
- qualifications
- domain alignment
- responsibilities

Do NOT reduce the numerical relevance score because of eligibility requirements
such as citizenship, residency, work rights, licences, background checks, or
security clearances.

Eligibility issues must instead be represented in unknown_requirements or
hard_blockers.

This separation is important because a candidate may be an excellent
professional match for a role while still being legally or administratively
ineligible to apply.

Return a relevance score from 0 to 100.

Use the following guide:

90-100:
Excellent match.
The candidate satisfies nearly all important requirements and has directly
relevant experience for the role.

75-89:
Strong match.
The candidate satisfies most core requirements. Missing items are mostly
preferred, learnable, or relatively minor.

60-74:
Reasonable match.
The candidate has relevant and transferable experience but has some meaningful
gaps. The role is still realistically worth considering.

40-59:
Weak match.
There is some relevant experience, but several important requirements,
technical capabilities, or seniority expectations are missing.

20-39:
Poor match.
The role has limited alignment with the candidate's demonstrated background
and would require substantial missing expertise.

0-19:
Very poor match.
The role is fundamentally outside the candidate's demonstrated profession,
experience, or qualifications.


SENIORITY AND EXPERIENCE
------------------------
Consider explicit minimum experience requirements carefully.

A small experience gap should reduce the score but should not automatically
become a hard blocker.

A substantially higher seniority requirement should have a meaningful impact
on the score.

For example:
- 2 years requested vs roughly 1.5 years relevant experience:
  meaningful gap, but not necessarily a blocker.

- 7+ years of specialised experience requested vs roughly 1-2 years:
  major mismatch.

Do not calculate or invent years of experience beyond what the dates and
experience in the resume reasonably support.


REQUIREMENT CLASSIFICATION
--------------------------

matched_requirements:
Requirements clearly supported by the candidate's master resume.

Include meaningful technical skills, relevant experience, education, domain
experience, or other important requirements that genuinely strengthen the fit.

Do not pad this list with weak or trivial similarities.

Return at most 8 items.


missing_requirements:
Skills, experience, qualifications, or capabilities requested in the job
description that are not demonstrated in the candidate's master resume.

Use this for genuine professional gaps.

A missing requirement is not automatically a hard blocker.

Return at most 8 items.


unknown_requirements:
Requirements where the supplied candidate information is insufficient to
determine whether the candidate satisfies them.

Examples include:
- citizenship
- permanent residency
- work-right status
- driver's licence
- willingness to travel
- willingness to relocate
- background checks
- security-clearance eligibility
- personal availability

Do NOT classify these as missing skills or hard blockers simply because they
are absent from the supplied candidate information.

Return only meaningful unknown requirements.


hard_blockers:
Use ONLY when the information provided explicitly proves that the candidate
does not satisfy a mandatory requirement that would realistically prevent
their application from progressing.

Do NOT classify something as a hard blocker merely because the supplied
candidate information does not mention it.

Examples of possible hard blockers, when explicitly contradicted by the
candidate information, include:
- mandatory citizenship or residency requirement
- mandatory existing security clearance
- mandatory professional licence
- mandatory registration
- mandatory degree or qualification
- explicit legal eligibility requirement

If a job says the successful candidate must be able to OBTAIN a security
clearance, do NOT treat lack of an existing clearance as a blocker unless
the job explicitly requires the applicant to already hold that clearance.

A missing technology, preferred certification, nice-to-have skill, or moderate
experience gap is NOT a hard blocker.


LOCATION
--------
Location alignment may be noted when relevant, but it should have only a minor
effect on the relevance score.

Do not count living in the same city as a major matched professional
requirement.


SUMMARY
-------
Provide a concise explanation of the overall match.

The summary must:
- be no more than 2 short sentences
- explain the main reason for the score
- mention the most important strength and/or gap
- avoid generic language
- not repeat every item from the lists


OUTPUT BEHAVIOUR
----------------
Be conservative, consistent, and factual.

Do not exaggerate candidate suitability.

Do not lower the score merely because information about personal eligibility
is unknown.

Do not increase the score because of superficial keyword overlap.

The score should represent how well the candidate's demonstrated professional
background matches the role.

Eligibility and administrative constraints should be reported separately and
must not alter the professional-fit score.

Return only the structured output required by the supplied schema.
"""


def load_master_resume():
    return MASTER_RESUME_PATH.read_text(encoding="utf-8")

def load_candidate_profile():
    with open(CANDIDATE_PROFILE_PATH, "r", encoding="utf-8") as file:
        return yaml.safe_load(file)

def analyse_job(
    client,
    resume,
    candidate_profile,
    job,
):

    prompt = f"""
CANDIDATE MASTER RESUME
=======================
{resume}


CANDIDATE PROFILE
=================
{yaml.safe_dump(
    candidate_profile,
    sort_keys=False,
    allow_unicode=True
)}


JOB
===
Title: {job["title"]}
Company: {job["company"]}
Location: {job["location"]}
Work type: {job["work_type"]}


JOB DESCRIPTION
===============
{job["description"]}
"""

    response = client.responses.parse(
        model=MODEL,
        input=[
            {
                "role": "system",
                "content": SYSTEM_PROMPT,
            },
            {
                "role": "user",
                "content": prompt,
            },
        ],
        text_format=JobAnalysis,
    )

    if response.output_parsed is None:
        raise RuntimeError("OpenAI did not return a parsed relevance result.")

    return response.output_parsed


def main():
    resume = load_master_resume()
    candidate_profile = load_candidate_profile()

    jobs = get_unanalysed_jobs(
        limit = BATCH_SIZE
    )

    if not jobs:
        print("No unanalysed jobs found.")
        return

    client = OpenAI()

    kept = 0
    rejected = 0
    failed = 0

    print()
    print(f"Found {len(jobs)} unanalysed jobs.")
    print()

    for job in jobs:
        print(
            f'Analysing: {job["title"]} '
            f'| {job["company"]}'
        )

        try:
            analysis = analyse_job(
                client,
                resume,
                candidate_profile,
                job,
            )

            is_relevant = (
                analysis.score >= RELEVANCE_THRESHOLD
                and len(analysis.hard_blockers) == 0
            )
            
            save_job_analysis(
                job_id=job["job_id"],
                relevance_score=analysis.score,
                kept=is_relevant,
                analysis=analysis.model_dump(),
                model=MODEL,
            )
            
            if is_relevant:
                kept += 1
            
                print(
                    f'  KEEP | Score: {analysis.score}'
                )
            
            else:
                rejected += 1
            
                reason = []
            
                if analysis.score < RELEVANCE_THRESHOLD:
                    reason.append(
                        f"score {analysis.score}"
                    )
            
                if analysis.hard_blockers:
                    reason.append(
                        f"{len(analysis.hard_blockers)} "
                        "hard blocker(s)"
                    )
            
                print(
                    "  REJECT | "
                    + ", ".join(reason)
                )

        except Exception as error:
            failed += 1

            print(
                f"  ERROR | {error}"
            )

        print()

    print("=" * 50)
    print("RELEVANCE ANALYSIS COMPLETE")
    print("=" * 50)

    print(f"Analysed: {len(jobs)}")
    print(f"Kept:     {kept}")
    print(f"Rejected: {rejected}")
    print(f"Failed:   {failed}")
    if failed > 0:
        raise SystemExit(1)

if __name__ == "__main__":
    main()