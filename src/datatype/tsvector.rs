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

use bm25::vector::{Document, Element, Query, intern};
use std::num::Saturating;

#[derive(Debug, Clone)]
pub struct TsVectorOwned {
    entries: Vec<u32>,
    bytes: Vec<u8>,
}

impl TsVectorOwned {
    #[expect(dead_code)]
    #[inline(always)]
    pub fn new(entries: Vec<u32>, bytes: Vec<u8>) -> Self {
        Self { entries, bytes }
    }
}

impl TsVectorOwned {
    #[expect(dead_code)]
    #[inline(always)]
    pub fn as_borrowed(&self) -> TsVectorBorrowed<'_> {
        TsVectorBorrowed {
            entries: &self.entries,
            bytes: &self.bytes,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TsVectorBorrowed<'a> {
    entries: &'a [u32],
    bytes: &'a [u8],
}

impl<'a> TsVectorBorrowed<'a> {
    #[inline(always)]
    pub fn new(entries: &'a [u32], bytes: &'a [u8]) -> Self {
        Self { entries, bytes }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&[u8], Option<u16>)> {
        self.entries.iter().map(|&entry| {
            let haspos = (entry >> 0) & ((1 << 1) - 1);
            let len = (entry >> 1) & ((1 << 11) - 1);
            let pos = (entry >> 12) & ((1 << 20) - 1);
            let string = &self.bytes[pos as usize..][..len as usize];
            let count = if haspos != 0 {
                let offset = (pos + len).next_multiple_of(2) as usize;
                let (lo, hi) = (self.bytes[offset], self.bytes[offset + 1]);
                Some(u16::from_ne_bytes([lo, hi]))
            } else {
                None
            };
            (string, count)
        })
    }
}

impl TsVectorBorrowed<'_> {
    #[expect(dead_code)]
    #[inline(always)]
    pub fn own(&self) -> TsVectorOwned {
        TsVectorOwned {
            entries: self.entries.to_vec(),
            bytes: self.bytes.to_vec(),
        }
    }
}

pub fn cast_tsvector_to_document(tsvector: TsVectorBorrowed<'_>) -> Document {
    let mut internal = Vec::new();
    for (string, count) in tsvector.iter() {
        let key = intern(string);
        let value: u32 = count.expect("tsvector must have positions").into();
        internal.push(Element { key, value });
    }
    internal.sort_unstable_by(|Element { key: l, .. }, Element { key: r, .. }| Ord::cmp(l, r));
    dedup(&mut internal);
    Document::new(internal)
}

pub fn cast_tsvector_to_query(tsvector: TsVectorBorrowed<'_>) -> Query {
    let mut internal = Vec::new();
    for (string, _) in tsvector.iter() {
        let key = intern(string);
        internal.push(key);
    }
    internal.sort_unstable();
    internal.dedup();
    Query::new(internal)
}

fn dedup(internal: &mut Vec<Element>) {
    let n = internal.len();
    let (mut i, mut j) = (0_usize, 0_usize);
    while i < n {
        let mut k = i + 1;
        while k < n && internal[k].key == internal[i].key {
            k += 1;
        }
        let value = internal[i..k]
            .iter()
            .map(|&Element { value, .. }| Saturating(value))
            .sum::<Saturating<u32>>()
            .0;
        internal[j] = Element {
            key: internal[i].key,
            value,
        };
        (i, j) = (k, j + 1);
    }
    internal.truncate(j);
}
