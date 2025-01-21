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
