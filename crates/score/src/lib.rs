// This software is licensed under a dual license model:
//
// GNU Affero General Public License v3 (AGPLv3): You may use, modify, and
// distribute this software under the terms of the AGPLv3.
//
// Elastic License v2 (ELv2): You may also use, modify, and distribute this
// software under the Elastic License v2, which has specific restrictions.
//
// We welcome any commercial collaboration or support. For inquiries
// regarding the licenses, please contact us at:
// vectorchord-inquiry@tensorchord.ai
//
// Copyright (c) 2025-2026 TensorChord Inc.

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

#[derive(
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    IntoBytes,
    FromBytes,
    Immutable,
    KnownLayout,
)]
#[repr(transparent)]
pub struct Score(i64);

impl std::fmt::Debug for Score {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_f64())
    }
}

impl Score {
    pub const ZERO: Self = Score::from_f64(0.0f64);
    pub const INFINITY: Self = Score::from_f64(f64::INFINITY);
    pub const NEG_INFINITY: Self = Score::from_f64(f64::NEG_INFINITY);
    pub const NAN: Self = Score::from_f64(f64::NAN);

    #[inline(always)]
    pub const fn from_f64(value: f64) -> Self {
        let bits = value.to_bits() as i64;
        let mask = ((bits >> 63) as u64) >> 1;
        let res = bits ^ (mask as i64);
        Self(res)
    }

    #[inline(always)]
    pub const fn to_f64(self) -> f64 {
        let bits = self.0;
        let mask = ((bits >> 63) as u64) >> 1;
        let res = bits ^ (mask as i64);
        f64::from_bits(res as u64)
    }

    #[inline(always)]
    pub const fn to_i64(self) -> i64 {
        self.0
    }
}

impl From<f64> for Score {
    #[inline(always)]
    fn from(value: f64) -> Self {
        Score::from_f64(value)
    }
}

impl From<Score> for f64 {
    #[inline(always)]
    fn from(value: Score) -> Self {
        Score::to_f64(value)
    }
}

#[test]
fn conversions() {
    assert_eq!(Score::from(0.0f64), Score::ZERO);
    assert_eq!(Score::from(f64::INFINITY), Score::INFINITY);
    assert_eq!(Score::from(f64::NEG_INFINITY), Score::NEG_INFINITY);
    for i in -100..100 {
        let val = (i as f64) * 0.1;
        assert_eq!(f64::from(Score::from(val)).to_bits(), val.to_bits());
    }
    assert_eq!(f64::from(Score::from(0.0f64)).to_bits(), 0.0f64.to_bits());
    assert_eq!(
        f64::from(Score::from(-0.0f64)).to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(
        f64::from(Score::from(f64::NAN)).to_bits(),
        f64::NAN.to_bits()
    );
    assert_eq!(
        f64::from(Score::from(-f64::NAN)).to_bits(),
        (-f64::NAN).to_bits()
    );
    assert_eq!(
        f64::from(Score::from(f64::INFINITY)).to_bits(),
        f64::INFINITY.to_bits()
    );
    assert_eq!(
        f64::from(Score::from(-f64::INFINITY)).to_bits(),
        (-f64::INFINITY).to_bits()
    );
}
