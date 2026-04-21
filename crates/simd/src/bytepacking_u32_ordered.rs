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

mod bytewidth {
    #[crate::multiversion("v4", "v3", "v2", "a2")]
    pub fn bytewidth(min: u32, input: &[u32]) -> u8 {
        let mut last = min;
        let mut reduce_or = 0_u32;
        for x in input.iter().copied() {
            reduce_or |= x - last;
            last = x;
        }
        let bitwidth = if reduce_or != 0 {
            1 + reduce_or.ilog2() as u8
        } else {
            0
        };
        bitwidth.div_ceil(8).max(1)
    }
}

pub fn bytewidth(min: u32, input: &[u32]) -> u8 {
    bytewidth::bytewidth(min, input)
}

mod compress_1 {
    #[crate::multiversion("v4", "v3", "v2", "a2")]
    pub fn compress(min: u32, input: &[u32], output: &mut [u8]) {
        assert!(input.len() <= 128);
        let (output, remainder) = output.as_chunks_mut::<1>();
        assert!(remainder.is_empty());
        assert_eq!(input.len(), output.len());
        let mut state = min;
        let n = input.len();
        for i in 0..n {
            let bytes = (input[i] - state).to_ne_bytes();
            output[i] = cfg_select! {
                target_endian = "little" => {
                    [bytes[0]]
                }
                target_endian = "big" => {
                    [bytes[3]]
                }
            };
            state = input[i];
        }
    }
}

mod decompress_1 {
    #[crate::multiversion("v4", "v3", "v2", "a2")]
    pub fn decompress(min: u32, input: &[u8], output: &mut [u32]) {
        assert!(output.len() <= 128);
        let (input, remainder) = input.as_chunks::<1>();
        assert!(remainder.is_empty());
        assert_eq!(input.len(), output.len());
        let mut state = min;
        let n = input.len();
        for i in 0..n {
            let bytes = input[i];
            let bytes = cfg_select! {
                target_endian = "little" => {
                    [bytes[0], 0, 0, 0]
                }
                target_endian = "big" => {
                    [0, 0, 0, bytes[0]]
                }
            };
            output[i] = u32::from_ne_bytes(bytes) + state;
            state = output[i];
        }
    }
}

mod compress_2 {
    #[crate::multiversion("v4", "v3", "v2", "a2")]
    pub fn compress(min: u32, input: &[u32], output: &mut [u8]) {
        assert!(input.len() <= 128);
        let (output, remainder) = output.as_chunks_mut::<2>();
        assert!(remainder.is_empty());
        assert_eq!(input.len(), output.len());
        let mut state = min;
        let n = input.len();
        for i in 0..n {
            let bytes = (input[i] - state).to_ne_bytes();
            output[i] = cfg_select! {
                target_endian = "little" => {
                    [bytes[0], bytes[1]]
                }
                target_endian = "big" => {
                    [bytes[2], bytes[3]]
                }
            };
            state = input[i];
        }
    }
}

mod decompress_2 {
    #[crate::multiversion("v4", "v3", "v2", "a2")]
    pub fn decompress(min: u32, input: &[u8], output: &mut [u32]) {
        assert!(output.len() <= 128);
        let (input, remainder) = input.as_chunks::<2>();
        assert!(remainder.is_empty());
        assert_eq!(input.len(), output.len());
        let mut state = min;
        let n = input.len();
        for i in 0..n {
            let bytes = input[i];
            let bytes = cfg_select! {
                target_endian = "little" => {
                    [bytes[0], bytes[1], 0, 0]
                }
                target_endian = "big" => {
                    [0, 0, bytes[0], bytes[1]]
                }
            };
            output[i] = u32::from_ne_bytes(bytes) + state;
            state = output[i];
        }
    }
}

mod compress_3 {
    #[crate::multiversion("v4", "v3", "v2", "a2")]
    pub fn compress(min: u32, input: &[u32], output: &mut [u8]) {
        assert!(input.len() <= 128);
        let (output, remainder) = output.as_chunks_mut::<3>();
        assert!(remainder.is_empty());
        assert_eq!(input.len(), output.len());
        let mut state = min;
        let n = input.len();
        for i in 0..n {
            let bytes = (input[i] - state).to_ne_bytes();
            output[i] = cfg_select! {
                target_endian = "little" => {
                    [bytes[0], bytes[1], bytes[2]]
                }
                target_endian = "big" => {
                    [bytes[1], bytes[2], bytes[3]]
                }
            };
            state = input[i];
        }
    }
}

mod decompress_3 {
    #[crate::multiversion("v4", "v3", "v2", "a2")]
    pub fn decompress(min: u32, input: &[u8], output: &mut [u32]) {
        assert!(output.len() <= 128);
        let (input, remainder) = input.as_chunks::<3>();
        assert!(remainder.is_empty());
        assert_eq!(input.len(), output.len());
        let mut state = min;
        let n = input.len();
        for i in 0..n {
            let bytes = input[i];
            let bytes = cfg_select! {
                target_endian = "little" => {
                    [bytes[0], bytes[1], bytes[2], 0]
                }
                target_endian = "big" => {
                    [0, bytes[0], bytes[1], bytes[2]]
                }
            };
            output[i] = u32::from_ne_bytes(bytes) + state;
            state = output[i];
        }
    }
}

pub fn compress(min: u32, bytewidth: u8, input: &[u32], output: &mut [u8]) {
    assert!(
        matches!(bytewidth, 1..=4)
            && input.len() <= 128
            && bytewidth as usize * input.len() == output.len(),
        "unexpected len"
    );
    match bytewidth {
        1 => compress_1::compress(min, input, output),
        2 => compress_2::compress(min, input, output),
        3 => compress_3::compress(min, input, output),
        4 => output.copy_from_slice(zerocopy::IntoBytes::as_bytes(input)),
        _ => panic!("bytewidth out of bound"),
    }
}

pub fn decompress(min: u32, bytewidth: u8, input: &[u8], output: &mut [u32]) {
    assert!(
        matches!(bytewidth, 1..=4)
            && output.len() <= 128
            && bytewidth as usize * output.len() == input.len(),
        "unexpected len"
    );
    match bytewidth {
        1 => decompress_1::decompress(min, input, output),
        2 => decompress_2::decompress(min, input, output),
        3 => decompress_3::decompress(min, input, output),
        4 => zerocopy::IntoBytes::as_mut_bytes(output).copy_from_slice(input),
        _ => panic!("bytewidth out of bound"),
    }
}

#[test]
fn test() {
    for i in 0..=4 {
        let mut data: [u32; 128] = core::array::from_fn(|_| {
            if i < 4 {
                rand::random_range(0..1 << (i * 8))
            } else {
                rand::random()
            }
        });
        data.sort();
        for len in 0..=128 {
            let data = &data[..len];
            let min = data.get(0).copied().unwrap_or(998244353);
            let bytewidth = bytewidth(min, &data);
            assert!(bytewidth as usize <= i.max(1));
            let mut compressed = vec![0_u8; bytewidth as usize * len];
            compress(min, bytewidth, &data, &mut compressed);
            let mut decompressed = vec![0_u32; len];
            decompress(min, bytewidth, &compressed, &mut decompressed);
            assert_eq!(data, decompressed);
        }
    }
}
