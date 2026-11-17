//! Word (.docx) export: one clean, single-column, ATS-friendly template.
//! No tables, text boxes, columns or images, which ATS parsers often misread.

use docx_rs::{
    AbstractNumbering, Docx, IndentLevel, Level, LevelJc, LevelText, LineSpacing, NumberFormat,
    Numbering, NumberingId, PageMargin, Paragraph, Run, RunFonts, SpecialIndentType, Start,
};

use super::draft::{DraftEntry, LetterDraft, ResumeDraft, split_headline};
use super::resume::skill_line;

const FONT: &str = "Calibri";
/// docx-rs already defines numbering 1 (a numbered list), so bullets use 2.
const BULLETS: usize = 2;

/// Text split into (text, bold) runs, with Markdown links shown as their URL
/// ("[LinkedIn](https://www.linkedin.com/in/x)" -> "linkedin.com/in/x").
pub fn runs(markdown: &str) -> Vec<(String, bool)> {
    let mut text = String::new();
    let mut rest = markdown;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some(close) = after.find("](") else { break };
        let Some(end) = after[close + 2..].find(')') else {
            break;
        };
        let url = &after[close + 2..close + 2 + end];
        text.push_str(&rest[..open]);
        text.push_str(
            url.trim_start_matches("https://")
                .trim_start_matches("http://")
                .trim_start_matches("www.")
                .trim_end_matches('/'),
        );
        rest = &after[close + 2 + end + 1..];
    }
    text.push_str(rest);

    text.split("**")
        .enumerate()
        .filter(|(_, part)| !part.is_empty())
        .map(|(i, part)| (part.to_string(), i % 2 == 1))
        .collect()
}

fn para(markdown: &str, size: usize) -> Paragraph {
    let mut p = Paragraph::new().line_spacing(LineSpacing::new().before(0).after(40));
    for (text, bold) in runs(markdown) {
        let mut run = Run::new().add_text(text).size(size);
        if bold {
            run = run.bold();
        }
        p = p.add_run(run);
    }
    p
}

fn heading(text: &str) -> Paragraph {
    Paragraph::new()
        .line_spacing(LineSpacing::new().before(200).after(60))
        .add_run(
            Run::new()
                .add_text(text.to_uppercase())
                .bold()
                .size(24)
                .color("1F3A5F"),
        )
}

fn bullet(text: &str) -> Paragraph {
    para(text, 21).numbering(NumberingId::new(BULLETS), IndentLevel::new(0))
}

fn base() -> Docx {
    Docx::new()
        .default_fonts(
            RunFonts::new()
                .ascii(FONT)
                .hi_ansi(FONT)
                .cs(FONT)
                .east_asia(FONT),
        )
        .default_size(21)
        .page_margin(PageMargin::new().top(720).bottom(720).left(860).right(860))
        .add_abstract_numbering(
            AbstractNumbering::new(BULLETS).add_level(
                Level::new(
                    0,
                    Start::new(1),
                    NumberFormat::new("bullet"),
                    LevelText::new("•"),
                    LevelJc::new("left"),
                )
                .indent(
                    Some(360),
                    Some(SpecialIndentType::Hanging(240)),
                    None,
                    None,
                ),
            ),
        )
        .add_numbering(Numbering::new(BULLETS, BULLETS))
}

fn header(mut doc: Docx, name: &str, contact: &[String]) -> Docx {
    doc = doc.add_paragraph(
        Paragraph::new()
            .line_spacing(LineSpacing::new().after(40))
            .add_run(Run::new().add_text(name).bold().size(36)),
    );
    let (headline, contact) = split_headline(contact);
    if let Some(title) = headline {
        doc = doc.add_paragraph(
            Paragraph::new()
                .line_spacing(LineSpacing::new().after(40))
                .add_run(Run::new().add_text(title).bold().color("1F4E79").size(26)),
        );
    }
    if !contact.is_empty() {
        let lines: Vec<&str> = contact.iter().map(|l| l.as_str()).collect();
        doc = doc.add_paragraph(para(&lines.join(" | "), 20));
    }
    doc
}

