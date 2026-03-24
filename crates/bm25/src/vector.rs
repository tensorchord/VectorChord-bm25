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

#[derive(Debug, Clone)]
pub struct Bm25VectorOwned {
    indexes: Vec<u32>,
    values: Vec<u32>,
}

impl Bm25VectorOwned {
    #[inline(always)]
    pub fn new(indexes: Vec<u32>, values: Vec<u32>) -> Self {
        Self::new_checked(indexes, values).expect("invalid data")
    }

    #[inline(always)]
    pub fn new_checked(indexes: Vec<u32>, values: Vec<u32>) -> Option<Self> {
        if indexes.len() != values.len() {
            return None;
        }
        if u32::try_from(indexes.len()).is_err() {
            return None;
        }
        if !indexes.is_sorted_by(|a, b| a < b) {
            return None;
        }
        if !values.iter().all(|&x| x != 0) {
            return None;
        }
        if values.iter().fold(0_u64, |x, &y| x + y as u64) > u32::MAX as u64 {
            return None;
        }
        #[allow(unsafe_code)]
        unsafe {
            Some(Self::new_unchecked(indexes, values))
        }
    }

    /// # Safety
    ///
    /// * `indexes` must have the same length as `values`.
    /// * Length must not exceed `u32::MAX`.
    /// * `indexes` must be a strictly increasing sequence.
    /// * `values` must not contain zero.
    /// * Sum of `values` must not exceed `u32::MAX`.
    #[allow(unsafe_code)]
    #[inline(always)]
    pub unsafe fn new_unchecked(indexes: Vec<u32>, values: Vec<u32>) -> Self {
        Self { indexes, values }
    }

    #[inline(always)]
    pub fn indexes(&self) -> &[u32] {
        &self.indexes
    }

    #[inline(always)]
    pub fn values(&self) -> &[u32] {
        &self.values
    }
}

impl Bm25VectorOwned {
    // type Borrowed<'a> = SVectBorrowed<'a>;

    #[inline(always)]
    pub fn as_borrowed(&self) -> Bm25VectorBorrowed<'_> {
        Bm25VectorBorrowed {
            indexes: &self.indexes,
            values: &self.values,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Bm25VectorBorrowed<'a> {
    indexes: &'a [u32],
    values: &'a [u32],
}

impl<'a> Bm25VectorBorrowed<'a> {
    #[inline(always)]
    pub fn new(indexes: &'a [u32], values: &'a [u32]) -> Self {
        Self::new_checked(indexes, values).expect("invalid data")
    }

    #[inline(always)]
    pub fn new_checked(indexes: &'a [u32], values: &'a [u32]) -> Option<Self> {
        if indexes.len() != values.len() {
            return None;
        }
        if u32::try_from(indexes.len()).is_err() {
            return None;
        }
        if !indexes.is_sorted_by(|a, b| a < b) {
            return None;
        }
        if !values.iter().all(|&x| x != 0) {
            return None;
        }
        if values.iter().fold(0_u64, |x, &y| x + y as u64) > u32::MAX as u64 {
            return None;
        }
        #[allow(unsafe_code)]
        unsafe {
            Some(Self::new_unchecked(indexes, values))
        }
    }

    /// # Safety
    ///
    /// * `indexes` must have the same length as `values`.
    /// * Length must not exceed `u32::MAX`.
    /// * `indexes` must be a strictly increasing sequence.
    /// * `values` must not contain zero.
    /// * Sum of `values` must not exceed `u32::MAX`.
    #[inline(always)]
    #[allow(unsafe_code)]
    pub unsafe fn new_unchecked(indexes: &'a [u32], values: &'a [u32]) -> Self {
        Self { indexes, values }
    }

    #[inline(always)]
    pub fn indexes(&self) -> &'a [u32] {
        self.indexes
    }

    #[inline(always)]
    pub fn values(&self) -> &'a [u32] {
        self.values
    }

    #[inline(always)]
    pub fn len(&self) -> u32 {
        self.indexes.len() as u32
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.indexes.is_empty()
    }

    #[inline(always)]
    pub fn norm(&self) -> u32 {
        self.values.iter().sum()
    }
}

impl Bm25VectorBorrowed<'_> {
    // type Owned = SVectOwned<u32>;

    #[inline(always)]
    pub fn own(&self) -> Bm25VectorOwned {
        Bm25VectorOwned {
            indexes: self.indexes.to_vec(),
            values: self.values.to_vec(),
        }
    }
}
