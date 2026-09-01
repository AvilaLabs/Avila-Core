use std::cmp::Ordering;
use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{CORE_S1102, KernelError, Repair, RepairApplicability};

/// Prefix of every refusal raised while decoding an [`ExactNumber`] from a
/// typed document. A compiler that sees this prefix at a JSON Pointer may
/// re-read the raw string there to recover the mechanically safe repair.
pub const EXACT_NUMBER_DECODE_PREFIX: &str = "exact number: ";

const MAX_AUTHORED_BYTES: usize = 16_384;
const MAX_CANONICAL_DIGITS: usize = 16_384;
const MAX_ABSOLUTE_EXPONENT: i64 = 16_384;

/// A reduced exact rational within the draft kernel's explicit 128-bit work
/// budget. Operations refuse overflow instead of rounding or wrapping.
///
/// The canonical interchange remains an unbounded integer/rational string. A
/// future arbitrary-precision implementation can therefore replace this work
/// budget without changing record identity for accepted values.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExactNumber {
    numerator: i128,
    denominator: u128,
}

impl ExactNumber {
    pub fn from_canonical(input: &str) -> Result<Self, KernelError> {
        if input.contains('/') {
            read_authoritative_rational(input)
        } else {
            read_authoritative_decimal(input)
        }
    }

    fn new(numerator: i128, denominator: u128) -> Result<Self, KernelError> {
        if denominator == 0 {
            return Err(invalid_number("rational denominator is zero"));
        }
        if numerator == 0 {
            return Ok(Self {
                numerator: 0,
                denominator: 1,
            });
        }
        let divisor = gcd(numerator.unsigned_abs(), denominator);
        let reduced_denominator = denominator / divisor;
        let divisor = i128::try_from(divisor)
            .map_err(|_| resource_limit("rational reduction exceeds the 128-bit work budget"))?;
        Ok(Self {
            numerator: numerator / divisor,
            denominator: reduced_denominator,
        })
    }

    #[must_use]
    pub const fn numerator(&self) -> i128 {
        self.numerator
    }

    #[must_use]
    pub const fn denominator(&self) -> u128 {
        self.denominator
    }

    #[must_use]
    pub const fn is_zero(&self) -> bool {
        self.numerator == 0
    }

    #[must_use]
    pub const fn is_one(&self) -> bool {
        self.numerator == 1 && self.denominator == 1
    }

    #[must_use]
    pub const fn is_positive(&self) -> bool {
        self.numerator > 0
    }

    pub fn checked_mul(&self, other: &Self) -> Result<Self, KernelError> {
        // Cross-cancel before multiplying to preserve as much work budget as
        // possible without changing the exact result.
        let left_cancel = gcd(self.numerator.unsigned_abs(), other.denominator);
        let right_cancel = gcd(other.numerator.unsigned_abs(), self.denominator);
        let left_divisor = i128::try_from(left_cancel)
            .map_err(|_| resource_limit("exact multiplication exceeds the work budget"))?;
        let right_divisor = i128::try_from(right_cancel)
            .map_err(|_| resource_limit("exact multiplication exceeds the work budget"))?;
        let left_numerator = self.numerator / left_divisor;
        let right_numerator = other.numerator / right_divisor;
        let left_denominator = self.denominator / right_cancel;
        let right_denominator = other.denominator / left_cancel;
        let numerator = left_numerator
            .checked_mul(right_numerator)
            .ok_or_else(|| resource_limit("exact multiplication exceeds the work budget"))?;
        let denominator = left_denominator
            .checked_mul(right_denominator)
            .ok_or_else(|| resource_limit("exact multiplication exceeds the work budget"))?;
        Self::new(numerator, denominator)
    }

    pub fn checked_div(&self, other: &Self) -> Result<Self, KernelError> {
        if other.is_zero() {
            return Err(invalid_number("exact division by zero"));
        }
        let reciprocal_magnitude = i128::try_from(other.denominator)
            .map_err(|_| resource_limit("exact division exceeds the work budget"))?;
        let reciprocal_numerator = if other.numerator.is_negative() {
            -reciprocal_magnitude
        } else {
            reciprocal_magnitude
        };
        let reciprocal = Self::new(reciprocal_numerator, other.numerator.unsigned_abs())?;
        self.checked_mul(&reciprocal)
    }

