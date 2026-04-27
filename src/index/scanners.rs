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

use crate::index::fetcher::Fetcher;
use index::relation::{Page, RelationId, RelationRead};
use pgrx::pg_sys::Datum;

pub trait SearchBuilder: 'static {
    type Options;

    type Opfamily;

    type Opaque: Copy;

    fn new<R>(relation: R, opfamily: Self::Opfamily) -> Self
    where
        R: RelationId + RelationRead,
        R::Page: Page<Opaque = Self::Opaque>;

    unsafe fn add(&mut self, strategy: u16, datum: Option<Datum>);

    fn build<'b, R>(
        self,
        relation: R,
        options: Self::Options,
        fetcher: impl Fetcher + 'b,
    ) -> Box<dyn Iterator<Item = (f64, [u16; 3])> + 'b>
    where
        R: RelationRead,
        R::Page: Page<Opaque = Self::Opaque>;
}