fn entries(mut doc: Docx, title: &str, list: &[DraftEntry]) -> Docx {
    if list.is_empty() {
        return doc;
    }
    doc = doc.add_paragraph(heading(title));
    for entry in list {
        doc = doc.add_paragraph(
            Paragraph::new()
                .line_spacing(LineSpacing::new().before(80).after(20))
                .add_run(Run::new().add_text(&entry.heading).bold().size(22)),
        );
        if !entry.details.is_empty() {
            doc = doc.add_paragraph(para(&entry.details.join(" | "), 20));
        }
        let mut group: Option<&String> = None;
        for b in &entry.bullets {
            if b.group.is_some() && b.group.as_ref() != group {
                group = b.group.as_ref();
                doc = doc.add_paragraph(
                    Paragraph::new()
                        .line_spacing(LineSpacing::new().before(60).after(20))
                        .add_run(
                            Run::new()
                                .add_text(b.group.as_deref().unwrap_or(""))
                                .bold()
                                .color("1F4E79")
                                .size(21),
                        ),
                );
            }
            doc = doc.add_paragraph(bullet(&b.text));
        }
    }
    doc
}

fn pack(doc: Docx) -> anyhow::Result<Vec<u8>> {
    let mut buffer = std::io::Cursor::new(Vec::new());
    doc.build().pack(&mut buffer)?;
    Ok(buffer.into_inner())
}

pub fn resume_docx(draft: &ResumeDraft) -> anyhow::Result<Vec<u8>> {
    let mut doc = header(base(), &draft.name, &draft.contact);

    if !draft.summary.text.is_empty() {
        doc = doc.add_paragraph(heading("Professional Summary"));
        doc = doc.add_paragraph(para(&draft.summary.text, 21));
    }
    if !draft.skills.is_empty() {
        doc = doc.add_paragraph(heading("Skills"));
        for group in &draft.skills {
            let line = if group.group.is_empty() {
                skill_line(group)
            } else {
                format!("**{}:** {}", group.group, group.items.join(", "))
            };
            doc = doc.add_paragraph(para(&line, 21));
        }
    }
    doc = entries(doc, "Experience", &draft.experience);
    doc = entries(doc, "Projects", &draft.projects);
    doc = entries(doc, "Education", &draft.education);
    for section in &draft.other {
        doc = doc.add_paragraph(heading(&section.heading));
        for (id, text) in &section.lines {
            doc = doc.add_paragraph(if id.is_empty() {
                para(text, 21)
            } else {
                bullet(text)
            });
        }
    }
    pack(doc)
}

pub fn letter_docx(draft: &LetterDraft, date: &str) -> anyhow::Result<Vec<u8>> {
    let gap = || Paragraph::new().line_spacing(LineSpacing::new().before(0).after(160));
    let mut doc = header(base(), &draft.name, &draft.contact);
    doc = doc.add_paragraph(gap().add_run(Run::new().add_text(date).size(21)));
    doc = doc.add_paragraph(gap().add_run(Run::new().add_text(&draft.greeting).size(22)));
    for p in &draft.paragraphs {
        doc = doc.add_paragraph(gap().add_run(Run::new().add_text(&p.text).size(22)));
    }
    doc = doc.add_paragraph(gap().add_run(Run::new().add_text(&draft.closing).size(22)));
    doc = doc.add_paragraph(gap().add_run(Run::new().add_text(&draft.name).size(22)));
    pack(doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::documents::draft::Line;

    #[test]
    fn links_become_urls_and_bold_becomes_runs() {
        assert_eq!(
            runs("[LinkedIn](https://www.linkedin.com/in/x/) | **Role** at X"),
            vec![
                ("linkedin.com/in/x | ".to_string(), false),
                ("Role".into(), true),
                (" at X".into(), false)
            ]
        );
    }

    #[test]
    fn builds_a_valid_docx() {
        let draft = ResumeDraft {
            name: "Jane".into(),
            contact: vec!["Sydney".into()],
            summary: Line {
                text: "Analyst.".into(),
                evidence: vec![],
                group: None,
            },
            experience: vec![DraftEntry {
                heading: "Acme".into(),
                details: vec!["**Analyst**".into()],
                bullets: vec![Line {
                    text: "Built reports.".into(),
                    evidence: vec![],
                    group: None,
                }],
                ..DraftEntry::default()
            }],
            ..ResumeDraft::default()
        };
        let bytes = resume_docx(&draft).unwrap();
        assert_eq!(&bytes[..2], b"PK", "a .docx is a zip file");

        // Bullets must be a real bullet list, not docx-rs's built-in numbered list (id 1).
        let numbering = String::from_utf8(base().build().numberings).unwrap();
        assert!(numbering.contains(r#"<w:num w:numId="2"><w:abstractNumId w:val="2" />"#));
        assert!(numbering.contains(r#"<w:numFmt w:val="bullet" />"#));

        let letter = LetterDraft {
            name: "Jane".into(),
            paragraphs: vec![Line::default()],
            ..LetterDraft::default()
        };
        assert_eq!(
            &letter_docx(&letter, "14 November 2026").unwrap()[..2],
            b"PK"
        );
    }
}
