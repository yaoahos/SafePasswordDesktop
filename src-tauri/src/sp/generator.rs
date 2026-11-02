//! 对齐 Android PasswordGenerator.kt：字符口令 + 词表 passphrase。
#![allow(dead_code)]

use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GeneratorOptions {
    pub length: i32,
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub symbols: bool,
    pub exclude_ambiguous: bool,
    pub require_each: bool,
}

impl Default for GeneratorOptions {
    fn default() -> Self {
        GeneratorOptions {
            length: 20,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: true,
            exclude_ambiguous: true,
            require_each: true,
        }
    }
}

const LOWER: &str = "abcdefghijkmnopqrstuvwxyz";
const LOWER_FULL: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHJKLMNPQRSTUVWXYZ";
const UPPER_FULL: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "23456789";
const DIGITS_FULL: &str = "0123456789";
const SYMBOLS: &str = "!@#%^&*()-_=+[]{};:,.?";
const AMBIGUOUS: &str = "O0Il1|`'\"";

pub fn generate(options: &GeneratorOptions) -> String {
    let lower = if options.exclude_ambiguous { LOWER } else { LOWER_FULL };
    let upper = if options.exclude_ambiguous { UPPER } else { UPPER_FULL };
    let digits = if options.exclude_ambiguous { DIGITS } else { DIGITS_FULL };
    let symbols = if options.exclude_ambiguous {
        SYMBOLS.to_string()
    } else {
        // Kotlin: SYMBOLS + AMBIGUOUS
        format!("{SYMBOLS}{AMBIGUOUS}")
    };

    let mut active: Vec<&str> = Vec::new();
    if options.lowercase {
        active.push(lower);
    }
    if options.uppercase {
        active.push(upper);
    }
    if options.digits {
        active.push(digits);
    }
    if options.symbols {
        active.push(&symbols);
    }
    if active.is_empty() {
        active.push(lower);
    }

    let mut length = options.length.clamp(4, 128);
    if options.require_each && length < active.len() as i32 {
        length = active.len() as i32;
    }
    let all: String = active.concat();

    let mut rng = rand::thread_rng();
    let mut chars: Vec<char> = Vec::with_capacity(length as usize);
    if options.require_each {
        for group in &active {
            let idx = rng.gen_range(0..group.chars().count());
            chars.push(group.chars().nth(idx).unwrap());
        }
    }
    let all_chars: Vec<char> = all.chars().collect();
    while chars.len() < length as usize {
        let idx = rng.gen_range(0..all_chars.len());
        chars.push(all_chars[idx]);
    }

    // Fisher-Yates 洗牌
    for i in (1..chars.len()).rev() {
        let j = rng.gen_range(0..=i);
        chars.swap(i, j);
    }
    chars.into_iter().collect()
}

pub fn passphrase(word_count: i32, separator: &str, capitalize: bool, include_number: bool) -> String {
    let count = word_count.clamp(3, 8) as usize;
    let mut pool: Vec<&str> = WORDS.to_vec();
    let mut rng = rand::thread_rng();
    pool.shuffle(&mut rng);
    let mut words: Vec<String> = pool[..count]
        .iter()
        .map(|w| {
            if capitalize {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            } else {
                (*w).to_string()
            }
        })
        .collect();
    if include_number {
        // Kotlin: random.nextInt(10, 100) → [10, 100)
        let n: i32 = rng.gen_range(10..100);
        words.push(n.to_string());
    }
    words.join(separator)
}

