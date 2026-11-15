//! Pure-Rust Arbitrary Precision Integer Engine for ECMAScript BigInt (GOAL-48).

use std::cmp::Ordering;
use std::fmt;

/// Arbitrary-precision integer adhering to ECMAScript BigInt semantics.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct JsBigInt {
    /// True for non-negative (>= 0), false for negative (< 0).
    pub is_positive: bool,
    /// Little-endian base 2^64 limbs. Normalized such that the most significant
    /// limb is non-zero, unless the value is zero, where limbs is empty.
    pub limbs: Vec<u64>,
}

impl JsBigInt {
    /// Creates a BigInt representing zero.
    pub const fn zero() -> Self {
        Self {
            is_positive: true,
            limbs: Vec::new(),
        }
    }

    /// Creates a BigInt representing one.
    pub fn one() -> Self {
        Self {
            is_positive: true,
            limbs: vec![1],
        }
    }

    /// Checks if this BigInt is zero.
    pub fn is_zero(&self) -> bool {
        self.limbs.is_empty() || self.limbs.iter().all(|&l| l == 0)
    }

    /// Creates a BigInt from a signed 64-bit integer.
    pub fn from_i64(val: i64) -> Self {
        if val == 0 {
            return Self::zero();
        }
        let is_positive = val >= 0;
        let abs = if val == i64::MIN {
            (i64::MAX as u64) + 1
        } else {
            val.unsigned_abs()
        };
        Self {
            is_positive,
            limbs: vec![abs],
        }
    }

    /// Creates a BigInt from an unsigned 64-bit integer.
    pub fn from_u64(val: u64) -> Self {
        if val == 0 {
            Self::zero()
        } else {
            Self {
                is_positive: true,
                limbs: vec![val],
            }
        }
    }

    /// Creates a BigInt from a signed 128-bit integer.
    pub fn from_i128(val: i128) -> Self {
        if val == 0 {
            return Self::zero();
        }
        let is_positive = val >= 0;
        let abs = if val == i128::MIN {
            (i128::MAX as u128) + 1
        } else {
            val.unsigned_abs()
        };
        Self::from_u128(abs, is_positive)
    }

    /// Creates a BigInt from an unsigned 128-bit integer and sign.
    pub fn from_u128(abs: u128, is_positive: bool) -> Self {
        if abs == 0 {
            return Self::zero();
        }
        let low = abs as u64;
        let high = (abs >> 64) as u64;
        let mut limbs = vec![low];
        if high > 0 {
            limbs.push(high);
        }
        Self { is_positive, limbs }
    }

