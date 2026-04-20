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

mod build;
mod bulkdelete;
mod compression;
mod evaluate;
mod insert;
mod maintain;
mod search;
mod segment;
mod tape;
mod tree;
mod tuples;

pub mod types;
pub mod vector;

use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

pub const WIDTH: usize = 14;

const _: () = assert!(WIDTH <= 32);

#[repr(C, align(8))]
#[derive(Debug, Clone, Copy, PartialEq, FromBytes, IntoBytes, Immutable, KnownLayout)]
pub struct Opaque {
    pub next: u32,
    pub flags: u32,
}

#[allow(unsafe_code)]
unsafe impl index::relation::Opaque for Opaque {
    fn is_deleted(&self) -> bool {
        const B: u32 = 1 << 0;
        self.flags & B != 0
    }
    fn set_deleted(&mut self) {
        const B: u32 = 1 << 0;
        self.flags |= B;
    }
}

pub use build::build;
pub use bulkdelete::bulkdelete;
pub use evaluate::evaluate;
pub use insert::insert;
pub use maintain::maintain;
pub use search::search;
pub use segment::{Collector, Segment};

const FIELDNORM_TO_LENGTH: [u32; 256] = [
    0,
    1,
    2,
    3,
    4,
    5,
    6,
    7,
    8,
    9,
    10,
    11,
    12,
    13,
    14,
    15,
    16,
    17,
    18,
    19,
    20,
    21,
    22,
    23,
    24,
    25,
    26,
    27,
    28,
    29,
    30,
    31,
    32,
    33,
    34,
    35,
    36,
    37,
    38,
    39,
    40,
    42,
    44,
    46,
    48,
    50,
    52,
    54,
    56,
    60,
    64,
    68,
    72,
    76,
    80,
    84,
    88,
    96,
    104,
    112,
    120,
    128,
    136,
    144,
    152,
    168,
    184,
    200,
    216,
    232,
    248,
    264,
    280,
    312,
    344,
    376,
    408,
    440,
    472,
    504,
    536,
    600,
    664,
    728,
    792,
    856,
    920,
    984,
    1_048,
    1_176,
    1_304,
    1_432,
    1_560,
    1_688,
    1_816,
    1_944,
    2_072,
    2_328,
    2_584,
    2_840,
    3_096,
    3_352,
    3_608,
    3_864,
    4_120,
    4_632,
    5_144,
    5_656,
    6_168,
    6_680,
    7_192,
    7_704,
    8_216,
    9_240,
    10_264,
    11_288,
    12_312,
    13_336,
    14_360,
    15_384,
    16_408,
    18_456,
    20_504,
    22_552,
    24_600,
    26_648,
    28_696,
    30_744,
    32_792,
    36_888,
    40_984,
    45_080,
    49_176,
    53_272,
    57_368,
    61_464,
    65_560,
    73_752,
    81_944,
    90_136,
    98_328,
    106_520,
    114_712,
    122_904,
    131_096,
    147_480,
    163_864,
    180_248,
    196_632,
    213_016,
    229_400,
    245_784,
    262_168,
    294_936,
    327_704,
    360_472,
    393_240,
    426_008,
    458_776,
    491_544,
    524_312,
    589_848,
    655_384,
    720_920,
    786_456,
    851_992,
    917_528,
    983_064,
    1_048_600,
    1_179_672,
    1_310_744,
    1_441_816,
    1_572_888,
    1_703_960,
    1_835_032,
    1_966_104,
    2_097_176,
    2_359_320,
    2_621_464,
    2_883_608,
    3_145_752,
    3_407_896,
    3_670_040,
    3_932_184,
    4_194_328,
    4_718_616,
    5_242_904,
    5_767_192,
    6_291_480,
    6_815_768,
    7_340_056,
    7_864_344,
    8_388_632,
    9_437_208,
    10_485_784,
    11_534_360,
    12_582_936,
    13_631_512,
    14_680_088,
    15_728_664,
    16_777_240,
    18_874_392,
    20_971_544,
    23_068_696,
    25_165_848,
    27_263_000,
    29_360_152,
    31_457_304,
    33_554_456,
    37_748_760,
    41_943_064,
    46_137_368,
    50_331_672,
    54_525_976,
    58_720_280,
    62_914_584,
    67_108_888,
    75_497_496,
    83_886_104,
    92_274_712,
    100_663_320,
    109_051_928,
    117_440_536,
    125_829_144,
    134_217_752,
    150_994_968,
    167_772_184,
    184_549_400,
    201_326_616,
    218_103_832,
    234_881_048,
    251_658_264,
    268_435_480,
    301_989_912,
    335_544_344,
    369_098_776,
    402_653_208,
    436_207_640,
    469_762_072,
    503_316_504,
    536_870_936,
    603_979_800,
    671_088_664,
    738_197_528,
    805_306_392,
    872_415_256,
    939_524_120,
    1_006_632_984,
    1_073_741_848,
    1_207_959_576,
    1_342_177_304,
    1_476_395_032,
    1_610_612_760,
    1_744_830_488,
    1_879_048_216,
    2_013_265_944,
];

fn fieldnorm_to_length(fieldnorm: u8) -> u32 {
    FIELDNORM_TO_LENGTH[fieldnorm as usize]
}

fn length_to_fieldnorm(length: u32) -> u8 {
    match FIELDNORM_TO_LENGTH.binary_search(&length) {
        Ok(index) => index as u8,
        Err(index) => (index - 1) as u8,
    }
}

fn idf(number_of_documents: u32, token_number_of_documents: u32) -> f64 {
    let number_of_documents = number_of_documents as f64;
    let token_number_of_documents = token_number_of_documents as f64;
    ((number_of_documents + 1.0) / (token_number_of_documents + 0.5)).ln()
}

fn tf(fieldnorm: u8, term_frequency: u32, k1: f64, b: f64, avgdl: f64) -> f64 {
    let term_frequency = term_frequency as f64;
    let document_length = fieldnorm_to_length(fieldnorm) as f64;
    (term_frequency * (k1 + 1.0)) / (term_frequency + k1 * (1.0 - b + b * document_length / avgdl))
}
