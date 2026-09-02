use crate::model::DEFAULT_FOLDER_PATTERN;
use crate::naming::DEFAULT_NAME_PATTERN;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preset {
    pub name: &'static str,
    pub folder: &'static str,
    pub file: &'static str,
    pub about: &'static str,
}

pub const PRESETS: [Preset; 6] = [
    Preset {
        name: "plain",
        folder: DEFAULT_FOLDER_PATTERN,
        file: DEFAULT_NAME_PATTERN,
        about: "A year folder holding month folders. What eonsort does when nobody says otherwise.",
    },
    Preset {
        name: "day",
        folder: "%Y/%Y-%m-%d",
        file: DEFAULT_NAME_PATTERN,
        about: "A folder per day, named so it sorts the way it reads.",
    },
    Preset {
        name: "place",
        folder: "%Y/%Y-%m-%d/{city|region|country|\"unknown place\"}",
        file: DEFAULT_NAME_PATTERN,
        about: "A folder per day, split by where the pictures were taken. Needs a gazetteer.",
    },
    Preset {
        name: "immich",
        folder: "%Y/%Y-%m-%d",
        file: "%Y%m%d-%H%M%S-{original_stem}",
        about: "Day folders with the date in front of every name, which is what an immich import expects to walk.",
    },
    Preset {
        name: "photoprism",
        folder: "%Y/%m",
        file: DEFAULT_NAME_PATTERN,
        about: "The originals layout PhotoPrism indexes without being told anything.",
    },
    Preset {
        name: "elodie",
        folder: "%Y-%m-%b/{city|country|\"Unknown Location\"}",
        file: "%Y-%m-%d_%H-%M-%S-{original_stem}",
        about: "The tree elodie builds, for anyone moving an archive across without reshuffling it.",
    },
];

pub fn by_name(name: &str) -> Option<Preset> {
    let wanted = name.trim().to_ascii_lowercase();
    PRESETS.into_iter().find(|preset| preset.name == wanted)
}

pub fn names() -> Vec<&'static str> {
    PRESETS.iter().map(|preset| preset.name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{validate_folder_pattern, validate_name_pattern};
    use crate::naming::{self, Token};

    #[test]
    fn every_preset_is_a_pattern_eonsort_will_accept() {
        for preset in PRESETS {
            assert!(
                validate_folder_pattern(preset.folder).is_ok(),
                "{}: {}",
                preset.name,
                preset.folder
            );
            assert!(
                validate_name_pattern(preset.file).is_ok(),
                "{}: {}",
                preset.name,
                preset.file
            );
        }
    }

    #[test]
    fn a_preset_can_be_looked_up_by_name_whatever_the_case() {
        assert_eq!(by_name("immich").unwrap().name, "immich");
        assert_eq!(by_name("IMMICH").unwrap().name, "immich");
        assert_eq!(by_name("  elodie  ").unwrap().name, "elodie");
        assert!(by_name("nothing of the sort").is_none());
    }

    #[test]
    fn every_preset_has_a_name_of_its_own() {
        let mut seen = names();
        let before = seen.len();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), before);
    }

    #[test]
    fn every_preset_says_what_it_is_for() {
        for preset in PRESETS {
            assert!(!preset.about.is_empty(), "{}", preset.name);
            assert!(preset.about.len() > 20, "{}", preset.name);
        }
    }

    #[test]
    fn the_plain_preset_is_what_eonsort_already_did() {
        let plain = by_name("plain").unwrap();
        assert_eq!(plain.folder, DEFAULT_FOLDER_PATTERN);
        assert_eq!(plain.file, DEFAULT_NAME_PATTERN);
    }

    #[test]
    fn the_presets_that_need_a_place_say_so_in_their_pattern() {
        assert!(naming::needs(by_name("place").unwrap().folder, Token::City));
        assert!(naming::needs(
            by_name("elodie").unwrap().folder,
            Token::City
        ));
        assert!(!naming::needs(
            by_name("plain").unwrap().folder,
            Token::City
        ));
    }

    #[test]
    fn a_place_preset_still_builds_a_folder_when_nothing_is_known() {
        for name in ["place", "elodie"] {
            let preset = by_name(name).unwrap();
            let resolved = naming::resolve(preset.folder, &naming::Facts::default()).unwrap();
            assert!(!resolved.contains("{"), "{name}: {resolved}");
            assert!(!resolved.ends_with('/'), "{name}: {resolved}");
        }
    }

    #[test]
    fn the_import_presets_put_the_date_at_the_front_of_the_name() {
        for name in ["immich", "elodie"] {
            let preset = by_name(name).unwrap();
            assert!(preset.file.starts_with('%'), "{name}: {}", preset.file);
            assert!(naming::needs(preset.file, Token::OriginalStem), "{name}");
        }
    }
}
