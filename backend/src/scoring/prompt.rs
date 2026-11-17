//! The scoring prompt. It started as v1's prompt, word for word. v2 adds
//! the job's level and the years it asks for, so the code can cap the score
//! of a lead role for someone with two years' experience (see
//! `seniority_cap`). Change `PROMPT_VERSION` whenever you change the prompt:
//! scores are stored per version, so old and new results never get mixed up.

pub const PROMPT_VERSION: &str = "v2-seniority";

pub const SYSTEM_PROMPT: &str = r#"You are evaluating how relevant a job opportunity is for a candidate.

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
The CANDIDATE EXPERIENCE block gives the candidate's years of work
experience, worked out from the dates on the resume. Use those numbers and
do not recount them.

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

A lead, principal, staff or manager role expects several years of
experience, including leading work or people. Do not call it a strong
match for a candidate with only a few years, even when the skills match.


ROLE LEVEL AND REQUIRED YEARS
-----------------------------
role_level: the level the job is hiring for, from its title and
description:
- "entry": graduate, junior, associate, trainee, intern
- "mid": no level stated, or a plain title such as "Data Analyst"
- "senior": senior
- "lead": lead, principal, staff, architect
- "manager": manager, head of, director

required_years: the minimum years of experience the job explicitly asks
for, as a whole number ("5+ years" is 5, "3-5 years" is 3). Use 0 when the
job doesn't ask for a number of years.


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

Return only the structured output required by the supplied schema."#;

pub struct JobForPrompt<'a> {
    pub title: &'a str,
    pub company: Option<&'a str>,
    pub location: Option<&'a str>,
    pub work_type: Option<&'a str>,
    pub description: &'a str,
}

/// The user message: v1's layout, plus the years worked out in code.
pub fn user_prompt(
    resume: &str,
    experience: &str,
    profile_yaml: &str,
    job: &JobForPrompt<'_>,
) -> String {
    let experience = if experience.trim().is_empty() {
        "No dated jobs on the resume."
    } else {
        experience.trim()
    };
    format!(
        "CANDIDATE MASTER RESUME\n=======================\n{resume}\n\n\n\
         CANDIDATE EXPERIENCE\n====================\n{experience}\n\n\n\
         CANDIDATE PROFILE\n=================\n{profile_yaml}\n\n\n\
         JOB\n===\nTitle: {}\nCompany: {}\nLocation: {}\nWork type: {}\n\n\n\
         JOB DESCRIPTION\n===============\n{}\n",
        job.title,
        job.company.unwrap_or(""),
        job.location.unwrap_or(""),
        job.work_type.unwrap_or(""),
        job.description,
    )
}

/// The JSON shape the model must answer in. Ollama enforces it with `format`.
pub fn output_schema() -> serde_json::Value {
    let list = serde_json::json!({ "type": "array", "items": { "type": "string" } });
    serde_json::json!({
        "type": "object",
        "properties": {
            "role_level": {
                "type": "string",
                "enum": ["entry", "mid", "senior", "lead", "manager"]
            },
            "required_years": { "type": "integer", "minimum": 0, "maximum": 30 },
            "score": { "type": "integer", "minimum": 0, "maximum": 100 },
            "summary": { "type": "string" },
            "matched_requirements": list,
            "missing_requirements": list,
            "unknown_requirements": list,
            "hard_blockers": list
        },
        "required": [
            "role_level", "required_years", "score", "summary", "matched_requirements", "missing_requirements",
            "unknown_requirements", "hard_blockers"
        ]
    })
}

/// The most a job may score, given the level it hires for, the years it
/// asks for and your years of experience, with the reason. None when the
/// model's score can stand.
///
/// Models are kind: a lead role whose tools you know scores 90 even with
/// two years' experience. The model reads the level and the years off the
/// ad (it's good at that); this does the comparison (code is better at it).
pub fn seniority_cap(level: &str, required_years: i64, your_years: f64) -> Option<(i64, String)> {
    let mut caps: Vec<(i64, String)> = Vec::new();
    let years = format!("you have {your_years} years");
    match level {
        "manager" if your_years < 6.0 => {
            caps.push((45, format!("Manager role: {years} of experience")))
        }
        "lead" if your_years < 5.0 => caps.push((50, format!("Lead role: {years} of experience"))),
        "senior" if your_years < 3.0 => {
            caps.push((65, format!("Senior role: {years} of experience")))
        }
        _ => {}
    }
    if required_years > 0 {
        let gap = required_years as f64 - your_years;
        let asks = format!("Asks for {required_years}+ years: {years}");
        if gap >= 3.0 {
            caps.push((50, asks));
        } else if gap >= 1.5 {
            caps.push((65, asks));
        } else if gap > 0.5 {
            caps.push((79, asks));
        }
    }
    caps.into_iter().min_by_key(|(cap, _)| *cap)
}

/// Your resume without your name and the contact lines under it (the ones
/// with an email, phone number or link): they never go to a model.
pub fn resume_for_prompt(content: &str, contact: &[String]) -> String {
    let private: Vec<&str> = contact
        .iter()
        .map(|c| c.trim())
        .filter(|c| has_contact_details(c))
        .collect();
    content
        .lines()
        .filter(|line| !private.contains(&line.trim()) && !line.trim_start().starts_with("# "))
        .collect::<Vec<_>>()
        .join("\n")
}

fn has_contact_details(line: &str) -> bool {
    let lower = line.to_lowercase();
    let digits = line.chars().filter(char::is_ascii_digit).count();
    lower.contains('@')
        || lower.contains("http")
        || lower.contains("www.")
        || lower.contains("linkedin")
        || lower.contains("github")
        || digits >= 7
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lead_roles_are_capped_for_a_few_years_experience() {
        assert_eq!(
            seniority_cap("lead", 0, 1.8),
            Some((50, "Lead role: you have 1.8 years of experience".into()))
        );
        assert_eq!(seniority_cap("lead", 0, 6.0), None);
        assert_eq!(seniority_cap("manager", 0, 3.0).unwrap().0, 45);
        assert_eq!(seniority_cap("senior", 0, 2.0).unwrap().0, 65);
        assert_eq!(seniority_cap("mid", 0, 1.0), None);
    }

    #[test]
    fn the_years_gap_caps_too_and_the_lowest_cap_wins() {
        assert_eq!(seniority_cap("mid", 5, 1.8).unwrap().0, 50);
        assert_eq!(seniority_cap("mid", 3, 1.4).unwrap().0, 65);
        assert_eq!(seniority_cap("mid", 2, 1.4).unwrap().0, 79);
        assert_eq!(seniority_cap("mid", 2, 1.6), None);
        assert_eq!(
            seniority_cap("senior", 7, 2.0),
            Some((50, "Asks for 7+ years: you have 2 years".into()))
        );
    }

    #[test]
    fn contact_lines_are_left_out() {
        let resume = "# Alex Morgan\nData Analyst\nSydney | 0400 000 000 | alex@example.com\n\n## Experience";
        let contact = vec![
            "Data Analyst".to_string(),
            "Sydney | 0400 000 000 | alex@example.com".to_string(),
        ];
        assert_eq!(
            resume_for_prompt(resume, &contact),
            "Data Analyst\n\n## Experience"
        );
    }
}