    pub fn checked_mul_integer(&self, value: i128) -> Result<Self, KernelError> {
        self.checked_mul(&Self::new(value, 1)?)
    }

    pub fn checked_add(&self, other: &Self) -> Result<Self, KernelError> {
        self.checked_add_signed(other, false)
    }

    pub fn checked_sub(&self, other: &Self) -> Result<Self, KernelError> {
        self.checked_add_signed(other, true)
    }

    fn checked_add_signed(&self, other: &Self, subtract: bool) -> Result<Self, KernelError> {
        let common = gcd(self.denominator, other.denominator);
        let left_scale = other.denominator / common;
        let right_scale = self.denominator / common;
        let left_scale = i128::try_from(left_scale)
            .map_err(|_| resource_limit("exact addition exceeds the work budget"))?;
        let right_scale = i128::try_from(right_scale)
            .map_err(|_| resource_limit("exact addition exceeds the work budget"))?;
        let left = self
            .numerator
            .checked_mul(left_scale)
            .ok_or_else(|| resource_limit("exact addition exceeds the work budget"))?;
        let right = other
            .numerator
            .checked_mul(right_scale)
            .ok_or_else(|| resource_limit("exact addition exceeds the work budget"))?;
        let numerator = if subtract {
            left.checked_sub(right)
        } else {
            left.checked_add(right)
        }
        .ok_or_else(|| resource_limit("exact addition exceeds the work budget"))?;
        let denominator = self
            .denominator
            .checked_div(common)
            .and_then(|value| value.checked_mul(other.denominator))
            .ok_or_else(|| resource_limit("exact addition exceeds the work budget"))?;
        Self::new(numerator, denominator)
    }

    pub fn checked_cmp(&self, other: &Self) -> Result<Ordering, KernelError> {
        match (self.numerator.signum(), other.numerator.signum()) {
            (left, right) if left != right => return Ok(left.cmp(&right)),
            (0, 0) => return Ok(Ordering::Equal),
            _ => {}
        }
        let ordering = compare_positive_rationals(
            self.numerator.unsigned_abs(),
            self.denominator,
            other.numerator.unsigned_abs(),
            other.denominator,
        );
        Ok(if self.numerator.is_negative() {
            ordering.reverse()
        } else {
            ordering
        })
    }

    pub fn round_half_even_integer(&self) -> Result<i128, KernelError> {
        let magnitude = self.numerator.unsigned_abs();
        let quotient = magnitude / self.denominator;
        let remainder = magnitude % self.denominator;
        let complement = self.denominator - remainder;
        let increment = remainder > complement || (remainder == complement && quotient % 2 == 1);
        let rounded = quotient
            .checked_add(u128::from(increment))
            .ok_or_else(|| resource_limit("rounded integer exceeds the work budget"))?;
        signed_from_magnitude(rounded, self.numerator.is_negative())
    }

    pub fn to_fixed_decimal(&self, scale: u32) -> Result<String, KernelError> {
        let factor = 10u128
            .checked_pow(scale)
            .ok_or_else(|| resource_limit("decimal display scale exceeds the work budget"))?;
        let factor = i128::try_from(factor)
            .map_err(|_| resource_limit("decimal display scale exceeds the work budget"))?;
        let scaled = self.checked_mul_integer(factor)?;
        if scaled.denominator != 1 {
            return Err(invalid_number(
                "exact value cannot be represented at the requested decimal scale",
            ));
        }

        let negative = scaled.numerator.is_negative();
        let mut digits = scaled.numerator.unsigned_abs().to_string();
        let scale = usize::try_from(scale)
            .map_err(|_| resource_limit("decimal display scale exceeds the work budget"))?;
        if scale > 0 {
            if digits.len() <= scale {
                digits.insert_str(0, &"0".repeat(scale + 1 - digits.len()));
            }
            digits.insert(digits.len() - scale, '.');
        }
        if negative {
            digits.insert(0, '-');
        }
        Ok(digits)
    }

