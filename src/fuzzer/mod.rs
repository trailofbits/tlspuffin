#[cfg(test)]
mod tests;
mod libafl_setup;
mod harness;
mod mutations;
mod mutations_util;
mod error_observer;
pub mod seeds;

pub use libafl_setup::start;


// Link against correct sancov impl
#[cfg(all(feature = "sancov_pcguard_log", feature = "sancov_libafl"))]
compile_error!("`sancov_pcguard_log` and `sancov_libafl` features are mutually exclusive.");

/*#[cfg(all(any(feature = "sancov_pcguard_log", feature = "sancov_libafl"), test))]
compile_error!(
    "you can not enable `sancov_pcguard_log` or `sancov_libafl` in tests"
);*/

// Use log if explicitely enabled
#[cfg(all(not(test), feature = "sancov_pcguard_log"))]
mod sancov_pcguard_log;

// Use dummy in tests
#[cfg(test)]
mod sancov_dummy;


#[cfg(all(not(test), feature = "sancov_libafl"))]
// This import achieves that OpenSSl compiled with -fsanitize-coverage=trace-pc-guard can link
pub use libafl_targets::{EDGES_MAP, MAX_EDGES_NUM, CMP_MAP};

#[cfg(any(test, not(feature = "sancov_libafl")))]
pub const EDGES_MAP_SIZE: usize = 65536;
#[cfg(any(test, not(feature = "sancov_libafl")))]
pub static mut EDGES_MAP: [u8; EDGES_MAP_SIZE] = [0; EDGES_MAP_SIZE];
#[cfg(any(test, not(feature = "sancov_libafl")))]
pub static mut MAX_EDGES_NUM: usize = 0;
#[cfg(any(test, not(feature = "sancov_libafl")))]
pub const CMP_MAP_SIZE: usize = 65536;
#[cfg(any(test, not(feature = "sancov_libafl")))]
pub static mut CMP_MAP: [u8; CMP_MAP_SIZE] = [0; CMP_MAP_SIZE];