//! How well a model's scores agree with your labels. Pure functions, no
//! database, so they're easy to test.
//!
//! The model says a job is relevant when score >= threshold and it found no
//! hard blockers (the same rule the app uses). Your label says "apply" (yes)
//! or "no" (no). "maybe" jobs are left out of agreement, precision and
//! recall, but still count towards the average score per label.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    Apply,
    Maybe,
    No,
}

impl Label {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "apply" => Some(Label::Apply),
            "maybe" => Some(Label::Maybe),
            "no" => Some(Label::No),
            _ => None,
        }
    }
}

/// One labelled job that a model has scored.
#[derive(Debug, Clone)]
pub struct Scored {
    pub label: Label,
    pub score: i32,
    pub blockers: i32,
}

pub fn predicts_relevant(score: i32, blockers: i32, threshold: i32) -> bool {
    score >= threshold && blockers == 0
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Metrics {
    /// Labelled jobs this model has scored.
    pub scored: usize,
    pub true_positive: usize,
    pub false_positive: usize,
    pub false_negative: usize,
    pub true_negative: usize,
    /// Share of apply/no jobs where the model agrees with you. None with no such jobs.
    pub agreement: Option<f64>,
    /// Of the jobs the model calls relevant, the share you'd apply to.
    pub precision: Option<f64>,
    /// Of the jobs you'd apply to, the share the model calls relevant.
    pub recall: Option<f64>,
    pub mean_score_apply: Option<f64>,
    pub mean_score_maybe: Option<f64>,
    pub mean_score_no: Option<f64>,
}

pub fn compute(rows: &[Scored], threshold: i32) -> Metrics {
    let mut m = Metrics {
        scored: rows.len(),
        ..Metrics::default()
    };
    for row in rows {
        let predicted = predicts_relevant(row.score, row.blockers, threshold);
        match (row.label, predicted) {
            (Label::Apply, true) => m.true_positive += 1,
            (Label::Apply, false) => m.false_negative += 1,
            (Label::No, true) => m.false_positive += 1,
            (Label::No, false) => m.true_negative += 1,
            (Label::Maybe, _) => {}
        }
    }

    let (tp, fp, fn_, tn) = (
        m.true_positive,
        m.false_positive,
        m.false_negative,
        m.true_negative,
    );
    m.agreement = ratio(tp + tn, tp + fp + fn_ + tn);
    m.precision = ratio(tp, tp + fp);
    m.recall = ratio(tp, tp + fn_);
    m.mean_score_apply = mean(rows, Label::Apply);
    m.mean_score_maybe = mean(rows, Label::Maybe);
    m.mean_score_no = mean(rows, Label::No);
    m
}

fn ratio(part: usize, whole: usize) -> Option<f64> {
    (whole > 0).then(|| part as f64 / whole as f64)
}

fn mean(rows: &[Scored], label: Label) -> Option<f64> {
    let scores: Vec<i32> = rows
        .iter()
        .filter(|r| r.label == label)
        .map(|r| r.score)
        .collect();
    (!scores.is_empty()).then(|| scores.iter().sum::<i32>() as f64 / scores.len() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(label: Label, score: i32, blockers: i32) -> Scored {
        Scored {
            label,
            score,
            blockers,
        }
    }

    #[test]
    fn counts_and_ratios() {
        let rows = vec![
            row(Label::Apply, 80, 0), // TP
            row(Label::Apply, 40, 0), // FN: score too low
            row(Label::Apply, 90, 1), // FN: blocker
            row(Label::No, 70, 0),    // FP
            row(Label::No, 20, 0),    // TN
            row(Label::Maybe, 65, 0), // ignored for ratios
        ];
        let m = compute(&rows, 60);
        assert_eq!(m.scored, 6);
        assert_eq!(
            (
                m.true_positive,
                m.false_negative,
                m.false_positive,
                m.true_negative
            ),
            (1, 2, 1, 1)
        );
        assert_eq!(m.agreement, Some(2.0 / 5.0));
        assert_eq!(m.precision, Some(0.5));
        assert_eq!(m.recall, Some(1.0 / 3.0));
        assert_eq!(m.mean_score_apply, Some(70.0));
        assert_eq!(m.mean_score_maybe, Some(65.0));
        assert_eq!(m.mean_score_no, Some(45.0));
    }

    #[test]
    fn empty_input_has_no_ratios() {
        let m = compute(&[], 60);
        assert_eq!(m.agreement, None);
        assert_eq!(m.precision, None);
        assert_eq!(m.mean_score_apply, None);
    }

    #[test]
    fn threshold_is_inclusive() {
        assert!(predicts_relevant(60, 0, 60));
        assert!(!predicts_relevant(59, 0, 60));
        assert!(!predicts_relevant(100, 2, 60));
    }
}
