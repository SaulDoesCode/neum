//! the sums: ordinal and reversed letter gematria, byte sums, and what can be said about a number

use std::borrow::Cow;

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use serde::{Deserialize, Serialize};

/// which sum the middle letter is on: o, r, or b (the byte sum bucket). Clicking it cycles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Ordinal,
    Reversed,
    Bytes,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Ordinal, Mode::Reversed, Mode::Bytes];

    /// o → r → b → o
    pub fn next(self) -> Mode {
        match self {
            Mode::Ordinal => Mode::Reversed,
            Mode::Reversed => Mode::Bytes,
            Mode::Bytes => Mode::Ordinal,
        }
    }

    pub fn letter(self) -> &'static str {
        match self {
            Mode::Ordinal => "o",
            Mode::Reversed => "r",
            Mode::Bytes => "b",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::Ordinal => "ordinal",
            Mode::Reversed => "reversed",
            Mode::Bytes => "bytes",
        }
    }

    /// this mode's sum of some data
    pub fn sum(self, data: &[u8]) -> u64 {
        match self {
            Mode::Ordinal => sums(data).0,
            Mode::Reversed => sums(data).1,
            Mode::Bytes => byte_sum(data),
        }
    }
}

pub fn parse_u8_list(s: &str) -> Result<Vec<u8>, String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    trimmed
        .split(',')
        .map(|part| {
            let part_trimmed = part.trim();
            part_trimmed
                .parse::<u8>()
                .map_err(|e| format!("Invalid integer '{}': {}", part_trimmed, e))
        })
        .collect()
}

/// whether the input is written as bytes: two or more comma separated 0..=255 (`104,105`),
/// the way keys are shown. A lone number is a sum to look up, so it doesn't count.
pub fn is_byte_list(s: &str) -> bool {
    s.contains(',') && parse_u8_list(s).is_ok_and(|b| !b.is_empty())
}

/// the bytes an input stands for: written bytes as those bytes, anything else as its UTF-8
pub fn input_bytes(s: &str) -> Cow<'_, [u8]> {
    if is_byte_list(s)
        && let Ok(b) = parse_u8_list(s)
    {
        return Cow::Owned(b);
    }
    Cow::Borrowed(s.as_bytes())
}

/// bytes as the comma list keys are written in (`104,105`)
pub fn byte_list(b: &[u8]) -> String {
    b.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")
}

pub fn hsl(h: f64, s: f64, l: f64) -> [u8; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    [to(r), to(g), to(b)]
}

/// golden-angle hues, so neighbouring sums land far apart on the wheel
pub fn color_for_sum(sum: u64) -> [u8; 3] {
    hsl((sum as f64 * 137.507_764) % 360.0, 0.45, 0.45)
}

/// AVX2 fast path from weum: 32 bytes per block, 4 blocks unrolled, PSADBW accumulation.
///
/// Unlike weum's kernel, the letter test is `(b | 0x20) - 'a' <= 25` rather than
/// `(b | 0x20) - ('a' - 1) <= 26`: with the latter, '@' and '`' pass as letters of value
/// 0 and add 27 to the reversed sum.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn dual_sum_avx2(data: &[u8]) -> (u64, u64) {
    let lower = _mm256_set1_epi8(0x20);
    let a = _mm256_set1_epi8(b'a' as i8);
    let max = _mm256_set1_epi8(25);
    let one = _mm256_set1_epi8(1);
    let twentyseven = _mm256_set1_epi8(27);
    let zero = _mm256_setzero_si256();

    let mut s = [zero; 4];
    let mut r = [zero; 4];

    macro_rules! block {
        ($i:expr, $p:expr) => {{
            let v = unsafe { _mm256_loadu_si256($p as *const __m256i) };
            // 0 for 'a' .. 25 for 'z', anything else lands above 25
            let off = _mm256_sub_epi8(_mm256_or_si256(v, lower), a);
            let mask = _mm256_cmpeq_epi8(_mm256_min_epu8(off, max), off);
            let gval = _mm256_and_si256(_mm256_add_epi8(off, one), mask);
            let rval = _mm256_and_si256(_mm256_sub_epi8(twentyseven, gval), mask);
            s[$i] = _mm256_add_epi64(s[$i], _mm256_sad_epu8(gval, zero));
            r[$i] = _mm256_add_epi64(r[$i], _mm256_sad_epu8(rval, zero));
        }};
    }

    let len = data.len();
    let ptr = data.as_ptr();
    let mut i = 0;
    while i + 128 <= len {
        block!(0, ptr.add(i));
        block!(1, ptr.add(i + 32));
        block!(2, ptr.add(i + 64));
        block!(3, ptr.add(i + 96));
        i += 128;
    }
    while i + 32 <= len {
        block!(0, ptr.add(i));
        i += 32;
    }

    let reduce = |acc: [__m256i; 4]| -> u64 {
        let t = _mm256_add_epi64(_mm256_add_epi64(acc[0], acc[1]), _mm256_add_epi64(acc[2], acc[3]));
        let lanes = _mm_add_epi64(_mm256_castsi256_si128(t), _mm256_extracti128_si256(t, 1));
        (_mm_cvtsi128_si64(lanes) as u64).wrapping_add(_mm_extract_epi64(lanes, 1) as u64)
    };
    let (ordinal, reversed) = sums_scalar(&data[i..]);
    (reduce(s) + ordinal, reduce(r) + reversed)
}

