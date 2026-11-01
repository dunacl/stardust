//! The interface in English, Spanish, French and German.
//!
//! English text is the key: [`tr`] looks a string up in the table in
//! `strings.rs` and gives the English back if it has no row, so a missing
//! translation shows English and never nothing. The tests below read
//! `ui.rs` and `cards.rs` and fail on a key without a row.
//!
//! The language is a process-wide setting rather than an argument, because
//! nearly every function in the UI writes text. It belongs to the UI alone:
//! simulation crates stay in English, numbers keep a decimal point in every
//! language, and body names are never translated. What the model writes
//! (its warnings) is matched against templates by [`tr_message`].

use crate::strings::{MESSAGES, ROWS};
use std::collections::BTreeMap;
use std::fmt::Display;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    En = 0,
    Es = 1,
    Fr = 2,
    De = 3,
}

impl Language {
    pub const ALL: [Language; 4] = [Language::En, Language::Es, Language::Fr, Language::De];

    /// Its name in its own language.
    pub fn name(self) -> &'static str {
        match self {
            Language::En => "English",
            Language::Es => "Español",
            Language::Fr => "Français",
            Language::De => "Deutsch",
        }
    }

    /// From an ISO 639-1 code, as in `?lang=es`.
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "en" => Some(Language::En),
            "es" => Some(Language::Es),
            "fr" => Some(Language::Fr),
            "de" => Some(Language::De),
            _ => None,
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn language() -> Language {
    Language::ALL[usize::from(CURRENT.load(Ordering::Relaxed))]
}

pub fn set_language(language: Language) {
    CURRENT.store(language as u8, Ordering::Relaxed);
}

type Row = [&'static str; 4];

fn table() -> &'static BTreeMap<&'static str, &'static Row> {
    static TABLE: OnceLock<BTreeMap<&'static str, &'static Row>> = OnceLock::new();
    TABLE.get_or_init(|| {
        ROWS.iter()
            .chain(MESSAGES)
            .map(|row| (row[0], row))
            .collect()
    })
}

/// `key` in `language`, or `key` itself if the table has no row.
fn lookup(language: Language, key: &str) -> &str {
    table().get(key).map_or(key, |row| row[language as usize])
}

/// Whether the table has a row for `key`.
#[cfg(test)]
pub fn has(key: &str) -> bool {
    table().contains_key(key)
}

/// `key` in the current language.
pub fn tr(key: &str) -> &str {
    lookup(language(), key)
}

/// Replaces each `{}` of `template`, in order.
fn fill(template: &str, values: &[&dyn Display]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut parts = template.split("{}");
    out.push_str(parts.next().unwrap_or_default());
    for (index, part) in parts.enumerate() {
        if let Some(value) = values.get(index) {
            out.push_str(&value.to_string());
        }
        out.push_str(part);
    }
    out
}

/// `key`, a template with `{}` holes, in the current language.
pub fn trf(key: &str, values: &[&dyn Display]) -> String {
    fill(tr(key), values)
}

/// What fills the holes of `template` in `text`, if `text` has its form.
fn holes<'a>(template: &str, text: &'a str) -> Option<Vec<&'a str>> {
    let parts: Vec<&str> = template.split("{}").collect();
    let mut rest = text.strip_prefix(parts[0])?;
    let mut found = Vec::new();
    for (index, part) in parts.iter().enumerate().skip(1) {
        let end = if index + 1 == parts.len() && !part.is_empty() {
            rest.len().checked_sub(part.len())?
        } else {
            rest.find(part)?
        };
        let end = if part.is_empty() && index + 1 == parts.len() {
            rest.len()
        } else {
            end
        };
        found.push(&rest[..end]);
        rest = rest[end..].strip_prefix(part)?;
    }
    rest.is_empty().then_some(found)
}

/// A sentence the model wrote, in `language`; unchanged if no template
/// fits, so a new warning shows in English until it gets a row.
fn message(language: Language, text: &str) -> String {
    for row in MESSAGES {
        if let Some(found) = holes(row[0], text) {
            let values: Vec<&dyn Display> = found.iter().map(|hole| hole as &dyn Display).collect();
            return fill(row[language as usize], &values);
        }
    }
    text.to_string()
}

