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

use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct Bm25IndexOptions {
    #[serde(default = "Bm25IndexOptions::default_k1")]
    #[validate(range(min = 1.2, max = 2.0))]
    pub k1: f64,
    #[serde(default = "Bm25IndexOptions::default_b")]
    #[validate(range(min = 0.0, max = 1.0))]
    pub b: f64,
}

impl Bm25IndexOptions {
    fn default_k1() -> f64 {
        1.2
    }
    fn default_b() -> f64 {
        0.75
    }
}

impl Default for Bm25IndexOptions {
    fn default() -> Self {
        Self {
            k1: Self::default_k1(),
            b: Self::default_b(),
        }
    }
}
