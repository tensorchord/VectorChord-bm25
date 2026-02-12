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
// Copyright (c) 2025 TensorChord Inc.

#![allow(clippy::len_without_is_empty)]
#![allow(clippy::manual_is_multiple_of)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::new_without_default)]
#![allow(clippy::not_unsafe_ptr_arg_deref)]
#![allow(unsafe_code)]

pub mod algorithm;
pub mod datatype;
pub mod guc;
pub mod index;
pub mod page;
pub mod segment;
pub mod utils;
pub mod weight;

pgrx::pg_module_magic!(
    name = c"vchord_bm25",
    version = {
        const RAW: &str = env!("VCHORD_BM25_VERSION");
        const BUFFER: [u8; RAW.len() + 1] = {
            let mut buffer = [0u8; RAW.len() + 1];
            let mut i = 0_usize;
            while i < RAW.len() {
                buffer[i] = RAW.as_bytes()[i];
                i += 1;
            }
            buffer
        };
        const STR: &::core::ffi::CStr =
            if let Ok(s) = ::core::ffi::CStr::from_bytes_with_nul(&BUFFER) {
                s
            } else {
                panic!("there are null characters in VCHORD_BM25_VERSION")
            };
        const { STR }
    }
);
const _: &str = include_str!("./sql/bootstrap.sql");
const _: &str = include_str!("./sql/finalize.sql");
pgrx::extension_sql_file!("./sql/bootstrap.sql", bootstrap);
pgrx::extension_sql_file!("./sql/finalize.sql", finalize);

#[pgrx::pg_guard]
#[unsafe(export_name = "_PG_init")]
unsafe extern "C-unwind" fn _pg_init() {
    index::init();
    guc::init();
}

#[cfg(not(all(target_endian = "little", target_pointer_width = "64")))]
compile_error!("Target is not supported.");
