//! Names for generated stars and planets.
//!
//! Names are labels, not physics: nothing else in the simulation reads them.
//! They are still part of the result, so they follow the determinism rules
//! (own ChaCha stream, fixed tables) and the same seed always gives the same
//! names.
//!
//! - A star gets an invented, pronounceable proper name built from fixed
//!   syllable tables.
//! - A planet is designated the way exoplanets are: the star's name followed
//!   by a lower-case letter starting at `b` (the star itself is `a`). The IAU
//!   convention letters planets in order of discovery; a generated system
//!   has no discovery order, so letters run outward from the star.
//! - Planets and moons also get a proper name from the same tables, and belts
//!   a name from their place. These are given last, by [`assign`], from their
//!   own stream, so no tier's draws depend on them.

use sg_core::rng::{ModuleId, RngStream};
use sg_schema::{HostRef, SystemSnapshot};
use std::collections::BTreeSet;

const OPENINGS: &[&str] = &[
    "Al", "Am", "Ar", "Bel", "Bry", "Cal", "Cor", "Dar", "Del", "Dra", "El", "Er", "Fen", "Gal",
    "Har", "Hel", "Il", "Ir", "Jor", "Kal", "Kes", "Lir", "Lor", "Mar", "Mel", "Nar", "Ner", "Or",
    "Os", "Pel", "Quel", "Ral", "Ren", "Sar", "Sel", "Tal", "Tor", "Ul", "Val", "Vor", "Wen",
    "Xan", "Yar", "Zel", "Zor",
];

const MIDDLES: &[&str] = &[
    "a", "an", "ar", "e", "en", "er", "i", "il", "in", "o", "on", "or", "u", "un", "ya",
];

const ENDINGS: &[&str] = &[
    "a", "ae", "an", "ara", "as", "ea", "el", "en", "era", "eth", "ia", "ine", "ion", "ira", "is",
    "ith", "on", "ona", "or", "os", "ra", "ris", "un", "ura", "us", "ys",
];

/// Proper names of real stars that the tables can produce, lower-case. A
/// generated system is not a real one, so these are redrawn.
const REAL_STAR_NAMES: &[&str] = &[
    "alya", "aran", "elnath", "heze", "izar", "maia", "merak", "mira", "mizar", "nashira", "ran",
    "salm", "saiph", "sarin", "talitha", "taras", "torcular",
];

fn pick<'a>(rng: &mut RngStream, table: &[&'a str]) -> &'a str {
    // The tables are far smaller than 2^53, so the index is always in range.
    let index = (rng.uniform() * table.len() as f64) as usize;
    table[index.min(table.len() - 1)]
}

fn candidate(rng: &mut RngStream) -> String {
    let mut name = String::from(pick(rng, OPENINGS));
    // Two names in five get a middle syllable.
    if rng.uniform() < 0.4 {
        name.push_str(pick(rng, MIDDLES));
    }
    name.push_str(pick(rng, ENDINGS));
    name
}

/// True if `name` reads acceptably. The tables can produce awkward joins
/// between syllables; this rejects a doubled vowel, three vowels running, a
/// `y` straight after a vowel, a letter three times running and an
/// immediately repeated pair or triple of letters (`Ililris`, `Erorara`).
fn reads_well(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let b = lower.as_bytes();
    let vowel = |c: u8| matches!(c, b'a' | b'e' | b'i' | b'o' | b'u' | b'y');
    let pairs_ok = b
        .windows(2)
        .all(|w| !(vowel(w[0]) && (w[0] == w[1] || w[1] == b'y')));
    let triples_ok = b
        .windows(3)
        .all(|w| !(w[0] == w[1] && w[1] == w[2]) && !(vowel(w[0]) && vowel(w[1]) && vowel(w[2])));
    let no_echo = |n: usize| b.windows(2 * n).all(|w| w[..n] != w[n..]);
    pairs_ok && triples_ok && no_echo(2) && no_echo(3)
}

/// The star's name for a run seed: 3 to 10 ASCII letters, capitalised.
pub fn star_name(seed: u64) -> String {
    let mut rng = RngStream::new(seed, ModuleId::NAMES);
    loop {
        let name = candidate(&mut rng);
        let real = REAL_STAR_NAMES.contains(&name.to_ascii_lowercase().as_str());
        if reads_well(&name) && !real {
            return name;
        }
    }
}

/// Designation of the planet at `index` counting outward from 0: `Name b`,
/// `Name c`, … `Name z`, then `Name 26`, `Name 27`, … by ordinal.
pub fn planet_name(star_name: &str, index: usize) -> String {
    match u8::try_from(index) {
        Ok(i) if i < 25 => format!("{star_name} {}", char::from(b'b' + i)),
        _ => format!("{star_name} {}", index + 1),
    }
}