pub fn sums_scalar(data: &[u8]) -> (u64, u64) {
    let (mut o, mut r) = (0u64, 0u64);
    for &b in data {
        let lc = b | 0x20;
        if lc.is_ascii_lowercase() {
            let v = (lc - b'a' + 1) as u64;
            o += v;
            r += 27 - v;
        }
    }
    (o, r)
}

pub fn sums(data: &[u8]) -> (u64, u64) {
    #[cfg(target_arch = "x86_64")]
    if std::is_x86_feature_detected!("avx2") {
        return unsafe { dual_sum_avx2(data) };
    }
    sums_scalar(data)
}

fn is_delimiter(b: u8) -> bool {
    matches!(b, b' ' | b'\n' | b'\t' | b'\r')
}

/// splits on spaces, newlines, tabs and carriage returns like weum's `tokenate`; tokens
/// are the raw bytes between delimiters, in order
pub fn tokenate(data: &[u8]) -> Vec<&[u8]> {
    #[cfg(target_arch = "x86_64")]
    if std::is_x86_feature_detected!("avx2") {
        return unsafe { tokenize_avx2(data) };
    }
    tokenize_scalar(data)
}

pub fn tokenize_scalar(data: &[u8]) -> Vec<&[u8]> {
    data.split(|&b| is_delimiter(b)).filter(|t| !t.is_empty()).collect()
}

/// 32 bytes at a time: one delimiter bitmask per block, from which word starts and ends
/// fall out as bit transitions (carried across blocks)
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn tokenize_avx2(data: &[u8]) -> Vec<&[u8]> {
    let space = _mm256_set1_epi8(b' ' as i8);
    let newline = _mm256_set1_epi8(b'\n' as i8);
    let tab = _mm256_set1_epi8(b'\t' as i8);
    let cr = _mm256_set1_epi8(b'\r' as i8);

    let mut tokens = Vec::new();
    let mut start = 0usize;
    let mut in_word = false;
    let len = data.len();
    let mut i = 0;
    while i + 32 <= len {
        let v = unsafe { _mm256_loadu_si256(data.as_ptr().add(i) as *const __m256i) };
        let delim = _mm256_or_si256(
            _mm256_or_si256(_mm256_cmpeq_epi8(v, space), _mm256_cmpeq_epi8(v, newline)),
            _mm256_or_si256(_mm256_cmpeq_epi8(v, tab), _mm256_cmpeq_epi8(v, cr)),
        );
        let word = !(_mm256_movemask_epi8(delim) as u32);
        // a bit per byte saying whether the byte before it was part of a word
        let before = (word << 1) | in_word as u32;
        let mut edges = (word & !before) | (!word & before);
        while edges != 0 {
            let bit = edges.trailing_zeros() as usize;
            if word >> bit & 1 == 1 {
                start = i + bit;
            } else {
                tokens.push(&data[start..i + bit]);
            }
            edges &= edges - 1;
        }
        in_word = word >> 31 == 1;
        i += 32;
    }
    for (j, &b) in data.iter().enumerate().skip(i) {
        match (is_delimiter(b), in_word) {
            (true, true) => {
                tokens.push(&data[start..j]);
                in_word = false;
            }
            (false, false) => {
                start = j;
                in_word = true;
            }
            _ => {}
        }
    }
    if in_word {
        tokens.push(&data[start..]);
    }
    tokens
}

