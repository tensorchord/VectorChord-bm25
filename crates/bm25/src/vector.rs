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

use crate::WIDTH;
use std::num::Saturating;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

pub fn intern(seed: &[u8; 32], string: &[u8]) -> [u8; WIDTH] {
    use zerocopy::FromBytes;
    if string.len() < WIDTH && !string.contains(&0) {
        let mut result = [0_u8; WIDTH];
        result[..string.len()].copy_from_slice(string);
        result
    } else {
        let hash = blake3::keyed_hash(seed, string);
        let Ok((mut result, _)) = <[u8; WIDTH]>::read_from_prefix(hash.as_bytes()) else {
            unreachable!()
        };
        if result[WIDTH - 1] == 0 {
            result[WIDTH - 1] = 1;
        }
        result
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, IntoBytes, FromBytes, Immutable, KnownLayout)]
pub struct Element {
    pub key: [u8; WIDTH],
    pub value: u32,
}

#[derive(Debug, Clone)]
pub struct Document {
    internal: Vec<Element>,
}

impl Document {
    #[inline(always)]
    pub fn new(internal: Vec<Element>) -> Self {
        Self::checked_new(internal).expect("invalid data")
    }

    #[inline(always)]
    pub fn checked_new(internal: Vec<Element>) -> Option<Self> {
        if !internal.is_sorted_by(|Element { key: l, .. }, Element { key: r, .. }| l < r) {
            return None;
        }
        if !internal.iter().all(|&Element { value, .. }| value != 0) {
            return None;
        }
        Some(Self { internal })
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.internal.len()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.internal.is_empty()
    }

    #[inline(always)]
    pub fn length(&self) -> u32 {
        self.internal
            .iter()
            .map(|&Element { value, .. }| Saturating(value))
            .sum::<Saturating<u32>>()
            .0
    }

    #[inline(always)]
    pub fn as_slice(&self) -> &[Element] {
        self.internal.as_slice()
    }

    #[inline(always)]
    pub fn iter(&self) -> impl Iterator<Item = &Element> {
        self.internal.iter()
    }
}

#[derive(Debug, Clone)]
pub struct Query {
    internal: Vec<[u8; WIDTH]>,
}

impl Query {
    #[inline(always)]
    pub fn new(internal: Vec<[u8; WIDTH]>) -> Self {
        Self::checked_new(internal).expect("invalid data")
    }

    #[inline(always)]
    pub fn checked_new(internal: Vec<[u8; WIDTH]>) -> Option<Self> {
        if !internal.is_sorted_by(|l, r| l < r) {
            return None;
        }
        Some(Self { internal })
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.internal.len()
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.internal.is_empty()
    }

    #[inline(always)]
    pub fn as_slice(&self) -> &[[u8; WIDTH]] {
        self.internal.as_slice()
    }

    #[inline(always)]
    pub fn iter(&self) -> impl Iterator<Item = &[u8; WIDTH]> {
        self.internal.iter()
    }
}
