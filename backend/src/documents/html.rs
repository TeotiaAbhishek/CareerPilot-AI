//! HTML for the PDF versions of a resume and a cover letter. The browser
//! worker prints it to A4 (see `pdf.rs`). Same single-column, ATS-friendly
//! layout as the .docx: real text, no tables, columns or images.

use super::docx::runs;
use super::draft::{DraftEntry, LetterDraft, ResumeDraft};
use super::resume::skill_line;

const STYLE: &str = r#"
  * { box-sizing: border-box; }
  body { margin: 0; color: #111; font: 10.5pt/1.35 Calibri, Carlito, "Segoe UI", Arial, sans-serif; }
  h1 { font-size: 20pt; margin: 0 0 2pt; }
  .contact { font-size: 9.5pt; margin-bottom: 6pt; }
  h2 { font-size: 11pt; text-transform: uppercase; letter-spacing: 0.04em; border-bottom: 1px solid #999;
       margin: 9pt 0 4pt; padding-bottom: 1pt; }
  .entry { margin-top: 4pt; break-inside: avoid; }
  .entry h3 { font-size: 10.5pt; margin: 0; }
  .details { font-size: 10pt; }
  ul { margin: 2pt 0 0; padding-left: 14pt; }
  li { margin: 0 0 1pt; }
  p { margin: 0 0 3pt; }
  .letter p { margin: 0 0 9pt; font-size: 11pt; }
"#;

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Markdown-ish text (bold, links) as HTML.
fn inline(markdown: &str) -> String {
    runs(markdown)
        .into_iter()
        .map(|(text, bold)| {
            if bold {
                format!("<strong>{}</strong>", escape(&text))
            } else {
                escape(&text)
            }
        })
        .collect()
}

fn page(title: &str, body: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>{}</title><style>{STYLE}</style></head><body>{body}</body></html>",
        escape(title)
    )
}

fn header(name: &str, contact: &[String]) -> String {
    let mut out = format!("<h1>{}</h1>", escape(name));
    if !contact.is_empty() {
        out.push_str(&format!(
            "<div class=\"contact\">{}</div>",
            inline(&contact.join(" | "))
        ));
    }
    out
}

fn entries(title: &str, list: &[DraftEntry]) -> String {
    if list.is_empty() {
        return String::new();
    }
    let mut out = format!("<h2>{}</h2>", escape(title));
    for entry in list {
        out.push_str("<div class=\"entry\">");
        out.push_str(&format!("<h3>{}</h3>", escape(&entry.heading)));
        if !entry.details.is_empty() {
            out.push_str(&format!(
                "<div class=\"details\">{}</div>",
                inline(&entry.details.join(" | "))
            ));
        }
        if !entry.bullets.is_empty() {
            out.push_str("<ul>");
            for b in &entry.bullets {
                out.push_str(&format!("<li>{}</li>", inline(&b.text)));
            }
            out.push_str("</ul>");
        }
        out.push_str("</div>");
    }
    out
}

pub fn resume_html(draft: &ResumeDraft) -> String {
    let mut body = header(&draft.name, &draft.contact);
    if !draft.summary.text.is_empty() {
        body.push_str("<h2>Professional Summary</h2>");
        body.push_str(&format!("<p>{}</p>", inline(&draft.summary.text)));
    }
    if !draft.skills.is_empty() {
        body.push_str("<h2>Skills</h2>");
        for group in &draft.skills {
            let line = if group.group.is_empty() {
                skill_line(group)
            } else {
                format!("**{}:** {}", group.group, group.items.join(", "))
            };
            body.push_str(&format!("<p>{}</p>", inline(&line)));
        }
    }
    body.push_str(&entries("Experience", &draft.experience));
    body.push_str(&entries("Projects", &draft.projects));
    body.push_str(&entries("Education", &draft.education));
    for section in &draft.other {
        body.push_str(&format!("<h2>{}</h2>", escape(&section.heading)));
        let (bullets, plain): (Vec<_>, Vec<_>) =
            section.lines.iter().partition(|(id, _)| !id.is_empty());
        for (_, text) in plain {
            body.push_str(&format!("<p>{}</p>", inline(text)));
        }
        if !bullets.is_empty() {
            body.push_str("<ul>");
            for (_, text) in bullets {
                body.push_str(&format!("<li>{}</li>", inline(text)));
            }
            body.push_str("</ul>");
        }
    }
    page(&format!("{} - Resume", draft.name), &body)
}

pub fn letter_html(draft: &LetterDraft, date: &str) -> String {
    let mut body = header(&draft.name, &draft.contact);
    body.push_str("<div class=\"letter\">");
    body.push_str(&format!("<p>{}</p>", escape(date)));
    body.push_str(&format!("<p>{}</p>", escape(&draft.greeting)));
    for p in &draft.paragraphs {
        body.push_str(&format!("<p>{}</p>", inline(&p.text)));
    }
    body.push_str(&format!(
        "<p>{}<br>{}</p>",
        escape(&draft.closing),
        escape(&draft.name)
    ));
    body.push_str("</div>");
    page(&format!("{} - Cover letter", draft.name), &body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::draft::Line;

    #[test]
    fn text_is_escaped_and_bold_and_links_survive() {
        let draft = ResumeDraft {
            name: "Jane <Citizen>".into(),
            contact: vec!["[LinkedIn](https://linkedin.com/in/jane)".into()],
            experience: vec![DraftEntry {
                heading: "Acme & Co".into(),
                details: vec!["**Analyst**".into(), "2022 – 2023".into()],
                bullets: vec![Line {
                    text: "Cut report time by **40%**.".into(),
                    evidence: vec![],
                }],
                ..DraftEntry::default()
            }],
            ..ResumeDraft::default()
        };
        let html = resume_html(&draft);
        assert!(html.contains("<h1>Jane &lt;Citizen&gt;</h1>"));
        assert!(html.contains("linkedin.com/in/jane"));
        assert!(html.contains("<h3>Acme &amp; Co</h3>"));
        assert!(html.contains("<strong>40%</strong>"));
    }
}