/// each distinct token once (sorted by bytes). FIX: `dedup` alone only removes repeats
/// that sit next to each other, so "a b a" kept both a's
pub fn unique_tokens(data: &[u8]) -> Vec<&[u8]> {
    let mut tkns = tokenate(data);
    tkns.sort_unstable();
    tkns.dedup();
    tkns
}

/// a^b written out in decimal, however long it gets
pub fn permsq(a: u64, b: u64) -> String {
    let mut digits = vec![1u8];
    for _ in 0..b {
        let mut carry = 0u64;
        for d in digits.iter_mut().rev() {
            let prod = (*d as u64) * a + carry;
            *d = (prod % 10) as u8;
            carry = prod / 10;
        }
        while carry > 0 {
            digits.insert(0, (carry % 10) as u8);
            carry /= 10;
        }
    }
    digits.into_iter().map(|d| (d + b'0') as char).collect()
}

/// the byte sums of some data
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ByteSums {
    /// Σ (b + 1)
    pub bytes: u64,
    /// Σ (256 − b)
    pub reversed: u64,
    /// Σ digit_sum(b + 1)
    pub digits: u64,
    /// how many bytes were summed
    pub len: u64,
}

impl ByteSums {
    /// the digit roots of `bytes`, `reversed` and `digits` (0 for 0, else 1 + (n − 1) mod 9)
    pub fn roots(&self) -> [u64; 3] {
        [digit_root(self.bytes), digit_root(self.reversed), digit_root(self.digits)]
    }

    /// the mean byte value, 1 ..= 256 (0 for no bytes)
    pub fn mean(&self) -> f64 {
        if self.len == 0 { 0.0 } else { self.bytes as f64 / self.len as f64 }
    }
}

/// the value of one byte: 1 ..= 256
pub fn value(b: u8) -> u64 {
    b as u64 + 1
}

/// the sum of n's decimal digits
pub fn digit_sum(mut n: u64) -> u64 {
    let mut s = 0;
    while n > 0 {
        s += n % 10;
        n /= 10;
    }
    s
}

/// 0 for 0, else 1 + (n − 1) mod 9
pub fn digit_root(n: u64) -> u64 {
    if n == 0 { 0 } else { 1 + (n - 1) % 9 }
}

const DIGITS: [u8; 256] = {
    let mut t = [0u8; 256];
    let mut i = 0;
    while i < 256 {
        let mut n = i + 1;
        let mut s = 0;
        while n > 0 {
            s += n % 10;
            n /= 10;
        }
        t[i] = s as u8;
        i += 1;
    }
    t
};

/// every byte sum of `data`
pub fn byte_sums(data: &[u8]) -> ByteSums {
    let (mut bytes, mut digits) = (0u64, 0u64);
    for &b in data {
        bytes += b as u64;
        digits += DIGITS[b as usize] as u64;
    }
    let len = data.len() as u64;
    bytes += len;
    ByteSums {bytes, reversed: 257 * len - bytes, digits, len}
}

/// Σ (b + 1) alone
pub fn byte_sum(data: &[u8]) -> u64 {
    data.iter().map(|&b| b as u64).sum::<u64>() + data.len() as u64
}

pub fn prime_factors(mut n: u64) -> Vec<u64> {
    let mut found = Vec::new();
    let mut d = 2;
    while n > 1 {
        while n.is_multiple_of(d) {
            found.push(d);
            n /= d;
        }
        d += 1;
        if d * d > n && n > 1 {
            found.push(n);
            break;
        }
    }
    found
}

fn pf_sum(n: u64) -> u64 {
    prime_factors(n).iter().sum()
}

pub fn pf_reduction(s: u64) -> (Vec<u64>, u64, u64) {
    const LIMIT: usize = 32;
    let pf = prime_factors(s);
    let pfr: u64 = pf.iter().sum();
    let ds = digit_sum(s);
    let dp = ds * pfr;
    let mut chain: Vec<u64> = Vec::new();
    for _ in 0..=LIMIT {
        let xn = match chain.last() {
            Some(&last) => pf_sum(last) * digit_sum(last),
            None => pf_sum(dp) * ds,
        };
        if chain.contains(&xn) { break; }
        chain.push(xn);
    }
    let total = chain.iter().sum::<u64>() + pfr;
    (chain, total, digit_sum(total))
}

#[allow(dead_code)]
pub(crate) fn vowel(c: u8) -> u64 {
    match c {
        b'a' => 1, b'e' => 2,
        b'o' => 3, b'u' => 4,
        b'i' => 5, b'y' => 6,
        _ => 0,
    }
}

