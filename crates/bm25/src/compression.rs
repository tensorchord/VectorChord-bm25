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

pub struct Decompressed {
    internal: [u32; 128],
    len: u8,
}

impl Decompressed {
    pub fn new() -> Self {
        Self {
            internal: [0u32; 128],
            len: 0,
        }
    }
    pub fn set_len(&mut self, new_len: u8) {
        assert!(new_len <= 128);
        self.len = new_len;
    }
    pub fn as_slice(&self) -> &[u32] {
        &self.internal[..self.len as usize]
    }
}

pub fn compress_document_ids(min_document_id: u32, uncompressed: &[u32]) -> (u8, Vec<u8>) {
    debug_assert!(min_document_id <= uncompressed.iter().copied().min().unwrap_or(u32::MAX));
    let n = uncompressed.len();
    if n > 128 {
        panic!("block size exceeds 128");
    }
    if let Ok(uncompressed) = <&[u32; 128]>::try_from(uncompressed) {
        let bitwidth = simd::bitpacking_u32_ordered::bitwidth(min_document_id, uncompressed);
        let mut compressed = vec![0_u8; 128 * (bitwidth as usize) / 8];
        simd::bitpacking_u32_ordered::compress(
            min_document_id,
            bitwidth,
            uncompressed,
            compressed.as_mut(),
        );
        ((0u8 << 7) | bitwidth, compressed)
    } else {
        let bytewidth = simd::bytepacking_u32_ordered::bytewidth(min_document_id, uncompressed);
        let mut compressed = vec![0_u8; bytewidth as usize * uncompressed.len()];
        simd::bytepacking_u32_ordered::compress(
            min_document_id,
            bytewidth,
            uncompressed,
            compressed.as_mut(),
        );
        ((1u8 << 7) | bytewidth, compressed)
    }
}

pub fn decompress_document_ids(
    min_document_id: u32,
    metadata: u8,
    compressed: &[u8],
    decompressed: &mut Decompressed,
) {
    let flags = metadata >> 7;
    if flags == 0 {
        let bitwidth = metadata & ((1 << 7) - 1);
        simd::bitpacking_u32_ordered::decompress(
            min_document_id,
            bitwidth,
            compressed,
            &mut decompressed.internal,
        );
        decompressed.set_len(128);
    } else {
        let bytewidth = metadata & ((1 << 7) - 1);
        let new_len = compressed.len() / bytewidth as usize;
        simd::bytepacking_u32_ordered::decompress(
            min_document_id,
            bytewidth,
            compressed,
            &mut decompressed.internal[..new_len],
        );
        decompressed.set_len(new_len as u8);
    }
}

pub fn compress_term_frequencies(uncompressed: &[u32]) -> (u8, Vec<u8>) {
    let n = uncompressed.len();
    if n > 128 {
        panic!("block size exceeds 128");
    }
    if let Ok(uncompressed) = <&[u32; 128]>::try_from(uncompressed) {
        let bitwidth = simd::bitpacking_u32_unordered::bitwidth(uncompressed);
        let mut compressed = vec![0_u8; 128 * (bitwidth as usize) / 8];
        simd::bitpacking_u32_unordered::compress(bitwidth, uncompressed, compressed.as_mut());
        ((0u8 << 7) | bitwidth, compressed)
    } else {
        let bytewidth = simd::bytepacking_u32_unordered::bytewidth(uncompressed);
        let mut compressed = vec![0_u8; bytewidth as usize * uncompressed.len()];
        simd::bytepacking_u32_unordered::compress(bytewidth, uncompressed, compressed.as_mut());
        ((1u8 << 7) | bytewidth, compressed)
    }
}

pub fn decompress_term_frequencies(
    metadata: u8,
    compressed: &[u8],
    decompressed: &mut Decompressed,
) {
    let flags = metadata >> 7;
    if flags == 0 {
        let bitwidth = metadata & ((1 << 7) - 1);
        simd::bitpacking_u32_unordered::decompress(
            bitwidth,
            compressed,
            &mut decompressed.internal,
        );
        decompressed.set_len(128);
    } else {
        let bytewidth = metadata & ((1 << 7) - 1);
        let new_len = compressed.len() / bytewidth as usize;
        simd::bytepacking_u32_unordered::decompress(
            bytewidth,
            compressed,
            &mut decompressed.internal[..new_len],
        );
        decompressed.set_len(new_len as u8);
    }
}