    #[must_use]
    pub fn canonical_rational(&self) -> String {
        if self.denominator == 1 {
            self.numerator.to_string()
        } else {
            format!("{}/{}", self.numerator, self.denominator)
        }
    }
}

impl fmt::Display for ExactNumber {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.canonical_rational())
    }
}

impl Serialize for ExactNumber {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.canonical_rational())
    }
}

impl<'de> Deserialize<'de> for ExactNumber {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(ExactNumberVisitor)
    }
}

struct ExactNumberVisitor;

impl Visitor<'_> for ExactNumberVisitor {
    type Value = ExactNumber;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a canonical decimal or reduced rational string")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        ExactNumber::from_canonical(value)
            .map_err(|error| E::custom(format!("{EXACT_NUMBER_DECODE_PREFIX}{}", error.detail())))
    }
}

/// Reads an already-canonical authoritative decimal.
///
/// A non-canonical but well-formed decimal is refused with a mechanically safe
/// repair naming its unique canonical form; the reader never rewrites it.
pub fn read_authoritative_decimal(input: &str) -> Result<ExactNumber, KernelError> {
    let lowered = lower_authored_decimal(input)?;
    if lowered != input {
        return Err(KernelError::with_repair(
            CORE_S1102,
            "authoritative decimal is not canonical",
            canonical_form_repair(lowered),
        ));
    }
    decimal_to_ratio(input)
}

fn canonical_form_repair(canonical: String) -> Repair {
    Repair {
        applicability: RepairApplicability::MechanicallySafe,
        candidates: vec![canonical],
    }
}

/// Lowers an authored decimal, including exponent notation, to the canonical
/// plain-decimal form defined by ADR-0006 SC-2.
pub fn lower_authored_decimal(input: &str) -> Result<String, KernelError> {
    if input.is_empty() || input.len() > MAX_AUTHORED_BYTES || !input.is_ascii() {
        return Err(invalid_number(
            "decimal is empty, non-ASCII, or exceeds the resource limit",
        ));
    }

    let bytes = input.as_bytes();
    let mut cursor = 0;
    let negative = match bytes.first() {
        Some(b'-') => {
            cursor += 1;
            true
        }
        Some(b'+') => return Err(invalid_number("a leading plus sign is not permitted")),
        _ => false,
    };

    let mantissa_start = cursor;
    let mut decimal_index = None;
    let mut exponent_index = None;
    let mut digit_count = 0usize;

    while cursor < bytes.len() {
        match bytes[cursor] {
            b'0'..=b'9' => digit_count += 1,
            b'.' if decimal_index.is_none() && exponent_index.is_none() => {
                decimal_index = Some(cursor);
            }
            b'e' | b'E' if exponent_index.is_none() => {
                exponent_index = Some(cursor);
                break;
            }
            _ => return Err(invalid_number("invalid decimal syntax")),
        }
        cursor += 1;
    }

    let mantissa_end = exponent_index.unwrap_or(bytes.len());
    if digit_count == 0
        || mantissa_end == mantissa_start
        || decimal_index == Some(mantissa_start)
        || decimal_index == Some(mantissa_end.saturating_sub(1))
    {
        return Err(invalid_number(
            "decimal mantissa must contain digits on each written side",
        ));
    }

    let exponent = if let Some(index) = exponent_index {
        parse_exponent(&input[index + 1..])?
    } else {
        0
    };

    let mut digits = String::with_capacity(digit_count);
    for character in input[mantissa_start..mantissa_end].chars() {
        if character != '.' {
            digits.push(character);
        }
    }

    let digits_before_decimal = decimal_index.map_or(mantissa_end - mantissa_start, |index| {
        index - mantissa_start
    });
    let decimal_position = i64::try_from(digits_before_decimal)
        .map_err(|_| invalid_number("decimal position exceeds the resource limit"))?
        .checked_add(exponent)
        .ok_or_else(|| invalid_number("decimal exponent exceeds the resource limit"))?;

    let canonical = place_decimal(&digits, decimal_position, negative)?;
    if canonical.len() > MAX_CANONICAL_DIGITS + 2 {
        return Err(invalid_number(
            "canonical decimal exceeds the resource limit",
        ));
    }
    Ok(canonical)
}