/// A sentence the model wrote, in the current language.
pub fn tr_message(text: &str) -> String {
    message(language(), text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::help;
    use sg_render::{Quality, ScaleMode};
    use sg_schema::{Fidelity, SimulationParams, SystemSpec};
    use std::collections::BTreeSet;

    /// The string literals passed to `tr(` and `trf(` in a source file.
    fn keys_in(source: &str) -> Vec<String> {
        let mut keys = Vec::new();
        for call in ["tr(", "trf("] {
            for (start, _) in source.match_indices(call) {
                let word = source[..start].chars().last();
                if word.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                // rustfmt may put a long literal on the next line.
                let after = source[start + call.len()..].trim_start();
                let Some(literal) = after.strip_prefix('"') else {
                    continue;
                };
                let mut key = String::new();
                let mut chars = literal.chars().peekable();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            // A line continuation drops what indents the
                            // next line.
                            Some('\n') => {
                                while chars.peek().is_some_and(|c| c.is_whitespace()) {
                                    chars.next();
                                }
                            }
                            Some(other) => key.push(other),
                            None => {}
                        },
                        other => key.push(other),
                    }
                }
                keys.push(key);
            }
        }
        keys
    }

    #[test]
    fn every_key_in_the_source_has_a_row() {
        let mut keys = keys_in(include_str!("ui.rs"));
        keys.extend(keys_in(include_str!("cards.rs")));
        keys.extend(keys_in(include_str!("help.rs")));
        assert!(keys.len() > 80, "the scan found only {}", keys.len());
        keys.retain(|key| !key.contains("no such"));
        let missing: BTreeSet<_> = keys
            .iter()
            .filter(|key| !table().contains_key(key.as_str()))
            .collect();
        assert!(missing.is_empty(), "no row for {missing:#?}");
    }

    #[test]
    fn everything_help_lists_has_a_row() {
        let mut missing = BTreeSet::new();
        for topic in help::TOPICS {
            missing.insert(topic.title);
            for item in topic.items {
                missing.insert(item.name);
                missing.insert(item.text);
            }
        }
        missing.retain(|key| !table().contains_key(key));
        assert!(missing.is_empty(), "no row for {missing:#?}");
    }

    #[test]
    fn labels_from_other_crates_have_rows() {
        let labels = ScaleMode::ALL
            .map(ScaleMode::label)
            .into_iter()
            .chain(Quality::ALL.map(Quality::label));
        for label in labels {
            assert!(table().contains_key(label), "{label}");
        }
        for preset in sg_sim::ALL_PRESETS {
            for fidelity in [Fidelity::Standard, Fidelity::Classic, Fidelity::High] {
                if let sg_sim::PresetAvailability::Unavailable(reason) =
                    sg_sim::preset_availability(preset, fidelity)
                {
                    assert!(table().contains_key(reason), "{reason}");
                }
            }
        }
    }

    #[test]
    fn every_row_is_filled_and_keeps_its_holes() {
        for row in ROWS.iter().chain(MESSAGES) {
            for text in row {
                assert!(!text.trim().is_empty(), "{row:?}");
                assert_eq!(
                    text.matches("{}").count(),
                    row[0].matches("{}").count(),
                    "{text}"
                );
            }
        }
    }

    #[test]
    fn no_key_is_listed_twice() {
        let mut seen = BTreeSet::new();
        for row in ROWS.iter().chain(MESSAGES) {
            assert!(seen.insert(row[0]), "twice: {}", row[0]);
        }
    }

    #[test]
    fn an_unknown_key_falls_back_to_english() {
        assert_eq!(lookup(Language::De, "no such key"), "no such key");
        assert_eq!(lookup(Language::Es, "Simulate"), "Simular");
    }

    #[test]
    fn holes_are_filled_in_order() {
        assert_eq!(fill("{} of {}", &[&1, &"two"]), "1 of two");
        assert_eq!(fill("none", &[]), "none");
    }

    #[test]
    fn templates_recover_their_holes() {
        assert_eq!(holes("a {} b {} c", "a 1 b 2 c"), Some(vec!["1", "2"]));
        assert_eq!(holes("{}: x", "name: x"), Some(vec!["name"]));
        assert_eq!(holes("x {}", "x 1, 2"), Some(vec!["1, 2"]));
        assert_eq!(holes("a {} b", "a 1 c"), None);
    }

    #[test]
    fn a_warning_from_the_model_is_translated() {
        let result = sg_sim::simulate(&SimulationParams {
            seed: 1,
            fidelity: Fidelity::Classic,
            system: SystemSpec::Auto {
                primary_mass_msun: Some(2.0),
                multiplicity: None,
            },
            ..SimulationParams::default()
        })
        .unwrap();
        let warning = result
            .diagnostics
            .warnings
            .iter()
            .find(|warning| warning.contains("Fogg"))
            .expect("a star outside Fogg's range");
        let spanish = message(Language::Es, warning);
        assert_ne!(&spanish, warning);
        assert!(spanish.contains("2.00"), "{spanish}");
        assert_eq!(&message(Language::En, warning), warning);
    }

    #[test]
    fn a_sentence_with_no_template_stays_as_it_is() {
        assert_eq!(message(Language::Fr, "something new"), "something new");
    }
}
