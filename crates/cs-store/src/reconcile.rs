use cs_core::{EventEnvelope, Measurement, Payload, SessionStatus, TaskOutcome, TaskReport};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum CallState {
    #[default]
    Incomplete,
    Streaming,
    Completed,
    Failed,
    Cancelled,
    Unknown,
}
impl CallState {
    pub fn observe(&mut self, next: Self) {
        use CallState::*;
        *self = match (*self, next) {
            (Unknown, _) => Unknown,
            (Incomplete | Streaming, next) => next,
            (old, Incomplete | Streaming) => old,
            (old, next) if old == next => old,
            _ => Unknown,
        };
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Incomplete => "incomplete",
            Self::Streaming => "streaming",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Unknown => "unknown",
        }
    }
    pub fn incomplete(self) -> bool {
        matches!(self, Self::Incomplete | Self::Streaming | Self::Unknown)
    }
}

pub(crate) fn session(events: &[EventEnvelope]) -> (SessionStatus, Option<i64>, Option<i64>) {
    type Stream<'a> = (&'a str, &'a str);
    let unknown = || (SessionStatus::Unknown, None, None);
    let mut epochs: BTreeMap<Stream<'_>, Vec<&EventEnvelope>> = BTreeMap::new();
    for e in events {
        epochs
            .entry((&e.producer_id, &e.producer_epoch))
            .or_default()
            .push(e);
    }
    if epochs.is_empty() {
        return (SessionStatus::Incomplete, None, None);
    }
    if epochs
        .keys()
        .map(|key| key.0)
        .collect::<BTreeSet<_>>()
        .len()
        != 1
    {
        return unknown();
    }
    let producer = epochs.keys().next().unwrap().0;
    let mut successors = BTreeMap::new();
    let mut roots = Vec::new();
    for (key, markers) in &epochs {
        let opens = markers
            .iter()
            .filter(|e| {
                matches!(
                    e.payload,
                    Payload::SessionStarted { .. } | Payload::SessionResumed { .. }
                )
            })
            .collect::<Vec<_>>();
        let ends = markers
            .iter()
            .filter(|e| matches!(e.payload, Payload::SessionEnded { .. }))
            .collect::<Vec<_>>();
        if opens.len() > 1 || ends.len() > 1 {
            return unknown();
        }
        if let (Some(open), Some(end)) = (opens.first(), ends.first()) {
            if !matches!((open.producer_sequence, end.producer_sequence), (Some(a),Some(b)) if a < b)
            {
                return unknown();
            }
        }
        if let Some(open) = opens.first() {
            if let Payload::SessionResumed { previous_epoch } = &open.payload {
                let parent = (producer, previous_epoch.as_str());
                if !epochs.contains_key(&parent) || successors.insert(parent, *key).is_some() {
                    return unknown();
                }
                continue;
            }
        }
        roots.push(*key);
    }
    if roots.len() != 1 {
        return unknown();
    }
    let mut current = roots[0];
    let mut visited = BTreeSet::new();
    while visited.insert(current) {
        match successors.get(&current) {
            Some(next) => current = *next,
            None => break,
        }
    }
    if visited.len() != epochs.len() {
        return unknown();
    }
    let started_at = epochs[&roots[0]]
        .iter()
        .find(|e| matches!(e.payload, Payload::SessionStarted { .. }))
        .map(|e| e.observed_at_unix_ms);
    let ended_at = epochs[&current]
        .iter()
        .find(|e| matches!(e.payload, Payload::SessionEnded { .. }))
        .map(|e| e.observed_at_unix_ms);
    (
        if ended_at.is_some() {
            SessionStatus::Ended
        } else {
            SessionStatus::Incomplete
        },
        started_at,
        ended_at,
    )
}

#[derive(Default)]
pub(crate) struct Task {
    starts: Vec<i64>,
    outcomes: Vec<(TaskOutcome, Option<String>, cs_core::EvidenceKind, i64)>,
}
impl Task {
    pub fn observe(&mut self, e: &EventEnvelope) {
        match &e.payload {
            Payload::TaskStarted { .. } => self.starts.push(e.observed_at_unix_ms),
            Payload::TaskOutcomeRecorded {
                outcome,
                oracle_version,
                evidence_kind,
                ..
            } => self.outcomes.push((
                *outcome,
                oracle_version.clone(),
                *evidence_kind,
                e.observed_at_unix_ms,
            )),
            _ => {}
        }
    }
    pub fn report(&self, task_id: String) -> TaskReport {
        let ambiguous = self.starts.len() > 1 || self.outcomes.len() > 1;
        let started_at_unix_ms = if self.starts.len() == 1 {
            self.starts.first().copied()
        } else {
            None
        };
        let outcome = if ambiguous {
            None
        } else {
            self.outcomes.first()
        };
        TaskReport {
            task_id,
            outcome: match outcome {
                Some((value, _, _, _)) => Measurement::Reported {
                    value: *value,
                    source: "synthetic task outcome event; provenance retained in evidence_kind"
                        .into(),
                },
                None => Measurement::Unavailable {
                    reason: if ambiguous {
                        "ambiguous task lifecycle"
                    } else {
                        "no task outcome event"
                    }
                    .into(),
                },
            },
            started_at_unix_ms,
            ended_at_unix_ms: outcome.map(|e| e.3),
            oracle_version: outcome.and_then(|e| e.1.clone()),
            evidence_kind: outcome.map(|e| e.2),
        }
    }
}
