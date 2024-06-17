use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

/// Fixed-point unsigned decimal amount with four decimal digits.
///
/// Can store values no greater than u64::MAX / 10000, that is,
/// 1,844,674,407,370,955.1615.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Amount(u64);

// An alternative approach would be using a "BigInt", a variable size integer
// type that can store arbitrarily large numbers. We'd avoid dealing with
// overflow at the cost of possibly increased memory usage.
// This is more fun, though!

impl Amount {
    pub fn checked_add(self, other: Amount) -> Option<Amount> {
        self.0.checked_add(other.0).map(Amount)
    }

    pub fn checked_sub(self, other: Amount) -> Option<Amount> {
        self.0.checked_sub(other.0).map(Amount)
    }
}

impl std::fmt::Display for Amount {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        // Always write out all four decimal digits, even if they are zero. That
        // makes this code simpler, and as a bonus makes it easier for users to
        // parse.
        write!(f, "{}.{:0>4}", self.0 / 10000, self.0 % 10000)
    }
}

impl Serialize for Amount {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.collect_str(&self)
    }
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum AmountParseError {
    #[error("invalid format")]
    InvalidFormat,
    #[error("value too large")]
    TooLarge,
}

impl TryFrom<&str> for Amount {
    type Error = AmountParseError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(\d+)(?:\.(\d{1,4}))?$").unwrap());
        let captures = RE.captures(s).ok_or(AmountParseError::InvalidFormat)?;

        // If the regex matched, the captures are guaranteed to be integers. The
        // only thing that can go wrong is that the integer is too large to fit
        // in a u64. Anything else is a developer error, so we panic.
        let integer = captures
            .get(1)
            .unwrap()
            .as_str()
            .parse::<u64>()
            .map_err(|e| match e.kind() {
                std::num::IntErrorKind::PosOverflow => AmountParseError::TooLarge,
                _ => panic!("unexpected error: {:?}", e),
            })?;
        let decimal = captures
            .get(2)
            .map(|s| parse_decimal_part(s.as_str()))
            .unwrap_or(0);

        integer
            .checked_mul(10000)
            .and_then(|integer| integer.checked_add(decimal))
            .map(Amount)
            .ok_or(AmountParseError::TooLarge)
    }
}

/// Parse an up to four digit fractional part into a u64 between 0 and 9999.
/// For example, parse "1" into 1000, "123" into 1230, and "1234" into 1234.
fn parse_decimal_part(s: &str) -> u64 {
    assert!(s.len() <= 4);
    // We don't need to worry about overflow in the cast or a negative exponent
    // because we know 0 <= s.len() <= 4.
    s.parse::<u64>().unwrap() * (10u64.pow(4 - s.len() as u32))
}

impl<'de> Deserialize<'de> for Amount {
    fn deserialize<D>(deserializer: D) -> Result<Amount, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let s: &str = Deserialize::deserialize(deserializer)?;
        s.try_into().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case("1234.5678")]
    #[test_case("1234.0001")]
    #[test_case("1234.1000")]
    #[test_case("0.5678")]
    fn test_round_trip(s: &str) {
        assert_eq!(Amount::try_from(s).unwrap().to_string(), s);
    }

    #[test]
    fn test_fewer_decimal_digits() {
        // As above, we always write out all four decimal digits. This isn't a
        // requirement, but this test documents the current behaviour.
        assert_eq!(Amount::try_from("1234.1").unwrap().to_string(), "1234.1000");
    }

    #[test]
    fn test_no_decimal_digits() {
        assert_eq!(Amount::try_from("1234").unwrap().to_string(), "1234.0000");
    }

    #[test]
    fn test_leading_zeroes() {
        // We choose to be permissive and allow leading zeroes, though they are
        // dropped when serializing.
        let amount = Amount::try_from("0001234.0005").unwrap();
        assert_eq!(amount, Amount::try_from("1234.0005").unwrap());
        assert_eq!(amount.to_string(), "1234.0005");
    }

    #[test_case("a"; "non-number integer part")]
    #[test_case("0.a"; "non-number decimal part")]
    #[test_case("1234."; "dot without decimal part")]
    #[test_case("0.12345"; "too many decimal digits")]
    fn test_invalid_format(s: &str) {
        assert_eq!(Amount::try_from(s), Err(AmountParseError::InvalidFormat));
    }

    #[test]
    fn test_max_value() {
        let mut s = u64::MAX.to_string();
        s.insert(s.len() - 4, '.');
        Amount::try_from(s.as_str()).unwrap();
    }

    #[test]
    fn test_too_large() {
        let mut s = (u64::MAX as u128 + 1).to_string();
        s.insert(s.len() - 4, '.');
        assert_eq!(
            Amount::try_from(s.as_str()),
            Err(AmountParseError::TooLarge)
        );
    }

    #[test_case("1", 1000)]
    #[test_case("12", 1200)]
    #[test_case("123", 1230)]
    #[test_case("1234", 1234)]
    fn test_parse_decimal_part(s: &str, expected: u64) {
        assert_eq!(parse_decimal_part(s), expected);
    }
}
