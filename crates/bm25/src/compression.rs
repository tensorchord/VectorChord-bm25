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

use bitpacking::BitPacker;
use zerocopy::{FromBytes, IntoBytes, Unalign};

pub fn compress_document_ids(min_document_id: u32, uncompressed: &[u32]) -> (u8, Vec<u8>) {
    debug_assert!(min_document_id <= uncompressed.iter().copied().min().unwrap_or(u32::MAX));
    let n = uncompressed.len();
    if n > 128 {
        panic!("block size exceeds 128");
    }
    if n < 128 {
        return (u8::MAX, uncompressed.as_bytes().to_vec());
    }
    let bitpacker = bitpacking::BitPacker4x::new();
    let bitwidth = bitpacker.num_bits_strictly_sorted(Some(min_document_id), uncompressed);
    let mut compressed = vec![0_u8; 128 * (bitwidth as usize) / 8];
    bitpacker.compress_strictly_sorted(
        Some(min_document_id),
        uncompressed,
        compressed.as_mut(),
        bitwidth,
    );
    (bitwidth, compressed)
}

pub fn decompress_document_ids(min_document_id: u32, bitwidth: u8, compressed: &[u8]) -> Vec<u32> {
    if bitwidth == u8::MAX {
        let d = <[Unalign<u32>]>::ref_from_bytes(compressed).expect("data corruption");
        let mut decompressed = Vec::<u32>::with_capacity(d.len());
        #[allow(unsafe_code)]
        unsafe {
            core::ptr::copy_nonoverlapping(d.as_ptr(), decompressed.as_mut_ptr().cast(), d.len());
            decompressed.set_len(d.len());
        };
        decompressed
    } else {
        let bitpacker = bitpacking::BitPacker4x::new();
        let mut decompressed = vec![0_u32; 128];
        bitpacker.decompress_strictly_sorted(
            Some(min_document_id),
            compressed,
            decompressed.as_mut(),
            bitwidth,
        );
        decompressed
    }
}

pub fn compress_term_frequencies(uncompressed: &[u32]) -> (u8, Vec<u8>) {
    let n = uncompressed.len();
    if n > 128 {
        panic!("block size exceeds 128");
    }
    if n < 128 {
        return (u8::MAX, uncompressed.as_bytes().to_vec());
    }
    let bitpacker = bitpacking::BitPacker4x::new();
    let bitwidth = bitpacker.num_bits(uncompressed);
    let mut compressed = vec![0_u8; 128 * (bitwidth as usize) / 8];
    bitpacker.compress(uncompressed, compressed.as_mut(), bitwidth);
    (bitwidth, compressed)
}

pub fn decompress_term_frequencies(bitwidth: u8, compressed: &[u8]) -> Vec<u32> {
    if bitwidth == u8::MAX {
        let d = <[Unalign<u32>]>::ref_from_bytes(compressed).expect("data corruption");
        let mut decompressed = Vec::<u32>::with_capacity(d.len());
        #[allow(unsafe_code)]
        unsafe {
            core::ptr::copy_nonoverlapping(d.as_ptr(), decompressed.as_mut_ptr().cast(), d.len());
            decompressed.set_len(d.len());
        };
        decompressed
    } else {
        let bitpacker = bitpacking::BitPacker4x::new();
        let mut decompressed = vec![0_u32; 128];
        bitpacker.decompress(compressed, decompressed.as_mut(), bitwidth);
        decompressed
    }
}
