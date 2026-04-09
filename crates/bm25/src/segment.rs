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

use crate::tf;
use crate::vector::Bm25VectorBorrowed;
use std::iter::zip;

pub struct Wand {
    tf: f64,
    document_length: u32,
    term_frequency: u32,
}

impl Wand {
    pub fn new() -> Self {
        Self {
            tf: 0.0f64,
            document_length: u32::MAX,
            term_frequency: 0_u32,
        }
    }
    pub fn push(&mut self, k1: f64, b: f64, avgdl: f64, document_length: u32, term_frequency: u32) {
        let tf = tf(k1, b, avgdl, document_length, term_frequency);
        if self.tf < tf {
            self.tf = tf;
            self.document_length = document_length;
            self.term_frequency = term_frequency;
        }
    }
    pub fn extend(&mut self, other: &Self) {
        if self.tf < other.tf {
            self.tf = other.tf;
            self.document_length = other.document_length;
            self.term_frequency = other.term_frequency;
        }
    }
    pub fn document_length(&self) -> u32 {
        self.document_length
    }
    pub fn term_frequency(&self) -> u32 {
        self.term_frequency
    }
}

pub struct Collector0 {
    documents: Vec<(u32, [u16; 3])>,
    relabel: Vec<Option<u32>>,
}

impl Collector0 {
    pub fn new() -> Self {
        Self {
            documents: Vec::new(),
            relabel: Vec::new(),
        }
    }
    pub fn add_document(&mut self, data: Option<(u32, [u16; 3])>) {
        if let Some(data) = data {
            let id = self.documents.len() as u32;
            self.documents.push(data);
            self.relabel.push(Some(id));
        } else {
            self.relabel.push(None);
        }
    }
    pub fn finish(self) -> Collector1 {
        Collector1 {
            documents: self.documents,
            relabel: self.relabel,
            lists: Vec::new(),
        }
    }
}

pub struct Collector1 {
    documents: Vec<(u32, [u16; 3])>,
    relabel: Vec<Option<u32>>,
    lists: Vec<(u32, u32, u32)>,
}

impl Collector1 {
    pub fn add_element(&mut self, token_id: u32, document_id: u32, term_frequency: u32) {
        if let Some(document_id) = self.relabel[document_id as usize] {
            self.lists.push((token_id, document_id, term_frequency));
        }
    }
    pub fn finish(self) -> Collector {
        Collector {
            documents: self.documents,
            lists: self.lists,
        }
    }
}

pub struct Collector {
    documents: Vec<(u32, [u16; 3])>,
    lists: Vec<(u32, u32, u32)>,
}

impl Collector {
    pub fn new() -> Self {
        Self {
            documents: Vec::new(),
            lists: Vec::new(),
        }
    }
    pub fn push(&mut self, document: Bm25VectorBorrowed<'_>, payload: [u16; 3]) {
        let document_id = self.documents.len();
        #[allow(non_contiguous_range_endpoints)]
        let Ok(document_id @ ..u32::MAX) = u32::try_from(document_id) else {
            panic!("number of documents exceeds {}", u32::MAX - 1);
        };
        let norm = document.norm();
        self.documents.push((norm, payload));
        for (&token_id, &term_frequency) in zip(document.indexes(), document.values()) {
            self.lists.push((token_id, document_id, term_frequency));
        }
    }
    pub fn finish(self) -> Segment {
        let (documents, mut lists) = (self.documents, self.lists);
        lists.sort_unstable_by_key(|&(token_id, document_id, _)| (token_id, document_id));
        Segment { documents, lists }
    }
}

pub struct Segment {
    documents: Vec<(u32, [u16; 3])>,
    lists: Vec<(u32, u32, u32)>,
}

impl Segment {
    pub fn documents(&self) -> &[(u32, [u16; 3])] {
        self.documents.as_slice()
    }
    pub fn tokens(&self) -> impl Iterator<Item = SegmentToken<'_>> {
        self.lists
            .chunk_by(|&(x, ..), &(y, ..)| x == y)
            .map(|internal| SegmentToken { internal })
    }
}

pub struct SegmentToken<'a> {
    internal: &'a [(u32, u32, u32)],
}

impl SegmentToken<'_> {
    pub fn id(&self) -> u32 {
        self.internal[0].0
    }
    pub fn number_of_documents(&self) -> u32 {
        self.internal.len() as u32
    }
    pub fn blocks(&self) -> impl Iterator<Item = SegmentBlock<'_>> {
        self.internal
            .chunks(128)
            .map(|internal| SegmentBlock { internal })
    }
}

pub struct SegmentBlock<'a> {
    internal: &'a [(u32, u32, u32)],
}

impl SegmentBlock<'_> {
    pub fn min_document_id(&self) -> u32 {
        self.internal[0].1
    }
    pub fn max_document_id(&self) -> u32 {
        let n = self.internal.len();
        self.internal[n - 1].1
    }
    pub fn number_of_documents(&self) -> u32 {
        self.internal.len() as u32
    }
    pub fn internal(&self) -> &[(u32, u32, u32)] {
        self.internal
    }
    pub fn document_ids(&self) -> Vec<u32> {
        self.internal.iter().map(|&(_, x, _)| x).collect()
    }
    pub fn term_frequencies(&self) -> Vec<u32> {
        self.internal.iter().map(|&(_, _, x)| x).collect()
    }
}
