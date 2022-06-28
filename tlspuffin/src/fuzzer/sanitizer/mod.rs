#[cfg(all(feature = "sancov_pcguard_log", feature = "sancov_libafl"))]
compile_error!("`sancov_pcguard_log` and `sancov_libafl` features are mutually exclusive.");

cfg_if::cfg_if! {
    if #[cfg(test)] {
        // Use dummy in tests and benchmarking
        pub mod sancov_dummy;
        pub const EDGES_MAP_SIZE: usize = 65536;
        pub static mut EDGES_MAP: [u8; EDGES_MAP_SIZE] = [0; EDGES_MAP_SIZE];
        pub static mut MAX_EDGES_NUM: usize = 0;
    } else {
        #[allow(unused_imports)]
        // This import achieves that OpenSSl compiled with -fsanitize-coverage=trace-pc-guard can link
        pub use libafl_targets;
    }
}

cfg_if::cfg_if! {
    if #[cfg(all(not(test), feature = "sancov_pcguard_log"))] {
        pub mod sancov_pcguard_log;
        pub const EDGES_MAP_SIZE: usize = 65536;
        pub static mut EDGES_MAP: [u8; EDGES_MAP_SIZE] = [0; EDGES_MAP_SIZE];
        pub static mut MAX_EDGES_NUM: usize = 0;
    } else if #[cfg(all(not(test), feature = "sancov_libafl"))] {
        pub use libafl_targets::{EDGES_MAP, MAX_EDGES_NUM};
    }
}

// Unused
// pub const CMP_MAP_SIZE: usize = 65536;
// pub static mut CMP_MAP: [u8; CMP_MAP_SIZE] = [0; CMP_MAP_SIZE];
