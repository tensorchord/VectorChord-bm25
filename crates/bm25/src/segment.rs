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

use crate::vector::Bm25VectorBorrowed;
use std::collections::BTreeMap;
use std::iter::zip;

pub struct Segment {
    pub(crate) documents: Vec<(u32, [u16; 3])>,
    pub(crate) tokens: BTreeMap<u32, Vec<(u32, u32)>>,
}

impl Segment {
    pub fn new() -> Self {
        Self {
            documents: Vec::new(),
            tokens: BTreeMap::new(),
        }
    }
    pub fn push(&mut self, document: Bm25VectorBorrowed<'_>, payload: [u16; 3]) {
        let i = self.documents.len();
        #[allow(non_contiguous_range_endpoints)]
        let Ok(i @ ..u32::MAX) = u32::try_from(i) else {
            panic!("number of documents exceeds {}", u32::MAX - 1);
        };
        let norm = document.norm();
        self.documents.push((norm, payload));
        for (&key, &val) in zip(document.indexes(), document.values()) {
            self.tokens.entry(key).or_default().push((i, val));
        }
    }
}
