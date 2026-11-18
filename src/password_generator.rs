//! Offline password and passphrase generation using fallible OS randomness.
//! Secret values are never formatted, serialized, or retained by this module.
use num_bigint::BigUint;
use rand::{TryRng, rngs::SysRng};
use std::{
    collections::HashSet,
    fmt,
    ops::RangeInclusive,
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

pub const PASSWORD_LENGTH_RANGE: RangeInclusive<usize> = 8..=256;
pub const PASSPHRASE_WORD_RANGE: RangeInclusive<usize> = 3..=32;

const UPPER: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const LOWER: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
const DIGITS: &[u8] = b"0123456789";
const SYMBOLS: &[u8] = b"!@#$%^&*()-_=+[]{};:,.?";
const AMBIGUOUS: &[u8] = b"Il1O0o";
const MAX_DRAWS: usize = 1024;
const MAX_CANDIDATES: usize = 4096;
const MAX_WORD_BYTES: usize = 9;
const MAX_PASSPHRASE_BYTES: usize = 1024;
const WORDS: &str = include_str!("password_generator/eff_large_wordlist.txt");
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static WORD_POOL: OnceLock<Result<Vec<&'static str>, GeneratorError>> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratorError {
    InvalidLength,
    NoClasses,
    EmptyClass,
    RandomUnavailable,
    RejectionLimit,
    InvalidWordList,
    OutputTooLong,
    IdentityExhausted,
}
impl fmt::Display for GeneratorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidLength => "Length is outside the supported range.",
            Self::NoClasses => "Select at least one character class.",
            Self::EmptyClass => "A selected character class has no permitted characters.",
            Self::RandomUnavailable => "Operating system randomness is unavailable.",
            Self::RejectionLimit => "Random sampling exceeded its retry limit.",
            Self::InvalidWordList => "The bundled word list is invalid.",
            Self::OutputTooLong => "The generated passphrase exceeds its size limit.",
            Self::IdentityExhausted => "Result identity space is exhausted.",
        })
    }
}
impl std::error::Error for GeneratorError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasswordSettings {
    pub length: usize,
    pub uppercase: bool,
    pub lowercase: bool,
    pub digits: bool,
    pub symbols: bool,
    pub exclude_ambiguous: bool,
}
impl Default for PasswordSettings {
    fn default() -> Self {
        Self {
            length: 20,
            uppercase: true,
            lowercase: true,
            digits: true,
            symbols: true,
            exclude_ambiguous: false,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Capitalization {
    #[default]
    Lowercase,
    InitialUppercase,
    Uppercase,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Separator {
    #[default]
    Hyphen,
    Space,
    Underscore,
    Dot,
}
impl Separator {
    fn as_char(self) -> char {
        match self {
            Self::Hyphen => '-',
            Self::Space => ' ',
            Self::Underscore => '_',
            Self::Dot => '.',
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassphraseSettings {
    pub words: usize,
    pub separator: Separator,
    pub capitalization: Capitalization,
    pub suffix_digit: bool,
    pub suffix_symbol: bool,
}
impl Default for PassphraseSettings {
    fn default() -> Self {
        Self {
            words: 6,
            separator: Separator::Hyphen,
            capitalization: Capitalization::Lowercase,
            suffix_digit: false,
            suffix_symbol: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SecretResultId(u64);

/// An ephemeral result with explicit borrowed access to its secret text.
pub struct SecretResult {
    id: SecretResultId,
    policy_revision: u64,
    entropy_bits: u64,
    secret: String,
}
impl fmt::Debug for SecretResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecretResult")
            .field("id", &self.id)
            .field("policy_revision", &self.policy_revision)
            .field("entropy_bits", &self.entropy_bits)
            .field("secret", &"[redacted]")
            .finish()
    }
}
impl SecretResult {
    pub fn id(&self) -> SecretResultId {
        self.id
    }
    pub fn policy_revision(&self) -> u64 {
        self.policy_revision
    }
    pub fn entropy_bits(&self) -> u64 {
        self.entropy_bits
    }
    pub fn expose(&self) -> &str {
        &self.secret
    }
}
fn result(
    secret: String,
    policy_revision: u64,
    entropy_bits: u64,
) -> Result<SecretResult, GeneratorError> {
    let id = NEXT_ID
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_| GeneratorError::IdentityExhausted)?;
    Ok(SecretResult {
        id: SecretResultId(id),
        policy_revision,
        entropy_bits,
        secret,
    })
}
fn classes(settings: &PasswordSettings) -> Result<Vec<Vec<u8>>, GeneratorError> {
    if !PASSWORD_LENGTH_RANGE.contains(&settings.length) {
        return Err(GeneratorError::InvalidLength);
    }
    let mut classes = Vec::new();
    for (enabled, source) in [
        (settings.uppercase, UPPER),
        (settings.lowercase, LOWER),
        (settings.digits, DIGITS),
        (settings.symbols, SYMBOLS),
    ] {
        if enabled {
            let class: Vec<_> = source
                .iter()
                .copied()
                .filter(|ch| !settings.exclude_ambiguous || !AMBIGUOUS.contains(ch))
                .collect();
            if class.is_empty() {
                return Err(GeneratorError::EmptyClass);
            }
            classes.push(class);
        }
    }
    if classes.is_empty() {
        return Err(GeneratorError::NoClasses);
    }
    Ok(classes)
}
/// Exact cardinality of length-n strings containing at least one character
/// from each disjoint class, via inclusion-exclusion.
fn constrained_count(sizes: &[usize], length: usize) -> BigUint {
    let total: usize = sizes.iter().sum();
    let mut positive = BigUint::from(0u8);
    let mut negative = BigUint::from(0u8);
    for mask in 0usize..(1usize << sizes.len()) {
        let omitted: usize = sizes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| ((mask >> i) & 1 == 1).then_some(*n))
            .sum();
        let term = BigUint::from(total - omitted).pow(length as u32);
        if mask.count_ones() % 2 == 0 {
            positive += term;
        } else {
            negative += term;
        }
    }
    positive - negative
}
fn bits(count: &BigUint) -> u64 {
    count.bits().saturating_sub(1)
}
pub fn password_entropy_bits(settings: &PasswordSettings) -> Result<u64, GeneratorError> {
    let classes = classes(settings)?;
    Ok(bits(&constrained_count(
        &classes.iter().map(Vec::len).collect::<Vec<_>>(),
        settings.length,
    )))
}
fn sample<R: TryRng + ?Sized>(rng: &mut R, bound: usize) -> Result<usize, GeneratorError> {
    if bound == 0 {
        return Err(GeneratorError::EmptyClass);
    }
    let bound = u32::try_from(bound).map_err(|_| GeneratorError::RejectionLimit)?;
    let threshold = bound.wrapping_neg() % bound;
    for _ in 0..MAX_DRAWS {
        let value = rng
            .try_next_u32()
            .map_err(|_| GeneratorError::RandomUnavailable)?;
        if value >= threshold {
            return Ok((value % bound) as usize);
        }
    }
    Err(GeneratorError::RejectionLimit)
}
fn password_with<R: TryRng + ?Sized>(
    settings: &PasswordSettings,
    revision: u64,
    rng: &mut R,
) -> Result<SecretResult, GeneratorError> {
    let classes = classes(settings)?;
    let alphabet: Vec<u8> = classes.iter().flatten().copied().collect();
    let entropy = bits(&constrained_count(
        &classes.iter().map(Vec::len).collect::<Vec<_>>(),
        settings.length,
    ));
    for _ in 0..MAX_CANDIDATES {
        let mut candidate = Vec::with_capacity(settings.length);
        for _ in 0..settings.length {
            candidate.push(alphabet[sample(rng, alphabet.len())?]);
        }
        if classes
            .iter()
            .all(|class| candidate.iter().any(|byte| class.contains(byte)))
        {
            let secret = candidate.into_iter().map(char::from).collect();
            return result(secret, revision, entropy);
        }
    }
    Err(GeneratorError::RejectionLimit)
}
pub(crate) fn generate_legacy_password(length: usize) -> Result<SecretResult, GeneratorError> {
    generate_password(
        &PasswordSettings {
            length,
            symbols: false,
            ..PasswordSettings::default()
        },
        0,
    )
}

pub fn generate_password(
    settings: &PasswordSettings,
    policy_revision: u64,
) -> Result<SecretResult, GeneratorError> {
    password_with(settings, policy_revision, &mut SysRng)
}
fn pool() -> Result<&'static [&'static str], GeneratorError> {
    WORD_POOL
        .get_or_init(|| {
            let mut keys = HashSet::new();
            let mut words = HashSet::new();
            let mut pool = Vec::new();
            for line in WORDS.lines() {
                let (key, word) = line
                    .split_once('\t')
                    .ok_or(GeneratorError::InvalidWordList)?;
                if key.len() != 5
                    || !key.bytes().all(|v| (b'1'..=b'6').contains(&v))
                    || word.is_empty()
                    || word.len() > MAX_WORD_BYTES
                    || !word.bytes().all(|v| v.is_ascii_lowercase() || v == b'-')
                    || !keys.insert(key)
                    || !words.insert(word)
                {
                    return Err(GeneratorError::InvalidWordList);
                }
                if word.bytes().all(|v| v.is_ascii_lowercase()) {
                    pool.push(word);
                }
            }
            if keys.len() != 7776 || pool.len() != 7772 {
                return Err(GeneratorError::InvalidWordList);
            }
            Ok(pool)
        })
        .as_ref()
        .map(Vec::as_slice)
        .map_err(|e| *e)
}
pub fn passphrase_entropy_bits(settings: &PassphraseSettings) -> Result<u64, GeneratorError> {
    if !PASSPHRASE_WORD_RANGE.contains(&settings.words) {
        return Err(GeneratorError::InvalidLength);
    }
    let mut count = BigUint::from(pool()?.len()).pow(settings.words as u32);
    if settings.suffix_digit {
        count *= BigUint::from(DIGITS.len());
    }
    if settings.suffix_symbol {
        count *= BigUint::from(SYMBOLS.len());
    }
    Ok(bits(&count))
}
fn passphrase_with<R: TryRng + ?Sized>(
    settings: &PassphraseSettings,
    revision: u64,
    rng: &mut R,
) -> Result<SecretResult, GeneratorError> {
    let entropy = passphrase_entropy_bits(settings)?;
    let words = pool()?;
    let mut output = String::new();
    for index in 0..settings.words {
        if index > 0 {
            output.push(settings.separator.as_char());
        }
        let word = words[sample(rng, words.len())?];
        match settings.capitalization {
            Capitalization::Lowercase => output.push_str(word),
            Capitalization::Uppercase => output.push_str(&word.to_ascii_uppercase()),
            Capitalization::InitialUppercase => {
                let (first, rest) = word.split_at(1);
                output.push_str(&first.to_ascii_uppercase());
                output.push_str(rest);
            }
        }
        if output.len() > MAX_PASSPHRASE_BYTES {
            return Err(GeneratorError::OutputTooLong);
        }
    }
    if settings.suffix_digit {
        output.push(DIGITS[sample(rng, DIGITS.len())?] as char);
    }
    if settings.suffix_symbol {
        output.push(SYMBOLS[sample(rng, SYMBOLS.len())?] as char);
    }
    if output.len() > MAX_PASSPHRASE_BYTES {
        return Err(GeneratorError::OutputTooLong);
    }
    result(output, revision, entropy)
}
pub fn generate_passphrase(
    settings: &PassphraseSettings,
    policy_revision: u64,
) -> Result<SecretResult, GeneratorError> {
    passphrase_with(settings, policy_revision, &mut SysRng)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixed {
        values: Vec<u64>,
        index: usize,
        fail: bool,
    }
    impl TryRng for Fixed {
        type Error = std::io::Error;
        fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
            self.try_next_u64().map(|v| v as u32)
        }
        fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
            if self.fail {
                return Err(std::io::Error::other("fixture failure"));
            }
            let value = self.values[self.index % self.values.len()];
            self.index += 1;
            Ok(value)
        }
        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Self::Error> {
            for chunk in dest.chunks_mut(8) {
                let bytes = self.try_next_u64()?.to_le_bytes();
                chunk.copy_from_slice(&bytes[..chunk.len()]);
            }
            Ok(())
        }
    }
    #[test]
    fn generator_rng_errors_and_unbiased_rejection() {
        let mut rng = Fixed {
            values: vec![0, 7],
            index: 0,
            fail: false,
        };
        assert_eq!(sample(&mut rng, 10), Ok(7));
        assert_eq!(rng.index, 2);
        let mut bad = Fixed {
            values: vec![0],
            index: 0,
            fail: true,
        };
        assert_eq!(sample(&mut bad, 10), Err(GeneratorError::RandomUnavailable));
        bad.fail = false;
        assert_eq!(sample(&mut bad, 10), Err(GeneratorError::RejectionLimit));
    }
    #[test]
    fn generator_counts_and_required_classes() {
        assert_eq!(constrained_count(&[2, 1], 2), BigUint::from(4u8));
        assert_eq!(bits(&constrained_count(&[2, 1], 2)), 2);
        let settings = PasswordSettings {
            length: 8,
            uppercase: true,
            lowercase: true,
            digits: true,
            symbols: true,
            exclude_ambiguous: true,
        };
        let mut rng = Fixed {
            values: vec![79, 103, 127, 135, 80, 104, 128, 136],
            index: 0,
            fail: false,
        };
        let result = password_with(&settings, 42, &mut rng).unwrap();
        let generated = result.expose().as_bytes();
        assert!(
            classes(&settings)
                .unwrap()
                .iter()
                .all(|class| generated.iter().any(|b| class.contains(b)))
        );
        assert!(!generated.iter().any(|b| AMBIGUOUS.contains(b)));
        assert_eq!(result.policy_revision(), 42);
        assert!(!format!("{result:?}").contains(result.expose()));
        let mut repeated = Fixed {
            values: vec![80],
            index: 0,
            fail: false,
        };
        assert!(matches!(
            password_with(&settings, 0, &mut repeated),
            Err(GeneratorError::RejectionLimit)
        ));
        assert!(
            classes(&PasswordSettings {
                length: 7,
                ..settings.clone()
            })
            .is_err()
        );
        assert!(
            classes(&PasswordSettings {
                uppercase: false,
                lowercase: false,
                digits: false,
                symbols: false,
                ..settings
            })
            .is_err()
        );
    }
    #[test]
    fn generator_os_random_entry_points_return_distinct_result_ids() {
        let password = generate_password(&PasswordSettings::default(), 1).unwrap();
        let phrase = generate_passphrase(&PassphraseSettings::default(), 2).unwrap();
        assert_ne!(password.id(), phrase.id());
        assert_eq!(password.expose().len(), 20);
        assert_eq!(phrase.expose().split('-').count(), 6);
        assert_eq!(phrase.policy_revision(), 2);
        assert!(!format!("{phrase:?}").contains(phrase.expose()));
    }

    #[test]
    fn generator_word_list_and_passphrase_bounds() {
        let words = pool().unwrap();
        assert_eq!(words.len(), 7772);
        assert_eq!(WORDS.lines().count(), 7776);
        use sha2::{Digest, Sha256};
        assert_eq!(
            format!("{:x}", Sha256::digest(WORDS.as_bytes())),
            "addd35536511597a02fa0a9ff1e5284677b8883b83e986e43f15a3db996b903e"
        );
        assert_eq!(
            WORDS
                .lines()
                .filter(|line| line.split_once('\t').unwrap().1.contains('-'))
                .count(),
            4
        );
        let mut rng = Fixed {
            values: vec![8000],
            index: 0,
            fail: false,
        };
        let settings = PassphraseSettings::default();
        let result = passphrase_with(&settings, 0, &mut rng).unwrap();
        assert_eq!(result.expose().split('-').count(), 6);
        assert!(result.expose().len() <= MAX_PASSPHRASE_BYTES);
        assert_eq!(result.entropy_bits(), bits(&BigUint::from(7772u32).pow(6)));
        let suffix = SYMBOLS.iter().position(|byte| *byte == b'-').unwrap();
        let mut rng = Fixed {
            values: vec![7772, 7773, 7774, 17, (SYMBOLS.len() + suffix) as u64],
            index: 0,
            fail: false,
        };
        let decorated = PassphraseSettings {
            words: 3,
            separator: Separator::Hyphen,
            capitalization: Capitalization::Uppercase,
            suffix_digit: true,
            suffix_symbol: true,
        };
        let phrase = passphrase_with(&decorated, 0, &mut rng).unwrap();
        assert_eq!(
            phrase.expose(),
            format!("{}7-", words[..3].join("-").to_ascii_uppercase())
        );
        let count = BigUint::from(7772u32).pow(3) * DIGITS.len() * SYMBOLS.len();
        assert_eq!(phrase.entropy_bits(), bits(&count));
        assert!(
            passphrase_entropy_bits(&PassphraseSettings {
                words: 33,
                ..settings
            })
            .is_err()
        );
    }
}