const WORDS: [&str; 136] = [
    "apple", "arrow", "atlas", "autumn", "banjo", "beach", "berry", "birch",
    "blade", "bloom", "breeze", "bridge", "brook", "cabin", "candle", "cedar",
    "chalk", "cherry", "cloud", "clover", "coast", "comet", "coral", "crane",
    "creek", "crown", "dawn", "delta", "dove", "dune", "eagle", "earth",
    "ember", "fable", "fern", "field", "flame", "flint", "forest", "frost",
    "garden", "glade", "glass", "globe", "gold", "grain", "grape", "grove",
    "harbor", "hazel", "heron", "hill", "honey", "island", "ivory", "jade",
    "jasmine", "juniper", "kelp", "lake", "lantern", "leaf", "lemon", "lily",
    "linen", "lotus", "lunar", "maple", "marble", "meadow", "melon", "mist",
    "moon", "moss", "mountain", "nest", "night", "north", "oak", "ocean",
    "olive", "onyx", "opal", "orange", "orchid", "otter", "pearl", "pebble",
    "pine", "plum", "pond", "poppy", "prairie", "quartz", "rain", "raven",
    "reed", "ridge", "river", "robin", "rose", "sail", "sand", "scarlet",
    "sea", "shell", "silver", "sky", "snow", "solar", "sparrow", "spring",
    "spruce", "star", "stone", "storm", "stream", "summer", "sun", "swan",
    "thistle", "tide", "timber", "topaz", "trail", "tulip", "valley", "violet",
    "wave", "willow", "wind", "winter", "wolf", "wood", "wren", "zephyr",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_defaults_and_bounds() {
        let opts = GeneratorOptions::default();
        assert_eq!(opts.length, 20);
        let pw = generate(&opts);
        assert_eq!(pw.len(), 20);
        // exclude_ambiguous 默认开：不含 O0Il1
        assert!(!pw.contains('O') && !pw.contains('0') && !pw.contains('l') && !pw.contains('I') && !pw.contains('1'));
        // requireEach：四组各至少一个
        assert!(pw.chars().any(|c| LOWER.contains(c)));
        assert!(pw.chars().any(|c| UPPER.contains(c)));
        assert!(pw.chars().any(|c| DIGITS.contains(c)));
        assert!(pw.chars().any(|c| SYMBOLS.contains(c)));
        // 长度钳制
        let tiny = generate(&GeneratorOptions { length: 1, ..Default::default() });
        assert_eq!(tiny.len(), 4); // 钳到 4，且 requireEach 需要 4 组 → 4
        let huge = generate(&GeneratorOptions { length: 999, ..Default::default() });
        assert_eq!(huge.len(), 128);
    }

    #[test]
    fn generate_single_group_and_no_require() {
        // 只开小写
        let pw = generate(&GeneratorOptions {
            length: 12,
            uppercase: false,
            digits: false,
            symbols: false,
            ..Default::default()
        });
        assert_eq!(pw.len(), 12);
        assert!(pw.chars().all(|c| LOWER.contains(c)));
        // requireEach 关 + length < 组数 → 不强制放大到组数
        let pw2 = generate(&GeneratorOptions {
            length: 4,
            require_each: false,
            ..Default::default()
        });
        assert_eq!(pw2.len(), 4);
    }

    #[test]
    fn passphrase_shape() {
        let phrase = passphrase(4, "-", true, true);
        let parts: Vec<&str> = phrase.split('-').collect();
        assert_eq!(parts.len(), 5); // 4 词 + 1 数字
        // 词首字母大写
        assert!(parts[0].chars().next().unwrap().is_uppercase());
        // 数字在 10..99
        let num: i32 = parts[4].parse().unwrap();
        assert!((10..100).contains(&num));
        // count 钳制 3..=8
        let short = passphrase(1, " ", false, false);
        assert_eq!(short.split(' ').count(), 3);
        let long = passphrase(99, " ", false, false);
        assert_eq!(long.split(' ').count(), 8);
        // 不含数字
        let plain = passphrase(3, " ", false, false);
        assert_eq!(plain.split(' ').count(), 3);
    }

    #[test]
    fn word_list_is_136_unique_lowercase() {
        // Kotlin words 列表实测 136 词（core-spec 里写的 120 是误数）
        assert_eq!(WORDS.len(), 136);
        let mut sorted: Vec<&str> = WORDS.to_vec();
        sorted.sort_unstable();
        sorted.dedup_by(|a, b| a == b);
        assert_eq!(sorted.len(), 136);
        assert!(WORDS.iter().all(|w| *w == w.to_lowercase()));
    }
}
