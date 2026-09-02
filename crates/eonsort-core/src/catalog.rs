use crate::weights::Weight;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Upright,
}

impl Kind {
    pub const ALL: [Kind; 1] = [Kind::Upright];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Upright => "upright",
        }
    }
}

pub struct Variant {
    pub id: &'static str,
    pub kind: Kind,
    pub label: &'static str,
    pub note: &'static str,
    pub weights: &'static [Weight],
}

const REPO: &str = "lmz/candle-yolo-v8";
const REVISION: &str = "be388c6fab95ae3035a039070e1b883b9c5a1325";

const UPRIGHT_N: [Weight; 1] = [Weight {
    repo: REPO,
    revision: REVISION,
    file: "yolov8n.safetensors",
    bytes: 6_369_332,
}];

const UPRIGHT_S: [Weight; 1] = [Weight {
    repo: REPO,
    revision: REVISION,
    file: "yolov8s.safetensors",
    bytes: 22_407_580,
}];

const UPRIGHT_M: [Weight; 1] = [Weight {
    repo: REPO,
    revision: REVISION,
    file: "yolov8m.safetensors",
    bytes: 51_918_852,
}];

const UPRIGHT_L: [Weight; 1] = [Weight {
    repo: REPO,
    revision: REVISION,
    file: "yolov8l.safetensors",
    bytes: 87_536_380,
}];

const UPRIGHT_X: [Weight; 1] = [Weight {
    repo: REPO,
    revision: REVISION,
    file: "yolov8x.safetensors",
    bytes: 136_636_180,
}];

pub const VARIANTS: [Variant; 5] = [
    Variant {
        id: "yolov8n",
        kind: Kind::Upright,
        label: "Smallest",
        note: "Quick enough for a whole library on any machine",
        weights: &UPRIGHT_N,
    },
    Variant {
        id: "yolov8s",
        kind: Kind::Upright,
        label: "Small",
        note: "A little slower, surer about what it sees",
        weights: &UPRIGHT_S,
    },
    Variant {
        id: "yolov8m",
        kind: Kind::Upright,
        label: "Middling",
        note: "The even trade between the time it takes and what it finds",
        weights: &UPRIGHT_M,
    },
    Variant {
        id: "yolov8l",
        kind: Kind::Upright,
        label: "Large",
        note: "Finds the awkward ones, and takes its time over the rest",
        weights: &UPRIGHT_L,
    },
    Variant {
        id: "yolov8x",
        kind: Kind::Upright,
        label: "Largest",
        note: "Slowest by far, for an archive you only turn once",
        weights: &UPRIGHT_X,
    },
];

impl Variant {
    pub fn stamp(&self) -> String {
        let revision = self
            .weights
            .first()
            .map(|w| &w.revision[..12.min(w.revision.len())])
            .unwrap_or("none");
        format!("{}@{revision}", self.id)
    }
}

pub fn of(kind: Kind) -> impl Iterator<Item = &'static Variant> {
    VARIANTS.iter().filter(move |v| v.kind == kind)
}

pub fn find(id: &str) -> Option<&'static Variant> {
    VARIANTS.iter().find(|v| v.id == id)
}

pub fn default_of(kind: Kind) -> &'static Variant {
    of(kind).next().expect("every kind has a variant")
}

pub fn chosen(kind: Kind, id: Option<&str>) -> &'static Variant {
    id.and_then(find)
        .filter(|v| v.kind == kind)
        .unwrap_or_else(|| default_of(kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_offers_at_least_one_variant() {
        for kind in Kind::ALL {
            assert!(of(kind).next().is_some(), "{} has none", kind.label());
        }
    }

    #[test]
    fn no_two_variants_share_a_name() {
        let mut seen: Vec<&str> = VARIANTS.iter().map(|v| v.id).collect();
        seen.sort_unstable();
        let held = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), held);
    }

    #[test]
    fn every_variant_names_a_file_and_a_length() {
        for variant in &VARIANTS {
            assert!(!variant.weights.is_empty(), "{} has no weights", variant.id);
            for weight in variant.weights {
                assert!(!weight.file.is_empty());
                assert!(weight.bytes > 0, "{} has no length", weight.file);
                assert_eq!(weight.revision.len(), 40, "{} is not pinned", weight.file);
            }
        }
    }

    #[test]
    fn falls_back_to_the_smallest_when_the_choice_is_unknown() {
        assert_eq!(chosen(Kind::Upright, None).id, "yolov8n");
        assert_eq!(chosen(Kind::Upright, Some("nonsense")).id, "yolov8n");
        assert_eq!(chosen(Kind::Upright, Some("yolov8m")).id, "yolov8m");
    }

    #[test]
    fn a_variant_of_another_kind_is_not_taken() {
        let other = VARIANTS.iter().find(|v| v.kind != Kind::Upright);
        if let Some(other) = other {
            assert_eq!(
                chosen(Kind::Upright, Some(other.id)).id,
                default_of(Kind::Upright).id
            );
        }
    }

    #[test]
    fn every_variant_stamps_itself_differently() {
        let mut stamps: Vec<String> = VARIANTS.iter().map(|v| v.stamp()).collect();
        let held = stamps.len();
        stamps.sort();
        stamps.dedup();
        assert_eq!(stamps.len(), held, "two variants stamp the same");
    }

    #[test]
    fn a_stamp_names_the_model_and_the_revision_it_came_from() {
        let stamp = find("yolov8m").unwrap().stamp();
        assert!(stamp.starts_with("yolov8m@"), "{stamp}");
        assert!(stamp.contains("be388c6fab95"), "{stamp}");
    }

    #[test]
    fn the_variants_of_a_kind_grow_in_size() {
        let sizes: Vec<u64> = of(Kind::Upright)
            .map(|v| crate::weights::total_bytes(v.weights))
            .collect();
        let mut sorted = sizes.clone();
        sorted.sort_unstable();
        assert_eq!(sizes, sorted, "variants should be listed smallest first");
    }
}
