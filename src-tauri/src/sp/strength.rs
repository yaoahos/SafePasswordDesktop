//! 对齐 Android PasswordStrength.kt：熵估算 + 强度评级 + 破解时间文案。
#![allow(dead_code)]

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StrengthResult {
    pub score: i32,
    pub label: String,
    pub entropy_bits: f64,
    pub crack_time: String,
}

const COMMON_PASSWORDS: [&str; 42] = [
    "123456", "123456789", "qwerty", "password", "111111", "12345678", "abc123",
    "1234567", "password1", "12345", "1234567890", "iloveyou", "000000", "123123",
    "admin", "letmein", "welcome", "monkey", "dragon", "sunshine", "princess",
    "football", "baseball", "master", "shadow", "superman", "qazwsx", "666666",
    "888888", "woaini", "5201314", "a123456", "123qwe", "zxcvbnm", "asdfgh",
    "qwertyuiop", "1qaz2wsx", "passw0rd", "p@ssw0rd", "root", "guest", "test",
];

pub fn evaluate(password: &str) -> StrengthResult {
    if password.trim().is_empty() {
        return StrengthResult {
            score: 0,
            label: "很弱".to_string(),
            entropy_bits: 0.0,
            crack_time: "瞬间".to_string(),
        };
    }
    let lower = password.to_lowercase();
    if COMMON_PASSWORDS.contains(&lower.as_str()) {
        return StrengthResult {
            score: 0,
            label: "很弱".to_string(),
            entropy_bits: 0.0,
            crack_time: "瞬间".to_string(),
        };
    }

    let chars: Vec<char> = password.chars().collect();
    let mut pool: f64 = 0.0;
    if chars.iter().any(|c: &char| c.is_ascii_lowercase()) {
        pool += 26.0;
    }
    if chars.iter().any(|c: &char| c.is_ascii_uppercase()) {
        pool += 26.0;
    }
    if chars.iter().any(|c: &char| c.is_ascii_digit()) {
        pool += 10.0;
    }
    // Kotlin isLetterOrDigit 是 Unicode 感知的
    if chars.iter().any(|c| !(c.is_alphabetic() || c.is_numeric())) {
        pool += 33.0;
    }
    if pool == 0.0 {
        pool = 1.0;
    }

    let length = chars.len() as f64;
    let mut entropy = length * (pool.ln() / std::f64::consts::LN_2);
    if length < 6.0 {
        entropy *= 0.6;
    }
    if length >= 8.0 && chars.iter().all(|c| c.is_ascii_digit()) {
        entropy *= 0.55;
    }
    if has_run(&chars, 4) {
        entropy *= 0.75;
    }
    if has_sequence(&chars, 4) {
        entropy *= 0.85;
    }

    let score = if entropy < 28.0 {
        0
    } else if entropy < 36.0 {
        1
    } else if entropy < 60.0 {
        2
    } else if entropy < 80.0 {
        3
    } else {
        4
    };
    let label = match score {
        0 => "很弱",
        1 => "弱",
        2 => "一般",
        3 => "强",
        _ => "很强",
    };
    StrengthResult {
        score,
        label: label.to_string(),
        entropy_bits: entropy,
        crack_time: crack_time(entropy),
    }
}

fn has_run(text: &[char], length: usize) -> bool {
    let mut run = 1;
    for i in 1..text.len() {
        run = if text[i] == text[i - 1] { run + 1 } else { 1 };
        if run >= length {
            return true;
        }
    }
    false
}

fn has_sequence(text: &[char], length: usize) -> bool {
    if text.len() < length {
        return false;
    }
    let mut ascending = 1;
    let mut descending = 1;
    for i in 1..text.len() {
        let delta = text[i] as i32 - text[i - 1] as i32;
        ascending = if delta == 1 { ascending + 1 } else { 1 };
        descending = if delta == -1 { descending + 1 } else { 1 };
        if ascending >= length || descending >= length {
            return true;
        }
    }
    false
}

fn crack_time(entropy_bits: f64) -> String {
    let guesses = (entropy_bits - 1.0).exp2(); // 2^(entropy-1)
    let seconds = guesses / 1.0e10;
    if seconds < 1.0 {
        "瞬间".to_string()
    } else if seconds < 60.0 {
        format!("{} 秒", seconds as i64)
    } else if seconds < 3600.0 {
        format!("{} 分钟", (seconds / 60.0) as i64)
    } else if seconds < 86_400.0 {
        format!("{} 小时", (seconds / 3600.0) as i64)
    } else if seconds < 31_536_000.0 {
        format!("{} 天", (seconds / 86_400.0) as i64)
    } else if seconds < 31_536_000.0 * 1000.0 {
        format!("{} 年", (seconds / 31_536_000.0) as i64)
    } else {
        "超过 1000 年".to_string()
    }
}

// 供 SecurityAnalyzer 去重/计数用的小工具

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_and_common_are_zero() {
        assert_eq!(evaluate("").score, 0);
        assert_eq!(evaluate("   ").score, 0);
        let r = evaluate("password");
        assert_eq!((r.score, r.label.as_str(), r.crack_time.as_str()), (0, "很弱", "瞬间"));
        // 大小写不敏感命中
        assert_eq!(evaluate("PASSWORD").score, 0);
    }

    #[test]
    fn weak_short_digits() {
        let r = evaluate("12345"); // 常见密码
        assert_eq!(r.score, 0);
        let r2 = evaluate("987654"); // 纯数字8位内降权
        assert!(r2.score <= 1, "{}", r2.score);
    }

    #[test]
    fn strong_random() {
        let r = evaluate("Kx9#mQv2$Rt7&Lp4*Wn5");
        assert_eq!(r.score, 4);
        assert_eq!(r.label, "很强");
        // 连续重复 run/sequence 降权
        let dup = evaluate("aaaaaaaaaaaaaaaak");
        let no_dup = evaluate("akzapqmworizndle");
        assert!(dup.entropy_bits < no_dup.entropy_bits || dup.score <= no_dup.score);
    }

    #[test]
    fn run_and_sequence_detection() {
        assert!(has_run(&"xxxxy".chars().collect::<Vec<_>>(), 4));
        assert!(!has_run(&"xxxy".chars().collect::<Vec<_>>(), 4)); // 只有 3 连
        assert!(!has_run(&"xxyy".chars().collect::<Vec<_>>(), 4));
        assert!(has_sequence(&"abcd".chars().collect::<Vec<_>>(), 4));
        assert!(has_sequence(&"dcba".chars().collect::<Vec<_>>(), 4));
        assert!(!has_sequence(&"abab".chars().collect::<Vec<_>>(), 4));
    }

    #[test]
    fn crack_time_buckets() {
        assert_eq!(crack_time(0.0), "瞬间"); // 2^-1/1e10
        assert!(crack_time(40.0).ends_with("秒") || crack_time(40.0).ends_with("分钟"));
        assert_eq!(crack_time(200.0), "超过 1000 年");
    }
}
