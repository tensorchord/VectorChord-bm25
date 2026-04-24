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

use crate::Opaque;
use crate::segment::{Mapping, Record, Segment};
use crate::tape::TapeWriter;
use crate::tuples::*;
use crate::types::Bm25IndexOptions;
use index::relation::{Page, RelationWrite};

pub fn build<R: RelationWrite, D, M>(
    bm25_options: Bm25IndexOptions,
    index: &R,
    seed: [u8; 32],
    segment: Segment<D, M>,
) where
    R::Page: Page<Opaque = Opaque>,
    D: IntoIterator<Item = Record>,
    M: IntoIterator<Item = Mapping>,
{
    let k1 = bm25_options.k1;
    let b = bm25_options.b;

    let mut meta = TapeWriter::<_, MetaTuple>::create(index);
    assert_eq!(meta.first(), 0);

    let flushed = crate::flush::flush(bm25_options.k1, bm25_options.b, index, segment);

    let tape_vectors = TapeWriter::<_, VectorTuple>::create(index);

    let mut tape_jump = TapeWriter::<_, JumpTuple>::create(index);
    let ptr_jump = tape_jump.push(JumpTuple {
        ptr_vectors: { tape_vectors }.first(),
        number_of_documents: flushed.number_of_documents,
        sum_of_document_lengths: flushed.sum_of_document_lengths,
        width_1_documents: flushed.width_1_documents,
        width_0_documents: flushed.width_0_documents,
        depth_documents: flushed.depth_documents,
        start_documents: flushed.start_documents,
        free_documents: flushed.free_documents,
        depth_tokens: flushed.depth_tokens,
        start_tokens: flushed.start_tokens,
        free_tokens: flushed.free_tokens,
        ptr_documents: flushed.ptr_documents,
        ptr_tokens: flushed.ptr_tokens,
        ptr_summaries: flushed.ptr_summaries,
        ptr_blocks: flushed.ptr_blocks,
    });
    assert_eq!(ptr_jump.1, 1);

    let tape_lock = TapeWriter::<_, ()>::create(index);

    meta.push(MetaTuple {
        k1,
        b,
        ptr_lock: { tape_lock }.first(),
        ptr_jump: ptr_jump.0,
        seed,
    });
}
