pub const ADJECTIVES: [&str; 24] = [
    "sneaky",
    "wiggly",
    "cursed",
    "soggy",
    "mystic",
    "grumpy",
    "sparkly",
    "feral",
    "cosmic",
    "crunchy",
    "spooky",
    "sleepy",
    "chonky",
    "goofy",
    "shiny",
    "damp",
    "ancient",
    "smol",
    "unhinged",
    "polite",
    "haunted",
    "spicy",
    "bouncy",
    "forbidden",
];

pub const NOUNS: [&str; 24] = [
    "worm", "orb", "wizard", "hat", "potion", "goblin", "toad", "spell", "scroll", "mushroom",
    "gremlin", "cauldron", "wand", "noodle", "familiar", "rune", "dragon", "pebble", "moth",
    "slime", "grimoire", "crystal", "newt", "bean",
];

/// Scales a random number in `0..1` to an index below `len`.
pub fn random_index(random: f64, len: usize) -> usize {
    let scaled = (random * len as f64).floor();
    // `as` saturates, and the `min` covers a random number of exactly 1.
    (scaled as usize).min(len.saturating_sub(1))
}

fn pick(words: &[&'static str], random: f64) -> &'static str {
    words
        .get(random_index(random, words.len()))
        .copied()
        .unwrap_or("worm")
}

/// Builds a random alias like `sneaky-worm-42`.
pub fn summon_wizard_words(mut random: impl FnMut() -> f64) -> String {
    let number = random_index(random(), 100);
    let adjective = pick(&ADJECTIVES, random());
    let noun = pick(&NOUNS, random());
    format!("{adjective}-{noun}-{number}")
}
