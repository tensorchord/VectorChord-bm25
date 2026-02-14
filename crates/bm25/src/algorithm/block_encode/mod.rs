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

mod delta_bitpack;

use std::num::NonZero;

pub trait BlockEncodeTrait {
    fn encode(
        &mut self,
        offset: Option<NonZero<u32>>,
        docids: &mut [u32],
        freqs: &mut [u32],
    ) -> &[u8];
}

pub trait BlockDecodeTrait {
    fn decode(&mut self, data: &[u8], offset: Option<NonZero<u32>>);
    fn next(&mut self) -> bool;
    fn seek(&mut self, target: u32) -> bool;
    fn docid(&self) -> u32;
    fn freq(&self) -> u32;
}

pub type BlockEncode = delta_bitpack::DeltaBitpackEncode;
pub type BlockDecode = delta_bitpack::DeltaBitpackDecode;
