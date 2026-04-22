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
use std::cmp::Ordering;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

#[repr(C, packed(2))]
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
pub struct Record(pub u32, pub [u16; 3]);

#[repr(C)]
#[derive(Debug, Clone, FromBytes, IntoBytes, Immutable, KnownLayout)]
pub struct Mapping(pub [u8; WIDTH], pub u32, pub u32);

impl PartialEq for Mapping {
    fn eq(&self, other: &Self) -> bool {
        (self.0, self.1).eq(&(other.0, other.1))
    }
}

impl Eq for Mapping {}

impl PartialOrd for Mapping {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Mapping {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.0, self.1).cmp(&(other.0, other.1))
    }
}

pub struct Segment<R, M> {
    pub records: R,
    pub mappings: M,
}
