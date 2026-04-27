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

use std::path::{Path, PathBuf};

pub struct TempFile {
    path: PathBuf,
}

impl TempFile {
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub fn tempfile() -> TempFile {
    let path = temppath();
    std::fs::File::create_new(&path).expect("failed to create the temporary file");
    TempFile { path }
}

pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

pub fn tempdir() -> TempDir {
    let path = temppath();
    std::fs::create_dir(&path).expect("failed to create the temporary directory");
    TempDir { path }
}

fn temppath() -> PathBuf {
    let tablespace_path = unsafe {
        use rand::seq::IndexedRandom;
        use std::mem::MaybeUninit;
        pgrx::pg_sys::PrepareTempTablespaces();
        let mut tablespaces = [pgrx::pg_sys::Oid::INVALID; 8];
        let length =
            pgrx::pg_sys::GetTempTablespaces(tablespaces.as_mut_ptr(), tablespaces.len() as _);
        let tablespace = tablespaces[..length as usize]
            .choose(&mut rand::rng())
            .copied()
            .unwrap_or(pgrx::pg_sys::Oid::INVALID);
        let tablespace = if tablespace != pgrx::pg_sys::Oid::INVALID {
            tablespace
        } else {
            pgrx::pg_sys::MyDatabaseTableSpace
        };
        let mut buf = [MaybeUninit::<std::ffi::c_char>::uninit(); pgrx::pg_sys::MAXPGPATH as usize];
        pgrx::pg_sys::TempTablespacePath(buf.as_mut_ptr().cast::<std::ffi::c_char>(), tablespace);
        let s = std::ffi::CStr::from_ptr(buf.as_ptr().cast::<std::ffi::c_char>());
        // It is reasonable to make this assumption because PostgreSQL
        // uses symbolic links internally to access tablespaces.
        let s = s.to_str().expect("found non-utf8 characters in the path");
        assert!(s.is_ascii(), "found non-ascii characters in the path");
        debug_assert!(s.starts_with("base/") || s.starts_with("pg_tblspc/"));
        debug_assert!(s.ends_with("/pgsql_tmp"));
        AsRef::<Path>::as_ref(s).to_path_buf()
    };
    if let Err(e) = std::fs::create_dir(&tablespace_path) {
        if e.kind() != std::io::ErrorKind::AlreadyExists {
            panic!("failed to create the temporary directory in the tablespace");
        }
    }
    let path = tablespace_path.join(crate::tempname());
    {
        // a leftover from a backend crash
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir_all(&path);
    }
    path
}
