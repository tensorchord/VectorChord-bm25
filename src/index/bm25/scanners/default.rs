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

use crate::datatype::memory_bm25vector::Bm25VectorOutput;
use crate::index::bm25::scanners::SearchOptions;
use crate::index::fetcher::*;
use crate::index::scanners::SearchBuilder;
use always_equal::AlwaysEqual;
use bm25::vector::Bm25VectorOwned;
use index::relation::{Page, RelationRead};
use pgrx::heap_tuple::PgHeapTuple;
use std::cmp::Reverse;
use std::num::NonZero;

pub struct DefaultBuilder {
    orderbys: Vec<Option<Bm25VectorOwned>>,
}

impl SearchBuilder for DefaultBuilder {
    type Options = SearchOptions;

    type Opfamily = ();

    type Opaque = bm25::Opaque;

    fn new((): ()) -> Self {
        Self {
            orderbys: Vec::new(),
        }
    }

    unsafe fn add(&mut self, strategy: u16, value: Option<pgrx::pg_sys::Datum>) {
        match strategy {
            1 => {
                let document = 'block: {
                    use pgrx::datum::FromDatum;
                    let Some(datum) = value else {
                        break 'block None;
                    };
                    if datum.is_null() {
                        break 'block None;
                    }
                    let tuple = unsafe {
                        PgHeapTuple::<'_, pgrx::AllocatedByRust>::from_datum(datum, false).unwrap()
                    };
                    let query: Bm25VectorOutput = match tuple.get_by_index(NonZero::new(2).unwrap())
                    {
                        Ok(Some(s)) => s,
                        Ok(None) => pgrx::error!("Bad input: empty vector at bm25query"),
                        Err(_) => unreachable!(),
                    };
                    Some(query.as_borrowed().own())
                };
                self.orderbys.push(document);
            }
            _ => unreachable!(),
        }
    }

    fn build<'b, R>(
        self,
        index: R,
        options: SearchOptions,
        mut fetcher: impl Fetcher + 'b,
    ) -> Box<dyn Iterator<Item = (f64, [u16; 3], bool)> + 'b>
    where
        R: RelationRead,
        R::Page: Page<Opaque = bm25::Opaque>,
    {
        let mut vector = None;
        for orderby_vector in self.orderbys.into_iter().flatten() {
            if vector.is_none() {
                vector = Some(orderby_vector);
            } else {
                pgrx::error!("vector search with multiple vectors is not supported");
            }
        }
        let Some(vector) = vector else {
            return Box::new(std::iter::empty()) as Box<dyn Iterator<Item = (f64, [u16; 3], bool)>>;
        };
        let Some(limit) = NonZero::new(options.limit as usize) else {
            pgrx::error!("number of needed rows is set to 0");
        };
        let result = if !options.prefilter {
            bm25::search(&index, limit, vector.as_borrowed(), |_| true)
        } else {
            bm25::search(&index, limit, vector.as_borrowed(), |pointer| {
                let Some(mut tuple) = fetcher.fetch(pointer) else {
                    return false;
                };
                if !tuple.filter() {
                    return false;
                }
                true
            })
        };
        let iter = result
            .into_iter()
            .map(move |(Reverse(score), AlwaysEqual(pointer))| (score.to_f64(), pointer, false));
        Box::new(iter)
    }
}
