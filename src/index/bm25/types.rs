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

use bm25::types::Bm25IndexOptions;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct Bm25BuildOptions {}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct Bm25IndexingOptions {
    #[serde(flatten)]
    #[validate(nested)]
    pub index: Bm25IndexOptions,
    #[serde(default)]
    #[validate(nested)]
    pub build: Bm25BuildOptions,
}
