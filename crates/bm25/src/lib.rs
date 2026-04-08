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

#[repr(C, align(8))]
#[derive(Debug, Clone, Copy, PartialEq, FromBytes, IntoBytes, Immutable, KnownLayout)]
pub struct Opaque {
    pub next: u32,
    pub index: u32,
}

#[allow(unsafe_code)]
unsafe impl index::relation::Opaque for Opaque {}

pub use build::build;
pub use bulkdelete::bulkdelete;
pub use evaluate::evaluate;
pub use insert::insert;
pub use maintain::maintain;
pub use search::search;
pub use segment::Segment;

fn idf(number_of_documents: u32, token_number_of_documents: u32) -> f64 {
    let number_of_documents = number_of_documents as f64;
    let token_number_of_documents = token_number_of_documents as f64;
    ((number_of_documents + 1.0) / (token_number_of_documents + 0.5)).ln()
}

fn tf(k1: f64, b: f64, avgdl: f64, document_length: u32, term_frequency: u32) -> f64 {
    let term_frequency = term_frequency as f64;
    let document_length = document_length as f64;
    (term_frequency * (k1 + 1.0)) / (term_frequency + k1 * (1.0 - b + b * document_length / avgdl))
}