/// Reads an already-canonical rational or integer string.
pub fn read_authoritative_rational(input: &str) -> Result<ExactNumber, KernelError> {
    if input.is_empty() || input.len() > MAX_AUTHORED_BYTES || !input.is_ascii() {
        return Err(invalid_number(
            "rational is empty, non-ASCII, or exceeds the resource limit",
        ));
    }

    let mut parts = input.split('/');
    let numerator_text = parts.next().unwrap_or_default();
    let denominator_text = parts.next();
    if parts.next().is_some() {
        return Err(invalid_number("a rational contains at most one slash"));
    }

    let numerator = parse_signed_integer(numerator_text)?;
    let value = if let Some(denominator_text) = denominator_text {
        let denominator = parse_unsigned_integer(denominator_text)?;
        ExactNumber::new(numerator, denominator)?
    } else {
        ExactNumber::new(numerator, 1)?
    };

    let canonical = value.canonical_rational();
    if canonical != input {
        return Err(KernelError::with_repair(
            CORE_S1102,
            "authoritative rational is not reduced and canonical",
            canonical_form_repair(canonical),
        ));
    }
    Ok(value)
}

fn parse_exponent(input: &str) -> Result<i64, KernelError> {
    if input.is_empty() {
        return Err(invalid_number("decimal exponent is missing"));
    }
    let (negative, digits) = match input.as_bytes().first() {
        Some(b'+') => (false, &input[1..]),
        Some(b'-') => (true, &input[1..]),
        _ => (false, input),
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid_number("invalid decimal exponent"));
    }
    let magnitude: i64 = digits
        .parse()
        .map_err(|_| invalid_number("decimal exponent exceeds the resource limit"))?;
    if magnitude > MAX_ABSOLUTE_EXPONENT {
        return Err(invalid_number(
            "decimal exponent exceeds the resource limit",
        ));
    }
    Ok(if negative { -magnitude } else { magnitude })
}

fn place_decimal(
    digits: &str,
    decimal_position: i64,
    negative: bool,
) -> Result<String, KernelError> {
    let significant = digits.trim_start_matches('0');
    if significant.is_empty() {
        return Ok("0".into());
    }

    let removed_leading = digits.len() - significant.len();
    let adjusted_position = decimal_position
        .checked_sub(
            i64::try_from(removed_leading)
                .map_err(|_| invalid_number("decimal exceeds the resource limit"))?,
        )
        .ok_or_else(|| invalid_number("decimal exceeds the resource limit"))?;
    let digit_count = i64::try_from(significant.len())
        .map_err(|_| invalid_number("decimal exceeds the resource limit"))?;
    let mut unsigned = String::new();

    if adjusted_position <= 0 {
        unsigned.push_str("0.");
        let zero_count = usize::try_from(-adjusted_position)
            .map_err(|_| invalid_number("decimal exceeds the resource limit"))?;
        if zero_count > MAX_CANONICAL_DIGITS {
            return Err(invalid_number(
                "canonical decimal exceeds the resource limit",
            ));
        }
        unsigned.extend(std::iter::repeat_n('0', zero_count));
        unsigned.push_str(significant);
    } else if adjusted_position >= digit_count {
        unsigned.push_str(significant);
        let zero_count = usize::try_from(adjusted_position - digit_count)
            .map_err(|_| invalid_number("decimal exceeds the resource limit"))?;
        if unsigned.len().saturating_add(zero_count) > MAX_CANONICAL_DIGITS {
            return Err(invalid_number(
                "canonical decimal exceeds the resource limit",
            ));
        }
        unsigned.extend(std::iter::repeat_n('0', zero_count));
    } else {
        let split = usize::try_from(adjusted_position)
            .map_err(|_| invalid_number("decimal exceeds the resource limit"))?;
        unsigned.push_str(&significant[..split]);
        unsigned.push('.');
        unsigned.push_str(&significant[split..]);
    }

    if let Some((integer, fraction)) = unsigned.split_once('.') {
        let fraction = fraction.trim_end_matches('0');
        unsigned = if fraction.is_empty() {
            integer.into()
        } else {
            format!("{integer}.{fraction}")
        };
    }

    if negative {
        unsigned.insert(0, '-');
    }
    Ok(unsigned)
}

