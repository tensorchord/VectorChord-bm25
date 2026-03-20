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
// Copyright (c) 2025 TensorChord Inc.

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
pub struct U48([u16; 3]);

impl std::fmt::Debug for U48 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = (self.0[0] as u64) << 32 | (self.0[1] as u64) << 16 | self.0[2] as u64;
        write!(f, "{value}",)
    }
}

impl U48 {
    pub const ZERO: Self = U48::from_array([0, 0, 0]);
    pub const MAX: Self = U48::from_array([u16::MAX, u16::MAX, u16::MAX]);

    #[inline(always)]
    pub const fn from_array(array: [u16; 3]) -> Self {
        Self(array)
    }

    #[inline(always)]
    pub const fn to_array(self) -> [u16; 3] {
        self.0
    }

    #[inline(always)]
    pub const fn from_pair(pair: (u32, u16)) -> Self {
        Self([(pair.0 >> 16) as u16, pair.0 as u16, pair.1])
    }

    #[inline(always)]
    pub const fn to_pair(self) -> (u32, u16) {
        ((self.0[0] as u32) << 16 | self.0[1] as u32, self.0[2])
    }

    #[inline(always)]
    pub fn strict_successor(mut self) -> Self {
        let mut carry = true;
        (self.0[2], carry) = self.0[2].overflowing_add(carry as u16);
        (self.0[1], carry) = self.0[1].overflowing_add(carry as u16);
        (self.0[0], carry) = self.0[0].overflowing_add(carry as u16);
        if carry {
            panic!("overflowing");
        }
        self
    }
}
