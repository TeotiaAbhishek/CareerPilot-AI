//! Signs that a line reads as AI-written, from Wikipedia's "Signs of AI
//! writing" guide, narrowed to what shows up in resumes, letters and emails.
//!
//! One word on its own proves nothing, so most of these are "info" flags:
//! a nudge to reword, never a block. Leftovers no person would send (a
//! chatbot's "I hope this helps", a "[Your Name]" placeholder, a citation
//! marker) are errors.

use super::check::{Flag, Severity};

/// Words and phrases that are far more common in model output than in
/// resumes people write, with a plainer choice. A trailing `*` matches any
/// ending: "leverag*" finds leverage, leveraged and leveraging.
const AI_WORDS: &[(&str, &str)] = &[
    ("delv*", "look into"),
    ("tapestry", "say what it is"),
    ("testament", "say what it shows"),
    ("pivotal", "important"),
    ("showcas*", "show"),
    ("underscor*", "show"),
    ("vibrant", "drop it"),
    ("intricat*", "complex"),
    ("interplay", "how X and Y affect each other"),
    ("garner*", "get, win"),
    ("foster*", "build"),
    ("bolster*", "strengthen"),
    ("enduring", "drop it"),
    ("realm", "area"),
    ("landscape", "name the field"),
    ("seamless*", "smooth"),
    ("leverag*", "use"),
    ("utiliz*", "use"),
    ("utilis*", "use"),
    ("spearhead*", "led, started"),
    ("synerg*", "say what worked together"),
    ("cutting-edge", "name the technology"),
    ("groundbreaking", "say what was new"),
    ("game-chang*", "say what changed"),
    ("transformative", "say what changed"),
    ("meticulous*", "careful"),
    ("commendable", "drop it"),
    ("holistic*", "complete, end-to-end"),
    ("multifaceted", "varied"),
    ("paramount", "important"),
    ("noteworthy", "drop it"),
    ("embark*", "start"),
    ("elevat*", "improve"),
    ("unlock*", "make possible"),
    ("empower*", "help, let"),
    ("harnessing", "using"),
    ("harness the", "use the"),
    ("navigat* the", "handle"),
    ("ever-evolving", "changing"),
    ("fast-paced", "drop it"),
    ("dynamic", "drop it"),
    ("passionate about", "say what you did"),
    ("proven track record", "give the result instead"),
    ("results-driven", "give the result instead"),
    ("in today's", "drop it"),
    ("serves as", "is"),
    ("stands as", "is"),
    ("plays a pivotal role", "say what it does"),
    ("plays a crucial role", "say what it does"),
    ("plays a vital role", "say what it does"),
    ("not only", "say both plainly"),
    ("not just", "say it plainly"),
    ("boasts", "has"),
    ("in the heart of", "in"),
    ("world-class", "drop it"),
    ("best-in-class", "drop it"),
    ("deep dive", "analysis"),
    ("at its core", "drop it"),
];

/// ", ensuring …" style endings that tack significance onto a sentence.
const ING_TAILS: &[&str] = &[
    "ensuring",
    "highlighting",
    "showcasing",
    "reflecting",
    "underscoring",
    "demonstrating",
    "fostering",
    "emphasising",
    "emphasizing",
    "solidifying",
    "cementing",
    "contributing to",
];

/// Sentence openers models lean on.
const OPENERS: &[&str] = &[
    "additionally",
    "furthermore",
    "moreover",
    "in conclusion",
    "overall",
];

/// Things no person sends: chatbot chatter, placeholders, citation markers.
const LEFTOVERS: &[&str] = &[
    "certainly!",
    "i hope this helps",
    "let me know if",
    "here's a",
    "here is a",
    "as an ai",
    "as of my last",
    "[your ",
    "[company",
    "[insert",
    "insert_",
    "utm_source",
    "oaicite",
    "contentreference",
    "[cite",
];

