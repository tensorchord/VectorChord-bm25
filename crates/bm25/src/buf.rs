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

pub struct Buf {
    pub internal: [u32; 128],
    len: u8,
}

impl Buf {
    pub fn new() -> Self {
        Self {
            internal: [0u32; 128],
            len: 0,
        }
    }
    pub fn set_len(&mut self, new_len: u8) {
        assert!(new_len <= 128);
        self.len = new_len;
    }
    pub fn as_slice(&self) -> &[u32] {
        &self.internal[..self.len as usize]
    }
}
