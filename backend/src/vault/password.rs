//! Strong random passwords for new accounts (the F6 sign-up agent uses this too).

/// Letters and digits without look-alikes (no 0/O, 1/l/I), plus symbols most sites accept.
const LOWER: &[u8] = b"abcdefghijkmnopqrstuvwxyz";
const UPPER: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";
const DIGITS: &[u8] = b"23456789";
const SYMBOLS: &[u8] = b"!@#$%^&*-_=+?";

pub const MIN_LENGTH: usize = 12;
pub const MAX_LENGTH: usize = 64;

/// A random password with at least one of each character class, in random order.
/// Uses rand's thread RNG, a cryptographically secure generator seeded by the OS.
pub fn generate(length: usize) -> String {
    let length = length.clamp(MIN_LENGTH, MAX_LENGTH);
    let all: Vec<u8> = [LOWER, UPPER, DIGITS, SYMBOLS].concat();
    let pick = |set: &[u8]| set[rand::random_range(0..set.len())];

    let mut chars: Vec<u8> = vec![pick(LOWER), pick(UPPER), pick(DIGITS), pick(SYMBOLS)];
    while chars.len() < length {
        chars.push(pick(&all));
    }
    // Fisher-Yates shuffle, so the guaranteed characters aren't always first.
    for i in (1..chars.len()).rev() {
        chars.swap(i, rand::random_range(0..=i));
    }
    String::from_utf8(chars).expect("all characters are ASCII")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_every_class_and_the_right_length() {
        for _ in 0..50 {
            let p = generate(16);
            assert_eq!(p.len(), 16);
            assert!(p.bytes().any(|c| LOWER.contains(&c)));
            assert!(p.bytes().any(|c| UPPER.contains(&c)));
            assert!(p.bytes().any(|c| DIGITS.contains(&c)));
            assert!(p.bytes().any(|c| SYMBOLS.contains(&c)));
        }
        assert_eq!(generate(3).len(), MIN_LENGTH);
        assert_eq!(generate(500).len(), MAX_LENGTH);
        assert_ne!(generate(20), generate(20));
    }
}