    /// Parses a string into a BigInt, supporting optional sign, base prefixes
    /// (0x, 0b, 0o), and optional trailing 'n'.
    pub fn parse(s: &str) -> Result<Self, String> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err("SyntaxError: Cannot convert empty string to a BigInt".to_string());
        }

        let without_suffix = trimmed.strip_suffix('n').unwrap_or(trimmed);
        if without_suffix.is_empty() {
            return Err("SyntaxError: Invalid BigInt literal".to_string());
        }

        let (is_positive, body) = if let Some(rest) = without_suffix.strip_prefix('-') {
            (false, rest)
        } else if let Some(rest) = without_suffix.strip_prefix('+') {
            (true, rest)
        } else {
            (true, without_suffix)
        };

        if body.is_empty() {
            return Err("SyntaxError: Invalid BigInt literal".to_string());
        }

        let (radix, digits) =
            if let Some(hex) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
                (16u32, hex)
            } else if let Some(bin) = body.strip_prefix("0b").or_else(|| body.strip_prefix("0B")) {
                (2u32, bin)
            } else if let Some(oct) = body.strip_prefix("0o").or_else(|| body.strip_prefix("0O")) {
                (8u32, oct)
            } else {
                (10u32, body)
            };

        if digits.is_empty() {
            return Err("SyntaxError: Invalid BigInt literal".to_string());
        }

        let mut result = Self::zero();
        let radix_big = Self::from_u64(radix as u64);

        for ch in digits.chars() {
            if ch == '_' {
                continue; // Support numeric separators (e.g. 1_000n)
            }
            let digit = ch.to_digit(radix).ok_or_else(|| {
                format!("SyntaxError: Invalid character '{ch}' in BigInt literal")
            })?;
            result = result.mul_abs(&radix_big);
            result = result.add_abs(&Self::from_u64(digit as u64));
        }

        result.is_positive = if result.is_zero() { true } else { is_positive };
        result.normalize();
        Ok(result)
    }

    /// Normalizes internal representation by stripping trailing zero limbs.
    fn normalize(&mut self) {
        while let Some(&0) = self.limbs.last() {
            self.limbs.pop();
        }
        if self.limbs.is_empty() {
            self.is_positive = true;
        }
    }

    /// Compares absolute magnitudes.
    pub fn cmp_abs(&self, other: &Self) -> Ordering {
        if self.limbs.len() != other.limbs.len() {
            return self.limbs.len().cmp(&other.limbs.len());
        }
        for (a, b) in self.limbs.iter().rev().zip(other.limbs.iter().rev()) {
            if a != b {
                return a.cmp(b);
            }
        }
        Ordering::Equal
    }

    /// Adds absolute values.
    fn add_abs(&self, other: &Self) -> Self {
        let max_len = self.limbs.len().max(other.limbs.len());
        let mut limbs = Vec::with_capacity(max_len + 1);
        let mut carry = 0u64;

        for i in 0..max_len {
            let a = *self.limbs.get(i).unwrap_or(&0);
            let b = *other.limbs.get(i).unwrap_or(&0);
            let sum = (a as u128) + (b as u128) + (carry as u128);
            limbs.push(sum as u64);
            carry = (sum >> 64) as u64;
        }
        if carry > 0 {
            limbs.push(carry);
        }

        let mut res = Self {
            is_positive: true,
            limbs,
        };
        res.normalize();
        res
    }

    /// Subtracts absolute values (assumes self >= other).
    fn sub_abs(&self, other: &Self) -> Self {
        let mut limbs = Vec::with_capacity(self.limbs.len());
        let mut borrow = 0i128;

        for i in 0..self.limbs.len() {
            let a = *self.limbs.get(i).unwrap_or(&0) as i128;
            let b = *other.limbs.get(i).unwrap_or(&0) as i128;
            let diff = a - b - borrow;
            if diff < 0 {
                limbs.push((diff + (1i128 << 64)) as u64);
                borrow = 1;
            } else {
                limbs.push(diff as u64);
                borrow = 0;
            }
        }

        let mut res = Self {
            is_positive: true,
            limbs,
        };
        res.normalize();
        res
    }

    /// Multiplies absolute values.
    fn mul_abs(&self, other: &Self) -> Self {
        if self.is_zero() || other.is_zero() {
            return Self::zero();
        }
        let mut limbs = vec![0u64; self.limbs.len() + other.limbs.len()];

        for (i, &a) in self.limbs.iter().enumerate() {
            let mut carry = 0u128;
            for (j, &b) in other.limbs.iter().enumerate() {
                let cur = limbs[i + j] as u128;
                let prod = (a as u128) * (b as u128) + cur + carry;
                limbs[i + j] = prod as u64;
                carry = prod >> 64;
            }
            if carry > 0 {
                limbs[i + other.limbs.len()] += carry as u64;
            }
        }

        let mut res = Self {
            is_positive: true,
            limbs,
        };
        res.normalize();
        res
    }

    /// Adds two BigInts respecting signs.
    pub fn add(&self, other: &Self) -> Self {
        if self.is_positive == other.is_positive {
            let mut res = self.add_abs(other);
            res.is_positive = self.is_positive;
            res
        } else {
            match self.cmp_abs(other) {
                Ordering::Equal => Self::zero(),
                Ordering::Greater => {
                    let mut res = self.sub_abs(other);
                    res.is_positive = self.is_positive;
                    res
                }
                Ordering::Less => {
                    let mut res = other.sub_abs(self);
                    res.is_positive = other.is_positive;
                    res
                }
            }
        }
    }

    /// Negates a BigInt.
    pub fn neg(&self) -> Self {
        if self.is_zero() {
            self.clone()
        } else {
            let mut res = self.clone();
            res.is_positive = !res.is_positive;
            res
        }
    }

    /// Subtracts two BigInts respecting signs.
    pub fn sub(&self, other: &Self) -> Self {
        self.add(&other.neg())
    }

    /// Multiplies two BigInts respecting signs.
    pub fn mul(&self, other: &Self) -> Self {
        if self.is_zero() || other.is_zero() {
            return Self::zero();
        }
        let mut res = self.mul_abs(other);
        res.is_positive = self.is_positive == other.is_positive;
        res
    }

    /// Divides and computes remainder of absolute values using binary restoring division.
    fn div_rem_abs(&self, other: &Self) -> Result<(Self, Self), String> {
        if other.is_zero() {
            return Err("RangeError: Division by zero".to_string());
        }
        if self.cmp_abs(other) == Ordering::Less {
            return Ok((Self::zero(), self.clone()));
        }

        let mut quotient = Self::zero();
        let mut remainder = Self::zero();

        // Total bits in self
        let total_bits = self.bit_length();

        for bit_idx in (0..total_bits).rev() {
            remainder = remainder.shl(1);
            if self.test_bit(bit_idx) {
                remainder = remainder.add_abs(&Self::one());
            }

            if remainder.cmp_abs(other) != Ordering::Less {
                remainder = remainder.sub_abs(other);
                quotient.set_bit(bit_idx);
            }
        }

        quotient.normalize();
        remainder.normalize();
        Ok((quotient, remainder))
    }

    /// Integer division.
    pub fn div(&self, other: &Self) -> Result<Self, String> {
        let (mut q, _) = self.div_rem_abs(other)?;
        if !q.is_zero() {
            q.is_positive = self.is_positive == other.is_positive;
        }
        Ok(q)
    }

    /// Remainder operation.
    pub fn rem(&self, other: &Self) -> Result<Self, String> {
        let (_, mut r) = self.div_rem_abs(other)?;
        if !r.is_zero() {
            r.is_positive = self.is_positive;
        }
        Ok(r)
    }

    /// Exponentiation (pow) adhering to ECMAScript standard.
    pub fn pow(&self, exp: &Self) -> Result<Self, String> {
        if !exp.is_positive && !exp.is_zero() {
            return Err("RangeError: Exponent must be non-negative".to_string());
        }
        if exp.is_zero() {
            return Ok(Self::one());
        }
        if self.is_zero() {
            return Ok(Self::zero());
        }

        let mut base = self.clone();
        let mut exponent = exp.clone();
        let mut result = Self::one();

        while !exponent.is_zero() {
            if exponent.test_bit(0) {
                result = result.mul(&base);
            }
            base = base.mul(&base);
            exponent = exponent.shr(1);
        }

        Ok(result)
    }

    /// Bitwise NOT (~).
    pub fn bit_not(&self) -> Self {
        // In ECMAScript: ~x === -(x + 1)
        let one = Self::one();
        let plus_one = self.add(&one);
        let mut res = plus_one;
        if !res.is_zero() {
            res.is_positive = !res.is_positive;
        }
        res
    }

    /// Bitwise shift left.
    pub fn shl(&self, shift: usize) -> Self {
        if self.is_zero() || shift == 0 {
            return self.clone();
        }
        let limb_shift = shift / 64;
        let bit_shift = (shift % 64) as u32;

        let mut limbs = vec![0u64; limb_shift];
        let mut carry = 0u64;

        for &limb in &self.limbs {
            let shifted = if bit_shift == 0 {
                limb
            } else {
                (limb << bit_shift) | carry
            };
            limbs.push(shifted);
            carry = if bit_shift == 0 {
                0
            } else {
                limb >> (64 - bit_shift)
            };
        }
        if carry > 0 {
            limbs.push(carry);
        }

        let mut res = Self {
            is_positive: self.is_positive,
            limbs,
        };
        res.normalize();
        res
    }

    /// Bitwise shift right.
    pub fn shr(&self, shift: usize) -> Self {
        if self.is_zero() || shift == 0 {
            return self.clone();
        }
        let limb_shift = shift / 64;
        let bit_shift = (shift % 64) as u32;

        if limb_shift >= self.limbs.len() {
            return if self.is_positive {
                Self::zero()
            } else {
                Self::from_i64(-1)
            };
        }

        let mut limbs = Vec::new();
        let source = &self.limbs[limb_shift..];

        for i in 0..source.len() {
            let cur = source[i];
            let next = source.get(i + 1).copied().unwrap_or(0);
            let shifted = if bit_shift == 0 {
                cur
            } else {
                (cur >> bit_shift) | (next << (64 - bit_shift))
            };
            limbs.push(shifted);
        }

        let mut res = Self {
            is_positive: self.is_positive,
            limbs,
        };
        res.normalize();
        res
    }

    /// Bit length (index of highest set bit + 1).
    pub fn bit_length(&self) -> usize {
        if let Some(&last) = self.limbs.last() {
            (self.limbs.len() - 1) * 64 + (64 - last.leading_zeros() as usize)
        } else {
            0
        }
    }

    /// Converts non-negative single limb BigInt to u64 if fits.
    pub fn to_u64(&self) -> Option<u64> {
        if !self.is_positive {
            return None;
        }
        if self.limbs.is_empty() {
            Some(0)
        } else if self.limbs.len() == 1 {
            Some(self.limbs[0])
        } else {
            None
        }
    }

    /// Bitwise AND (&).
    pub fn bit_and(&self, other: &Self) -> Self {
        let max_len = self.limbs.len().min(other.limbs.len());
        let mut limbs = Vec::with_capacity(max_len);
        for i in 0..max_len {
            limbs.push(self.limbs[i] & other.limbs[i]);
        }
        let is_positive = self.is_positive && other.is_positive;
        let mut res = Self { is_positive, limbs };
        res.normalize();
        res
    }

    /// Bitwise OR (|).
    pub fn bit_or(&self, other: &Self) -> Self {
        let max_len = self.limbs.len().max(other.limbs.len());
        let mut limbs = Vec::with_capacity(max_len);
        for i in 0..max_len {
            let a = self.limbs.get(i).copied().unwrap_or(0);
            let b = other.limbs.get(i).copied().unwrap_or(0);
            limbs.push(a | b);
        }
        let is_positive = self.is_positive || other.is_positive;
        let mut res = Self { is_positive, limbs };
        res.normalize();
        res
    }

    /// Bitwise XOR (^).
    pub fn bit_xor(&self, other: &Self) -> Self {
        let max_len = self.limbs.len().max(other.limbs.len());
        let mut limbs = Vec::with_capacity(max_len);
        for i in 0..max_len {
            let a = self.limbs.get(i).copied().unwrap_or(0);
            let b = other.limbs.get(i).copied().unwrap_or(0);
            limbs.push(a ^ b);
        }
        let is_positive = self.is_positive == other.is_positive;
        let mut res = Self { is_positive, limbs };
        res.normalize();
        res
    }

    /// Tests if a bit at `index` is 1.
    pub fn test_bit(&self, index: usize) -> bool {
        let limb_idx = index / 64;
        let bit_idx = index % 64;
        if let Some(&limb) = self.limbs.get(limb_idx) {
            (limb & (1u64 << bit_idx)) != 0
        } else {
            false
        }
    }

    /// Sets a bit at `index` to 1.
    pub fn set_bit(&mut self, index: usize) {
        let limb_idx = index / 64;
        let bit_idx = index % 64;
        if limb_idx >= self.limbs.len() {
            self.limbs.resize(limb_idx + 1, 0);
        }
        self.limbs[limb_idx] |= 1u64 << bit_idx;
    }

    /// Converts to standard decimal string representation.
    pub fn to_decimal_string(&self) -> String {
        if self.is_zero() {
            return "0".to_string();
        }

        let mut temp = self.clone();
        temp.is_positive = true;
        let ten = Self::from_u64(10);
        let mut digits = Vec::new();

        while !temp.is_zero() {
            let (q, r) = temp.div_rem_abs(&ten).unwrap();
            let d = r.limbs.first().copied().unwrap_or(0);
            digits.push((b'0' + d as u8) as char);
            temp = q;
        }

        if !self.is_positive {
            digits.push('-');
        }

        digits.iter().rev().collect()
    }
}

impl PartialOrd for JsBigInt {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for JsBigInt {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.is_positive != other.is_positive {
            return if self.is_positive {
                Ordering::Greater
            } else {
                Ordering::Less
            };
        }
        let mag = self.cmp_abs(other);
        if self.is_positive {
            mag
        } else {
            mag.reverse()
        }
    }
}

impl fmt::Debug for JsBigInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}n", self.to_decimal_string())
    }
}

impl fmt::Display for JsBigInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_decimal_string())
    }
}

impl std::str::FromStr for JsBigInt {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}
