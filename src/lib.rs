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

#[cfg(any(test, feature = "pg_test"))]
pub mod tests;

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

#[cfg(not(any(
    feature = "pg13",
    feature = "pg14",
    feature = "pg15",
    feature = "pg16",
    feature = "pg17",
    feature = "pg18"
)))]
compiler_error!("PostgreSQL version must be selected.");

// const SCHEMA: &str = "bm25_catalog";

// const SCHEMA_C_BYTES: [u8; SCHEMA.len() + 1] = {
//     let mut bytes = [0u8; SCHEMA.len() + 1];
//     let mut i = 0_usize;
//     while i < SCHEMA.len() {
//         bytes[i] = SCHEMA.as_bytes()[i];
//         i += 1;
//     }
//     bytes
// };

// const SCHEMA_C_STR: &std::ffi::CStr = match std::ffi::CStr::from_bytes_with_nul(&SCHEMA_C_BYTES) {
//     Ok(x) => x,
//     Err(_) => panic!("there are null characters in schema"),
// };

#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {
        // perform one-off initialization when the pg_test framework starts
    }

    pub fn postgresql_conf_options() -> Vec<&'static str> {
        vec![r#"search_path = '"$user", public, bm25_catalog'"#]
    }
}