#[allow(dead_code)]
pub(crate) fn consonant(c: u8) -> u64 {
    match c {
        b'b' => 1, b'c' => 2, b'd' => 3, b'f' => 4,
        b'g' => 5, b'h' => 6, b'j' => 7, b'k' => 8,
        b'l' => 9, b'm' => 10, b'n' => 11, b'p' => 12,
        b'q' => 13, b'r' => 14, b's' => 15, b't' => 16,
        b'v' => 17, b'w' => 18, b'x' => 19, b'z' => 20,
        _ => 0,
    }
}

/// the byte sum story of some data: its byte sums, and the factoring of the byte sum
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Story {
    pub sums: ByteSums,
    /// prime factors of `sums.bytes`
    pub factors: Vec<u64>,
    /// `pf_reduction(sums.bytes)`: the chain, its total and that total's digit sum
    pub chain: Vec<u64>,
    pub total: u64,
    pub total_digits: u64,
}

pub fn byte_story(data: &[u8]) -> Story {
    let sums = byte_sums(data);
    let (chain, total, total_digits) = pf_reduction(sums.bytes);
    Story { sums, factors: prime_factors(sums.bytes), chain, total, total_digits }
}

impl Story {
    /// `2 × 3 × 173`, `prime` for primes, `·` below 2
    pub fn factored(&self) -> String {
        match self.factors.as_slice() {
            [] => "·".into(),
            [_] => "prime".into(),
            f => f.iter().map(u64::to_string).collect::<Vec<_>>().join(" × "),
        }
    }

