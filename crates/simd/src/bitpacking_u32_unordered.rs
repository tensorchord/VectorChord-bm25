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

mod bitwidth {
    #[crate::multiversion("v4", "v3", "v2", "a2")]
    pub fn bitwidth(input: &[u32; 128]) -> u8 {
        let mut reduce_or = 0_u32;
        for x in input.iter().copied() {
            reduce_or |= x;
        }
        if reduce_or != 0 {
            1 + reduce_or.ilog2() as u8
        } else {
            0
        }
    }
}

pub fn bitwidth(input: &[u32; 128]) -> u8 {
    bitwidth::bitwidth(input)
}

seq_macro::seq!(BITWIDTH in 1..=31 {
    mod compress_~BITWIDTH {
        #[inline]
        #[cfg(target_arch = "x86_64")]
        #[crate::target_cpu(enable = "v2")]
        fn compress_v2(input: &[u32; 128], output: &mut [u8]) {
            type S = ();
            type T = core::arch::x86_64::__m128i;
            #[inline]
            #[crate::target_cpu(enable = "v2")]
            fn delta(&mut (): &mut S, value: T) -> T {
                value
            }
            use core::arch::x86_64::_mm_or_si128 as bitor;
            use core::arch::x86_64::_mm_slli_epi32 as shl;
            use core::arch::x86_64::_mm_srli_epi32 as shr;
            crate::bitpacking::compress!(BITWIDTH, 32, (), input, output)
        }

        #[inline]
        #[cfg(target_arch = "aarch64")]
        #[crate::target_cpu(enable = "a2")]
        fn compress_a2(input: &[u32; 128], output: &mut [u8]) {
            type S = ();
            type T = core::arch::aarch64::uint32x4_t;
            #[inline]
            #[crate::target_cpu(enable = "a2")]
            fn delta(&mut (): &mut S, value: T) -> T {
                value
            }
            use core::arch::aarch64::vorrq_u32 as bitor;
            use crate::emulate::vshlq_n_u32 as shl;
            use crate::emulate::vshrq_n_u32 as shr;
            crate::bitpacking::compress!(BITWIDTH, 32, (), input, output)
        }

        #[crate::multiversion(@"v2", @"a2")]
        pub fn compress(input: &[u32; 128], output: &mut [u8]) {
            type S = ();
            type T = [u32; 4];
            fn delta(&mut (): &mut S, value: T) -> T {
                value
            }
            fn bitor(lhs: T, rhs: T) -> T {
                core::array::from_fn(|i| lhs[i] | rhs[i])
            }
            fn shl<const N: usize>(value: T) -> T {
                core::array::from_fn(|i| value[i] << N)
            }
            fn shr<const N: usize>(value: T) -> T {
                core::array::from_fn(|i| value[i] >> N)
            }
            crate::bitpacking::compress!(BITWIDTH, 32, (), input, output)
        }
    }
});

pub fn compress(bitwidth: u8, input: &[u32; 128], output: &mut [u8]) {
    assert!(
        bitwidth <= 32 && bitwidth as usize * 128 / 8 == output.len(),
        "unexpected output len"
    );
    seq_macro::seq!(BITWIDTH in 1..=31 {
        match bitwidth {
            0 => (),
            #(BITWIDTH => compress_~BITWIDTH::compress(input, output),)*
            32 => {
                output.copy_from_slice(zerocopy::IntoBytes::as_bytes(input));
            },
            _ => panic!("bitwidth out of bound"),
        }
    });
}

seq_macro::seq!(BITWIDTH in 1..=31 {
    mod decompress_~BITWIDTH {
        #[inline]
        #[cfg(target_arch = "x86_64")]
        #[crate::target_cpu(enable = "v2")]
        fn decompress_v2(input: &[u8], output: &mut [u32; 128]) {
            type S = ();
            type T = core::arch::x86_64::__m128i;
            #[inline]
            #[crate::target_cpu(enable = "v2")]
            fn delta(&mut (): &mut S, value: T) -> T {
                value
            }
            use core::arch::x86_64::_mm_or_si128 as bitor;
            use core::arch::x86_64::_mm_and_si128 as bitand;
            use core::arch::x86_64::_mm_slli_epi32 as shl;
            use core::arch::x86_64::_mm_srli_epi32 as shr;
            let mask = core::arch::x86_64::_mm_set1_epi32(((1u32 << BITWIDTH) - 1).cast_signed());
            crate::bitpacking::decompress!(BITWIDTH, 32, mask, (), input, output)
        }

        #[inline]
        #[cfg(target_arch = "aarch64")]
        #[crate::target_cpu(enable = "a2")]
        fn decompress_a2(input: &[u8], output: &mut [u32; 128]) {
            type S = ();
            type T = core::arch::aarch64::uint32x4_t;
            #[inline]
            #[crate::target_cpu(enable = "a2")]
            fn delta(&mut (): &mut S, value: T) -> T {
                value
            }
            use core::arch::aarch64::vorrq_u32 as bitor;
            use core::arch::aarch64::vandq_u32 as bitand;
            use crate::emulate::vshlq_n_u32 as shl;
            use crate::emulate::vshrq_n_u32 as shr;
            let mask = core::arch::aarch64::vdupq_n_u32((1u32 << BITWIDTH) - 1);
            crate::bitpacking::decompress!(BITWIDTH, 32, mask, (), input, output)
        }

        #[crate::multiversion(@"v2", @"a2")]
        pub fn decompress(input: &[u8], output: &mut [u32; 128]) {
            type S = ();
            type T = [u32; 4];
            fn delta(&mut (): &mut S, value: T) -> T {
                value
            }
            fn bitor(lhs: T, rhs: T) -> T {
                core::array::from_fn(|i| lhs[i] | rhs[i])
            }
            fn bitand(lhs: T, rhs: T) -> T {
                core::array::from_fn(|i| lhs[i] & rhs[i])
            }
            fn shl<const N: usize>(value: T) -> T {
                core::array::from_fn(|i| value[i] << N)
            }
            fn shr<const N: usize>(value: T) -> T {
                core::array::from_fn(|i| value[i] >> N)
            }
            let mask = [(1u32 << BITWIDTH) - 1; _];
            crate::bitpacking::decompress!(BITWIDTH, 32, mask, (), input, output)
        }
    }
});

pub fn decompress(bitwidth: u8, input: &[u8], output: &mut [u32; 128]) {
    assert!(
        bitwidth <= 32 && bitwidth as usize * 128 / 8 == input.len(),
        "unexpected input len"
    );
    seq_macro::seq!(BITWIDTH in 1..=31 {
        match bitwidth {
            0 => (),
            #(BITWIDTH => decompress_~BITWIDTH::decompress(input, output),)*
            32 => {
                zerocopy::IntoBytes::as_mut_bytes(output).copy_from_slice(input);
            },
            _ => panic!("bitwidth out of bound"),
        }
    });
}

#[test]
fn test() {
    for i in 0..=32 {
        let data: [u32; 128] = core::array::from_fn(|_| {
            if i < 32 {
                rand::random_range(0..1 << i)
            } else {
                rand::random()
            }
        });
        let bitwidth = bitwidth(&data);
        assert!(bitwidth as usize <= i);
        let mut compressed = vec![0_u8; i * 128 / 8];
        compress(bitwidth, &data, &mut compressed);
        let mut decompressed = [0_u32; 128];
        decompress(bitwidth, &compressed, &mut decompressed);
        assert_eq!(data, decompressed);
    }
}
