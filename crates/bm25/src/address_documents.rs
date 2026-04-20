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
use crate::tape::BackwardTapeWriter;
use crate::tuples::{AddressDocumentsTuple, WithReader};
use index::relation::{Page, PageGuard, RelationRead, RelationWrite};
use std::cmp::Ordering;

// The `write` function imposes strict constraints on its input.
// * All tuples must be evenly distributed across either `n + 1` pages or `n` pages.
// * On each page, tuples must be contiguous and ordered, starting from `1`.
// * The first `n` pages must contain the same number of tuples.

pub fn write<R: RelationWrite>(index: &R, elements: &[(u32, u16)]) -> (u16, u16, u32, u32, u32)
where
    R::Page: Page<Opaque = Opaque>,
{
    let mut tape = BackwardTapeWriter::<_, AddressDocumentsTuple>::create(index);
    let width_1 = AddressDocumentsTuple::fit(tape.freespace())
        .expect("implementation: a blank page cannot fit a single tuple") as u16;
    let width_0 = {
        let mut state: Option<(u16, bool)> = None;
        for chunk in elements.chunk_by(|(l, _), (r, _)| l == r) {
            for (index, (_, element)) in chunk.iter().copied().enumerate() {
                if index + 1 != element as usize {
                    panic!("tuples are not continuous");
                }
            }
            match state {
                None => state = Some((chunk.len() as u16, false)),
                Some((w, false)) => match Ord::cmp(&(chunk.len() as u16), &w) {
                    Ordering::Less => state = Some((w, true)),
                    Ordering::Equal => (),
                    Ordering::Greater => panic!("tuples are not continuous"),
                },
                Some((_, true)) => panic!("tuples are not continuous"),
            }
        }
        state.map(|(w, _)| w).unwrap_or(1)
    };
    let mut buffer = Vec::new();
    for chunk in elements.chunks(width_0 as usize) {
        buffer.push(chunk[0].0);
    }
    let mut depth = 0_u32;
    while buffer.len() > 1 {
        depth += 1;
        for chunk in core::mem::take(&mut buffer).chunks(width_1 as usize) {
            let tuple = AddressDocumentsTuple {
                internal: chunk.to_vec(),
            };
            buffer.push(tape.tape_put(tuple).0);
            tape.tape_move();
        }
    }
    let start = buffer.first().copied().unwrap_or(u32::MAX);
    let free = tape.into_head().id();
    (width_1, width_0, depth, start, free)
}

pub fn read<R: RelationRead>(
    index: &R,
    width_1: u16,
    width_0: u16,
    depth: u32,
    start: u32,
    document_id: u32,
) -> Option<(R::ReadGuard<'_>, u16)> {
    if start == u32::MAX {
        return None;
    }
    let digits = {
        let mut digits = [0_u32; 32];
        let mut number = document_id / width_0 as u32;
        for i in 0..depth {
            digits[i as usize] = number % width_1 as u32;
            number /= width_1 as u32;
        }
        digits
    };
    let mut id = start;
    for digit in digits[..depth as usize].iter().copied().rev() {
        let address_guard = index.read(id);
        let address_bytes = address_guard.get(1).expect("data corruption");
        let address_tuple = AddressDocumentsTuple::deserialize_ref(address_bytes);
        let internal = address_tuple.internal();
        id = *internal.get(digit as usize)?;
    }
    let i = (document_id % width_0 as u32) as u16;
    let document_guard = index.read(id);
    if i < document_guard.len() {
        Some((document_guard, i + 1))
    } else {
        None
    }
}
