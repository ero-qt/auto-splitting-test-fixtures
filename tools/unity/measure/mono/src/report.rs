//! What a run produces. One entry per member, grouped by the structure it
//! belongs to.

use std::collections::BTreeMap;

use serde::Serialize;

/// The measurements, by structure and then by member.
pub type Report = BTreeMap<&'static str, BTreeMap<&'static str, Measurement>>;

/// One measured member: a single agreed offset, or the ambiguity spelled out.
#[derive(Serialize)]
#[serde(untagged)]
pub enum Measurement {
    Single(u32),
    Ambiguous { candidates: Vec<u32> },
    Missing { missing: String },
}

impl Measurement {
    /// The offset when exactly one candidate survived, and the reason there
    /// is none otherwise.
    pub fn of(candidates: Vec<u32>, missing: &str) -> Self {
        match candidates.as_slice() {
            [only] => Self::Single(*only),
            [] => Self::Missing {
                missing: missing.into(),
            },
            _ => Self::Ambiguous { candidates },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(measurement: Measurement) -> String {
        serde_json::to_string(&measurement).expect("rendering one measurement")
    }

    #[test]
    fn one_surviving_candidate_is_the_offset() {
        assert_eq!(rendered(Measurement::of(vec![0x40], "not found")), "64");
    }

    // Nothing found and too much found are different answers, and neither is
    // an offset, so a reader can never mistake one for a measurement.
    #[test]
    fn no_candidate_and_several_read_apart() {
        assert_eq!(
            rendered(Measurement::of(Vec::new(), "kind bits not found")),
            r#"{"missing":"kind bits not found"}"#,
        );
        assert_eq!(
            rendered(Measurement::of(vec![0x10, 0x18], "not found")),
            r#"{"candidates":[16,24]}"#,
        );
    }
}
