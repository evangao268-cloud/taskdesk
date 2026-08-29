//! Pure nudge scheduling logic. No I/O: callers load definitions and acks
//! from the store and pass a concrete `today`.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NudgeDef {
    pub id: String,
    pub title: String,
    pub interval_days: u32,
    /// First date the nudge becomes due, before any acks.
    pub anchor_date: NaiveDate,
    pub create_task_on_ack: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct NudgeAck {
    pub nudge_id: String,
    pub acked_on: NaiveDate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DueNudge {
    pub id: String,
    pub title: String,
    pub interval_days: u32,
    pub days_overdue: i64,
    pub create_task_on_ack: bool,
}

/// A nudge is due when `next_due <= today`, where `next_due` is the last ack
/// plus the interval, or the anchor date if never acked. Overdue nudges
/// surface as one prompt regardless of how many cycles were missed.
pub fn due_nudges(defs: &[NudgeDef], acks: &[NudgeAck], today: NaiveDate) -> Vec<DueNudge> {
    defs.iter()
        .filter(|d| d.enabled)
        .filter_map(|d| {
            let last_ack = acks
                .iter()
                .filter(|a| a.nudge_id == d.id)
                .map(|a| a.acked_on)
                .max();
            let next_due = match last_ack {
                Some(acked) => acked + chrono::Days::new(u64::from(d.interval_days)),
                None => d.anchor_date,
            };
            (next_due <= today).then(|| DueNudge {
                id: d.id.clone(),
                title: d.title.clone(),
                interval_days: d.interval_days,
                days_overdue: (today - next_due).num_days(),
                create_task_on_ack: d.create_task_on_ack,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    fn def(id: &str, interval: u32, anchor: &str) -> NudgeDef {
        NudgeDef {
            id: id.into(),
            title: format!("nudge {id}"),
            interval_days: interval,
            anchor_date: date(anchor),
            create_task_on_ack: false,
            enabled: true,
        }
    }

    fn ack(id: &str, on: &str) -> NudgeAck {
        NudgeAck {
            nudge_id: id.into(),
            acked_on: date(on),
        }
    }

    #[test]
    fn never_acked_due_from_anchor() {
        let defs = [def("a", 14, "2026-08-29")];
        assert_eq!(due_nudges(&defs, &[], date("2026-08-28")).len(), 0);
        let due = due_nudges(&defs, &[], date("2026-08-29"));
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].days_overdue, 0);
    }

    #[test]
    fn interval_boundary() {
        let defs = [def("a", 14, "2026-01-01")];
        let acks = [ack("a", "2026-08-01")];
        // Day 13 after ack: not due. Day 14: due.
        assert_eq!(due_nudges(&defs, &acks, date("2026-08-14")).len(), 0);
        assert_eq!(due_nudges(&defs, &acks, date("2026-08-15")).len(), 1);
    }

    #[test]
    fn latest_ack_wins() {
        let defs = [def("a", 7, "2026-01-01")];
        let acks = [ack("a", "2026-08-01"), ack("a", "2026-08-20")];
        assert_eq!(due_nudges(&defs, &acks, date("2026-08-26")).len(), 0);
        assert_eq!(due_nudges(&defs, &acks, date("2026-08-27")).len(), 1);
    }

    #[test]
    fn disabled_never_due() {
        let mut d = def("a", 1, "2026-01-01");
        d.enabled = false;
        assert_eq!(due_nudges(&[d], &[], date("2026-08-29")).len(), 0);
    }

    #[test]
    fn overdue_reports_days_but_single_prompt() {
        let defs = [def("a", 7, "2026-01-01")];
        let acks = [ack("a", "2026-08-01")];
        let due = due_nudges(&defs, &acks, date("2026-08-29"));
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].days_overdue, 21);
    }

    #[test]
    fn acks_for_other_nudges_ignored() {
        let defs = [def("a", 14, "2026-08-01")];
        let acks = [ack("b", "2026-08-28")];
        assert_eq!(due_nudges(&defs, &acks, date("2026-08-29")).len(), 1);
    }
}
