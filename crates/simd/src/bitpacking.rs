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

macro_rules! compress {
    ($bitwidth:literal, $ebitwidth:literal, $state:expr, $input:expr, $output:expr) => {{
        let mut state = $state;
        let input = $input;
        let output = $output;
        let Ok(output) = <&mut [u8; $bitwidth * 128 / 8]>::try_from(output) else {
            panic!("unexpected output len")
        };
        let input: &[zerocopy::Unalign<T>; $ebitwidth * 128 / 8 / size_of::<T>()] = zerocopy::transmute_ref!(input);
        let output: &mut [zerocopy::Unalign<T>; $bitwidth * 128 / 8 / size_of::<T>()] = zerocopy::transmute_mut!(output);
        let mut j = 0_usize;
        let mut compressing = zerocopy::FromZeros::new_zeroed();
        seq_macro::seq!(ITERATION in 0..$ebitwidth {
            #[allow(unused_assignments)]
            {
                const CURSOR: usize = ITERATION * $bitwidth % $ebitwidth;

                let uncompressed = delta(&mut state, input[ITERATION].get());

                compressing = if CURSOR != 0 {
                    bitor(compressing, shl::<{CURSOR as _}>(uncompressed))
                } else {
                    uncompressed
                };

                if CURSOR >= $ebitwidth - $bitwidth {
                    output[j].set(compressing);
                    j += 1;
                }

                compressing = if CURSOR > $ebitwidth - $bitwidth {
                    shr::<{($ebitwidth - CURSOR) as _}>(uncompressed)
                } else {
                    compressing
                };
            }
        });
        debug_assert_eq!(j, output.len());
    }};
}

pub(crate) use compress;

macro_rules! decompress {
    ($bitwidth:literal, $ebitwidth:literal, $mask:expr, $state:expr, $input:expr, $output:expr) => {{
        let mask: T = $mask;
        let mut state = $state;
        let input = $input;
        let output = $output;
        let Ok(input) = <&[u8; $bitwidth * 128 / 8]>::try_from(input) else {
            panic!("unexpected input len")
        };
        let input: &[zerocopy::Unalign<T>; $bitwidth * 128 / 8 / size_of::<T>()] = zerocopy::transmute_ref!(input);
        let output: &mut [zerocopy::Unalign<T>; $ebitwidth * 128 / 8 / size_of::<T>()] = zerocopy::transmute_mut!(output);
        let mut i = 0_usize;
        let mut decompressing = zerocopy::FromZeros::new_zeroed();
        seq_macro::seq!(ITERATION in 0..$ebitwidth {
            {
                const CURSOR: usize = ITERATION * $bitwidth % $ebitwidth;

                if CURSOR == 0 {
                    decompressing = input[i].get();
                    i += 1;
                }

                let mut decompressed = bitand(shr::<{CURSOR as _}>(decompressing), mask);

                if CURSOR > $ebitwidth - $bitwidth {
                    decompressing = input[i].get();
                    i += 1;
                }

                decompressed = if CURSOR > $ebitwidth - $bitwidth {
                    bitor(decompressed, bitand(shl::<{($ebitwidth - CURSOR) as _}>(decompressing), mask))
                } else {
                    decompressed
                };

                output[ITERATION].set(delta(&mut state, decompressed));
            }
        });
        debug_assert_eq!(i, input.len());
    }};
}

pub(crate) use decompress;
