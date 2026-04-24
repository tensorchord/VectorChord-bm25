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

#[cfg(target_arch = "aarch64")]
#[inline]
#[target_feature(enable = "neon")]
pub fn vshlq_n_u32<const N: usize>(
    a: core::arch::aarch64::uint32x4_t,
) -> core::arch::aarch64::uint32x4_t {
    seq_macro::seq!(I in 1..32 {
        match N {
            0 => a,
            #(I => core::arch::aarch64::vshlq_n_u32::<I>(a),)*
            _ => unreachable!(),
        }
    })
}

#[cfg(target_arch = "aarch64")]
#[inline]
#[target_feature(enable = "neon")]
pub fn vshrq_n_u32<const N: usize>(
    a: core::arch::aarch64::uint32x4_t,
) -> core::arch::aarch64::uint32x4_t {
    seq_macro::seq!(I in 1..32 {
        match N {
            0 => a,
            #(I => core::arch::aarch64::vshrq_n_u32::<I>(a),)*
            _ => unreachable!(),
        }
    })
}