/// Designation of a planet's moon at `index` counting outward from 0:
/// the planet's name and a Roman numeral, as for the moons of the Solar
/// System's planets (`Jupiter I` is Io).
pub fn moon_name(planet_name: &str, index: usize) -> String {
    const NUMERALS: [&str; 10] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"];
    match NUMERALS.get(index) {
        Some(numeral) => format!("{planet_name} {numeral}"),
        None => format!("{planet_name} moon {}", index + 1),
    }
}

/// A new proper name, not in `taken`, which it joins.
pub(crate) fn unique(rng: &mut RngStream, taken: &mut BTreeSet<String>) -> String {
    loop {
        let name = candidate(rng);
        let real = REAL_STAR_NAMES.contains(&name.to_ascii_lowercase().as_str());
        if reads_well(&name) && !real && taken.insert(name.clone()) {
            return name;
        }
    }
}

/// Gives every planet and moon a proper name, unique in its system, keeping
/// the name it was built with as its designation, and names the belts.
pub fn assign(snapshot: &mut SystemSnapshot, seed: u64) {
    let mut rng = RngStream::new(seed, ModuleId::BODY_NAMES);
    let star = star_name(seed);
    let mut taken = BTreeSet::from([star.clone()]);
    for body in &mut snapshot.bodies {
        let name = unique(&mut rng, &mut taken);
        body.designation = core::mem::replace(&mut body.name, name);
    }
    name_belts(snapshot, &star);
}

/// Names belts by host, innermost first: `Velora Belt`, or `Inner` and
/// `Outer` when there are two, or by number. A host with no planet to take
/// its label from goes by the system's name.
fn name_belts(snapshot: &mut SystemSnapshot, star: &str) {
    let hosts: BTreeSet<HostRef> = snapshot.belts.iter().map(|b| b.host).collect();
    for host in hosts {
        let label = snapshot
            .planets()
            .find(|p| p.host == host)
            .and_then(|p| p.designation.rsplit_once(' '))
            .map_or(star, |(label, _)| label)
            .to_string();
        let mut own: Vec<_> = snapshot
            .belts
            .iter_mut()
            .filter(|b| b.host == host)
            .collect();
        own.sort_by(|a, b| a.inner_au.total_cmp(&b.inner_au));
        let count = own.len();
        for (index, belt) in own.into_iter().enumerate() {
            belt.name = match (count, index) {
                (1, _) => format!("{label} Belt"),
                (2, 0) => format!("{label} Inner Belt"),
                (2, _) => format!("{label} Outer Belt"),
                _ => format!("{label} Belt {}", index + 1),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_repeat_for_a_seed_and_differ_between_seeds() {
        assert_eq!(star_name(42), star_name(42));
        let distinct: BTreeSet<String> = (1..=200).map(star_name).collect();
        assert!(distinct.len() > 150, "only {} distinct", distinct.len());
    }

    /// Pinned: a change here changes every snapshot (see the hashes in
    /// `tests/classic.rs`).
    #[test]
    fn names_are_pinned() {
        assert_eq!([star_name(1), star_name(42), star_name(1000)], PINNED_NAMES);
    }

    const PINNED_NAMES: [&str; 3] = ["Ama", "Vorus", "Draeth"];

    #[test]
    fn names_are_well_formed() {
        for seed in 1..=20_000u64 {
            let name = star_name(seed);
            assert!((3..=10).contains(&name.len()), "{name}");
            let mut chars = name.chars();
            assert!(chars.next().unwrap().is_ascii_uppercase(), "{name}");
            assert!(chars.all(|c| c.is_ascii_lowercase()), "{name}");
            assert!(reads_well(&name), "{name}");
            assert!(
                !REAL_STAR_NAMES.contains(&name.to_ascii_lowercase().as_str()),
                "{name}"
            );
        }
    }

    #[test]
    fn awkward_joins_are_rejected() {
        assert!(reads_well("Velora"));
        assert!(!reads_well("Alaae"));
        assert!(!reads_well("Draara"));
        assert!(!reads_well("Kalllis"));
        assert!(!reads_well("Ililris"));
        assert!(!reads_well("Erorara"));
        assert!(!reads_well("Kaluys"));
    }

    #[test]
    fn planets_are_lettered_outward_from_b() {
        assert_eq!(planet_name("Velora", 0), "Velora b");
        assert_eq!(planet_name("Velora", 1), "Velora c");
        assert_eq!(planet_name("Velora", 24), "Velora z");
        assert_eq!(planet_name("Velora", 25), "Velora 26");
        assert_eq!(planet_name("Velora", 300), "Velora 301");
    }
}
