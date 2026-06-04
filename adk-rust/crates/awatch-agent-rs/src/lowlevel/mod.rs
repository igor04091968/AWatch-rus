//! Safe low-level extension point for future agent optimizations.
//!
//! The default implementation is pure Rust. The `asm-lowlevel` feature is a
//! reserved integration point for future platform-specific CPU and timing
//! probes implemented through `core::arch`, inline assembly, or native OS
//! calls. Keep any future `unsafe` code isolated inside this module.

pub mod cpu;
pub mod timing;

pub use cpu::{CpuFeatures, get_cpu_features};
pub use timing::{high_precision_time_ns, monotonic_ticks};

#[cfg(feature = "asm-lowlevel")]
pub const LOWLEVEL_BACKEND: &str = "asm-lowlevel-ready-rust-fallback";

#[cfg(not(feature = "asm-lowlevel"))]
pub const LOWLEVEL_BACKEND: &str = "rust";