/// Style flags for one line the model wrote.
pub fn check(path: &str, text: &str) -> Vec<Flag> {
    let lower = text.to_lowercase().replace('’', "'");
    let mut flags = Vec::new();
    let mut flag = |kind: &str, detail: &str, severity: Severity, message: String| {
        flags.push(Flag {
            key: format!("{kind}|{path}|{detail}"),
            path: path.to_string(),
            severity,
            message,
        })
    };

    for leftover in LEFTOVERS {
        if lower.contains(leftover) {
            flag(
                "leftover",
                leftover,
                Severity::Error,
                format!(
                    "\"{}\" looks like chatbot text or a placeholder. Remove it.",
                    leftover.trim()
                ),
            );
        }
    }
    for (pattern, instead) in AI_WORDS {
        if let Some(found) = find(&lower, pattern) {
            flag(
                "ai_word",
                pattern,
                Severity::Info,
                format!("\"{found}\" reads as AI-written. Plainer: {instead}."),
            );
        }
    }
    for tail in ING_TAILS {
        if lower.contains(&format!(", {tail}")) {
            flag(
                "ing_tail",
                tail,
                Severity::Info,
                format!(
                    "\", {tail} …\" at the end reads as AI-written. End the sentence, or say the result."
                ),
            );
        }
    }
    for opener in OPENERS {
        let starts = lower.starts_with(opener) || lower.contains(&format!(". {opener}"));
        if starts {
            flag(
                "opener",
                opener,
                Severity::Info,
                format!("Starting with \"{opener}\" reads as AI-written. Just say the next thing."),
            );
        }
    }
    flags
}

/// The first match of `pattern` at a word start, as written in `lower`.
fn find(lower: &str, pattern: &str) -> Option<String> {
    let (stem, any_ending) = match pattern.strip_suffix('*') {
        Some(stem) => (stem, true),
        None => (pattern, false),
    };
    // "navigat* the": the star sits inside the phrase.
    if let Some((head, tail)) = stem.split_once("* ") {
        let at = word_start(lower, head)?;
        let rest = &lower[at + head.len()..];
        let end = rest
            .find(|c: char| !c.is_alphabetic())
            .unwrap_or(rest.len());
        return rest[end..]
            .strip_prefix(' ')
            .filter(|r| r.starts_with(tail))
            .map(|_| lower[at..at + head.len() + end + 1 + tail.len()].to_string());
    }
    let mut from = 0;
    while let Some(i) = lower[from..].find(stem).map(|i| i + from) {
        let before_ok = lower[..i]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        let after = &lower[i + stem.len()..];
        let end = if any_ending {
            after
                .find(|c: char| !c.is_alphabetic())
                .unwrap_or(after.len())
        } else {
            0
        };
        let after_ok = after[end..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric());
        if before_ok && after_ok {
            return Some(lower[i..i + stem.len() + end].to_string());
        }
        from = i + stem.len();
    }
    None
}

fn word_start(lower: &str, word: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = lower[from..].find(word).map(|i| i + from) {
        if lower[..i]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric())
        {
            return Some(i);
        }
        from = i + word.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<String> {
        check("x", text)
            .iter()
            .map(|f| {
                let mut parts = f.key.split('|');
                let kind = parts.next().unwrap();
                format!("{kind}:{}", parts.nth(1).unwrap())
            })
            .collect()
    }

    #[test]
    fn plain_bullets_pass() {
        assert!(kinds("Built Power BI dashboards that 40 managers use every week.").is_empty());
        assert!(kinds("Led a team of three analysts and cut report time by 60%.").is_empty());
        // Words that only start like a flagged one are fine.
        assert!(
            kinds("Delivered a booking app for a landscaper, with notes for each job.").is_empty()
        );
    }

    #[test]
    fn flags_ai_words_with_any_ending() {
        let flags = check(
            "x",
            "Leveraged SQL to spearhead a robust, seamless pipeline.",
        );
        let messages: Vec<&str> = flags.iter().map(|f| f.message.as_str()).collect();
        assert!(
            messages.contains(&"\"leveraged\" reads as AI-written. Plainer: use."),
            "{messages:?}"
        );
        let mut k = kinds("Leveraged SQL to spearhead a robust, seamless pipeline.");
        k.sort();
        assert_eq!(
            k,
            vec![
                "ai_word:leverag*",
                "ai_word:seamless*",
                "ai_word:spearhead*"
            ]
        );
        assert!(flags.iter().all(|f| f.severity == Severity::Info));
    }

    #[test]
    fn flags_ing_tails_openers_and_parallels() {
        let mut k = kinds(
            "Built a churn model, ensuring retention. Additionally, it is not just a model; it serves as a tool.",
        );
        k.sort();
        assert_eq!(
            k,
            vec![
                "ai_word:not just",
                "ai_word:serves as",
                "ing_tail:ensuring",
                "opener:additionally"
            ]
        );
        assert_eq!(
            kinds("Navigated the move to Azure."),
            vec!["ai_word:navigat* the"]
        );
    }

    #[test]
    fn leftovers_are_errors() {
        let flags = check(
            "x",
            "I hope this helps! Contact [Your Name] at utm_source=chatgpt.com",
        );
        assert_eq!(flags.len(), 3);
        assert!(flags.iter().all(|f| f.severity == Severity::Error));
    }
}