fn decimal_to_ratio(input: &str) -> Result<ExactNumber, KernelError> {
    let negative = input.starts_with('-');
    let unsigned = input.strip_prefix('-').unwrap_or(input);
    let (integer, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let joined = format!("{integer}{fraction}");
    let magnitude = parse_unsigned_magnitude(&joined)?;
    let numerator = signed_from_magnitude(magnitude, negative)?;
    let exponent = u32::try_from(fraction.len())
        .map_err(|_| resource_limit("decimal scale exceeds the 128-bit work budget"))?;
    let denominator = 10u128
        .checked_pow(exponent)
        .ok_or_else(|| resource_limit("decimal scale exceeds the 128-bit work budget"))?;
    ExactNumber::new(numerator, denominator)
}

fn parse_signed_integer(input: &str) -> Result<i128, KernelError> {
    let (negative, digits) = match input.strip_prefix('-') {
        Some(digits) => (true, digits),
        None => (false, input),
    };
    validate_integer_digits(digits, negative)?;
    signed_from_magnitude(parse_unsigned_magnitude(digits)?, negative)
}

fn parse_unsigned_integer(input: &str) -> Result<u128, KernelError> {
    validate_integer_digits(input, false)?;
    parse_unsigned_magnitude(input)
}

fn validate_integer_digits(digits: &str, negative: bool) -> Result<(), KernelError> {
    if digits.is_empty()
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
        || (digits.len() > 1 && digits.starts_with('0'))
        || (negative && digits == "0")
    {
        return Err(invalid_number("integer is not canonical"));
    }
    Ok(())
}

fn parse_unsigned_magnitude(input: &str) -> Result<u128, KernelError> {
    input
        .parse()
        .map_err(|_| resource_limit("exact value exceeds the 128-bit work budget"))
}

fn signed_from_magnitude(magnitude: u128, negative: bool) -> Result<i128, KernelError> {
    if negative {
        if magnitude == i128::MAX as u128 + 1 {
            Ok(i128::MIN)
        } else {
            let magnitude = i128::try_from(magnitude)
                .map_err(|_| resource_limit("exact value exceeds the 128-bit work budget"))?;
            Ok(-magnitude)
        }
    } else {
        i128::try_from(magnitude)
            .map_err(|_| resource_limit("exact value exceeds the 128-bit work budget"))
    }
}

fn gcd(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn compare_positive_rationals(
    mut left_numerator: u128,
    mut left_denominator: u128,
    mut right_numerator: u128,
    mut right_denominator: u128,
) -> Ordering {
    let mut reversed = false;
    loop {
        let left_quotient = left_numerator / left_denominator;
        let right_quotient = right_numerator / right_denominator;
        if left_quotient != right_quotient {
            let ordering = left_quotient.cmp(&right_quotient);
            return if reversed {
                ordering.reverse()
            } else {
                ordering
            };
        }

        let left_remainder = left_numerator % left_denominator;
        let right_remainder = right_numerator % right_denominator;
        match (left_remainder == 0, right_remainder == 0) {
            (true, true) => return Ordering::Equal,
            (true, false) => {
                return if reversed {
                    Ordering::Greater
                } else {
                    Ordering::Less
                };
            }
            (false, true) => {
                return if reversed {
                    Ordering::Less
                } else {
                    Ordering::Greater
                };
            }
            (false, false) => {
                left_numerator = left_denominator;
                left_denominator = left_remainder;
                right_numerator = right_denominator;
                right_denominator = right_remainder;
                reversed = !reversed;
            }
        }
    }
}

fn invalid_number(detail: impl Into<String>) -> KernelError {
    KernelError::new(CORE_S1102, detail)
}

fn resource_limit(detail: impl Into<String>) -> KernelError {
    KernelError::new(CORE_S1102, detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparison_does_not_cross_multiply() {
        let left = ExactNumber::new(i128::MAX - 1, u128::MAX - 1).unwrap();
        let right = ExactNumber::new(i128::MAX - 2, u128::MAX - 1).unwrap();
        assert_eq!(left.checked_cmp(&right).unwrap(), Ordering::Greater);
    }

    #[test]
    fn arithmetic_fails_closed_on_overflow() {
        let maximum = ExactNumber::new(i128::MAX, 1).unwrap();
        let two = ExactNumber::new(2, 1).unwrap();
        assert_eq!(maximum.checked_mul(&two).unwrap_err().code(), CORE_S1102);
        assert_eq!(maximum.checked_add(&two).unwrap_err().code(), CORE_S1102);
    }

    #[test]
    fn continued_fraction_comparison_matches_cross_products_in_small_domain() {
        for left_numerator in -20i128..=20 {
            for left_denominator in 1u128..=20 {
                for right_numerator in -20i128..=20 {
                    for right_denominator in 1u128..=20 {
                        let left = ExactNumber::new(left_numerator, left_denominator).unwrap();
                        let right = ExactNumber::new(right_numerator, right_denominator).unwrap();
                        let cross_left = left_numerator * right_denominator as i128;
                        let cross_right = right_numerator * left_denominator as i128;
                        assert_eq!(
                            left.checked_cmp(&right).unwrap(),
                            cross_left.cmp(&cross_right),
                            "{left_numerator}/{left_denominator} vs {right_numerator}/{right_denominator}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn noncanonical_rationals_are_refused() {
        for invalid in ["+1", "01", "-0", "1/1", "0/2", "2/4", "1/-2", "1/0"] {
            assert_eq!(
                read_authoritative_rational(invalid).unwrap_err().code(),
                CORE_S1102,
                "{invalid}"
            );
        }
    }

    #[test]
    fn noncanonical_numbers_carry_their_unique_canonical_form_as_a_safe_repair() {
        for (authored, canonical) in [
            ("100.0", "100"),
            ("1e2", "100"),
            ("0001.2300", "1.23"),
            ("01", "1"),
            ("-0.0", "0"),
            ("2/4", "1/2"),
            ("1/1", "1"),
            ("0/2", "0"),
        ] {
            let error = ExactNumber::from_canonical(authored).unwrap_err();
            assert_eq!(error.code(), CORE_S1102, "{authored}");
            let repair = error
                .repair()
                .unwrap_or_else(|| panic!("{authored} has a repair"));
            assert_eq!(repair.applicability, RepairApplicability::MechanicallySafe);
            assert_eq!(repair.candidates, vec![canonical.to_owned()], "{authored}");
        }
        for malformed in ["+1", "abc", "1/-2", "1/0", "NaN", ""] {
            let error = ExactNumber::from_canonical(malformed).unwrap_err();
            assert_eq!(error.code(), CORE_S1102, "{malformed}");
            assert!(
                error.repair().is_none(),
                "{malformed} must not offer a repair"
            );
        }
    }

    #[test]
    fn authored_decimal_lowering_is_exact_and_canonical() {
        for (authored, expected) in [
            ("0001.2300", "1.23"),
            ("0.00120", "0.0012"),
            ("1e3", "1000"),
            ("1e-3", "0.001"),
            ("1000e-2", "10"),
            ("-000.000", "0"),
        ] {
            assert_eq!(lower_authored_decimal(authored).unwrap(), expected);
        }
    }

    #[test]
    fn half_even_rounding_and_fixed_display_are_exact() {
        for (value, rounded) in [
            ("5/2", 2),
            ("7/2", 4),
            ("-5/2", -2),
            ("-7/2", -4),
            ("251/100", 3),
            ("249/100", 2),
        ] {
            assert_eq!(
                ExactNumber::from_canonical(value)
                    .unwrap()
                    .round_half_even_integer()
                    .unwrap(),
                rounded
            );
        }
        assert_eq!(
            ExactNumber::from_canonical("25")
                .unwrap()
                .to_fixed_decimal(1)
                .unwrap(),
            "25.0"
        );
        assert_eq!(
            ExactNumber::from_canonical("1/200")
                .unwrap()
                .to_fixed_decimal(3)
                .unwrap(),
            "0.005"
        );
    }
}