    /// the story as short lines, for under the sums
    pub fn lines(&self) -> Vec<String> {
        let s = &self.sums;
        if s.len == 0 {
            return vec!["no bytes yet".into()];
        }
        let [rb, rr, rd] = s.roots();
        let chain = self.chain.iter().map(u64::to_string).collect::<Vec<_>>().join(" → ");
        vec![
            format!(
                "{} byte{} · mean {:.2} · digits {} · roots {rb}/{rr}/{rd}",
                s.len,
                if s.len == 1 { "" } else { "s" },
                s.mean(),
                s.digits
            ),
            format!("{} = {}", s.bytes, self.factored()),
            format!("{chain} ⇒ {} ⇒ {}", self.total, self.total_digits),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// a small deterministic generator, so the fast paths see plenty of shapes
    fn noise(seed: u64, len: usize) -> Vec<u8> {
        let mut x = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x >> 24) as u8
            })
            .collect()
    }

    #[test]
    fn ordinal_and_reversed() {
        assert_eq!(sums(b"a"), (1, 26));
        assert_eq!(sums(b"z"), (26, 1));
        assert_eq!(sums(b"ABC abc"), (12, 150));
        assert_eq!(sums(b"ordinary cunt"), (162, 162));
        // '@' and '`' sit beside the letters and aren't letters
        assert_eq!(sums(b"@`[{"), (0, 0));
        assert_eq!(sums(b""), (0, 0));
    }

    #[test]
    fn fast_sums_match_scalar() {
        for len in [0, 1, 31, 32, 33, 127, 128, 129, 300, 1000] {
            for seed in 0..8 {
                let d = noise(seed + len as u64, len);
                assert_eq!(sums(&d), sums_scalar(&d), "len {len} seed {seed}");
            }
        }
    }

    #[test]
    fn tokens() {
        assert_eq!(tokenize_scalar(b"  a bb\tccc\r\nd "), vec![&b"a"[..], b"bb", b"ccc", b"d"]);
        assert!(tokenate(b" \n\t ").is_empty());
        assert_eq!(unique_tokens(b"a b a c b"), vec![&b"a"[..], b"b", b"c"]);
    }

    #[test]
    fn fast_tokens_match_scalar() {
        for len in [0, 5, 31, 32, 33, 64, 65, 200, 777] {
            for seed in 0..8 {
                // mostly letters, with delimiters sprinkled in
                let d: Vec<u8> = noise(seed * 31 + len as u64, len)
                    .into_iter()
                    .map(|b| match b % 7 { 0 => b' ', 1 => b'\n', 2 if b % 3 == 0 => b'\t', _ => b'a' + b % 26 })
                    .collect();
                assert_eq!(tokenate(&d), tokenize_scalar(&d), "len {len} seed {seed}");
            }
        }
    }

    #[test]
    fn modes_cycle_through_three() {
        let mut m = Mode::Ordinal;
        let mut seen = vec![];
        for _ in 0..3 {
            seen.push(m.letter());
            m = m.next();
        }
        assert_eq!(seen, ["o", "r", "b"]);
        assert_eq!(m, Mode::Ordinal);
        assert_eq!(Mode::Bytes.sum(b"hi"), 105 + 106);
        assert_eq!(Mode::Reversed.sum(b"a"), 26);
    }

    #[test]
    fn byte_sums_add_up() {
        let s = byte_sums(b"hi");
        assert_eq!(s.bytes, 105 + 106);
        assert_eq!(s.reversed, (256 - 104) + (256 - 105));
        assert_eq!(s.len, 2);
        assert_eq!(s.digits, digit_sum(105) + digit_sum(106));
        assert_eq!(s.bytes, byte_sum(b"hi"));
        assert_eq!(s.mean(), 105.5);
        assert_eq!(byte_sums(&[]), ByteSums::default());
        assert_eq!(byte_sums(&[255]).bytes, 256);
        assert_eq!(byte_sums(&[255]).reversed, 1);
        for b in 0..=255u8 {
            assert_eq!(DIGITS[b as usize] as u64, digit_sum(value(b)));
        }
    }

    #[test]
    fn roots_and_digits() {
        assert_eq!(digit_sum(0), 0);
        assert_eq!(digit_sum(9875), 29);
        assert_eq!(digit_root(0), 0);
        assert_eq!(digit_root(9), 9);
        assert_eq!(digit_root(10), 1);
        assert_eq!(digit_root(9875), 2);
    }

    #[test]
    fn factoring() {
        assert_eq!(prime_factors(0), Vec::<u64>::new());
        assert_eq!(prime_factors(1), Vec::<u64>::new());
        assert_eq!(prime_factors(2), vec![2]);
        assert_eq!(prime_factors(12), vec![2, 2, 3]);
        assert_eq!(prime_factors(97), vec![97]);
        assert_eq!(prime_factors(1038), vec![2, 3, 173]);
        for n in 2..2000u64 {
            assert_eq!(prime_factors(n).iter().product::<u64>(), n);
        }
    }

    #[test]
    fn pf_reduction_settles() {
        for n in 0..3000 {
            let (chain, total, td) = pf_reduction(n);
            assert!(!chain.is_empty() && chain.len() <= 33, "{n}");
            assert_eq!(td, digit_sum(total));
        }
    }

    #[test]
    fn written_bytes() {
        assert!(is_byte_list("104,105"));
        assert!(is_byte_list(" 0, 1 "));
        assert!(!is_byte_list("162"), "a lone number is a sum");
        assert!(!is_byte_list("a, b"));
        assert!(!is_byte_list("1,256"));
        assert_eq!(&*input_bytes("104,105"), b"hi");
        assert_eq!(&*input_bytes("hi"), b"hi");
        assert_eq!(&*input_bytes("162"), b"162");
        assert_eq!(byte_list(b"hi"), "104,105");
        assert_eq!(parse_u8_list("1, 2,3").unwrap(), vec![1, 2, 3]);
        assert_eq!(parse_u8_list("").unwrap(), Vec::<u8>::new());
        assert!(parse_u8_list("x").is_err());
    }

    #[test]
    fn the_same_story_for_text_and_its_bytes() {
        let text = byte_story(&input_bytes("hi"));
        let bytes = byte_story(&input_bytes("104,105"));
        assert_eq!(text, bytes);
        assert_eq!(text.sums.bytes, 211);
        assert_eq!(text.factored(), "prime");
        assert_eq!(text.lines().len(), 3);
        assert_eq!(byte_story(b"").lines(), vec!["no bytes yet".to_string()]);
        assert_eq!(byte_story(&[1, 2]).factored(), "prime");
        assert_eq!(byte_story(&[3, 3]).factored(), "2 × 2 × 2");
    }

    #[test]
    fn colors_stay_in_range() {
        assert_eq!(hsl(0.0, 0.0, 0.0), [0, 0, 0]);
        assert_eq!(hsl(0.0, 0.0, 1.0), [255, 255, 255]);
        assert_eq!(hsl(0.0, 1.0, 0.5), [255, 0, 0]);
        assert_ne!(color_for_sum(1), color_for_sum(2));
    }

    #[test]
    fn big_powers() {
        assert_eq!(permsq(2, 10), "1024");
        assert_eq!(permsq(10, 0), "1");
        assert_eq!(permsq(256, 2), "65536");
    }
}
