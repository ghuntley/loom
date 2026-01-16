// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! eBPF audit loader for Linux systems.
//!
//! This module is only compiled on Linux with the `ebpf` feature enabled.
//! On other platforms, the audit sidecar runs in stub mode without eBPF monitoring.

#![cfg(all(feature = "ebpf", target_os = "linux"))]

// The LoaderError enum is intentionally large due to the aya::programs::ProgramError
// contained in the Attach variant. Boxing would add unnecessary complexity for
// error types that are only used at startup/initialization time.
#![allow(clippy::result_large_err)]

use std::fs;
use std::path::Path;

use aya::maps::RingBuf;
use aya::programs::TracePoint;
use aya::Ebpf;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tracing::{info, warn};

pub type Result<T> = std::result::Result<T, LoaderError>;

#[derive(Debug, Error)]
pub enum LoaderError {
	#[error("failed to load eBPF bytecode: {0}")]
	Load(#[from] aya::EbpfError),

	#[error("failed to attach program '{program}' to tracepoint '{tracepoint}': {source}")]
	Attach {
		program: String,
		tracepoint: String,
		#[source]
		source: aya::programs::ProgramError,
	},

	#[error("ring buffer map not found")]
	RingBufferNotFound,

	#[error("map error: {0}")]
	Map(#[from] aya::maps::MapError),

	#[error("program not found: {0}")]
	ProgramNotFound(String),

	#[error("io error: {0}")]
	Io(#[from] std::io::Error),

	#[error("eBPF bytecode integrity check failed: expected {expected}, got {actual}")]
	IntegrityCheckFailed { expected: String, actual: String },
}

const DEFAULT_EBPF_PATH: &str = "/opt/loom/ebpf/loom-weaver-ebpf";
const EBPF_HASH_PATH: &str = "/opt/loom/ebpf/loom-weaver-ebpf.sha256";
const RING_BUFFER_MAP_NAME: &str = "EVENTS";

fn verify_ebpf_integrity(bytecode_path: &Path) -> Result<()> {
	let hash_path = Path::new(EBPF_HASH_PATH);
	if !hash_path.exists() {
		warn!("eBPF hash file not found, skipping integrity check");
		return Ok(());
	}

	let expected_hash = fs::read_to_string(hash_path)?.trim().to_lowercase();

	let bytecode = fs::read(bytecode_path)?;
	let mut hasher = Sha256::new();
	hasher.update(&bytecode);
	let actual_hash = format!("{:x}", hasher.finalize());

	if actual_hash != expected_hash {
		return Err(LoaderError::IntegrityCheckFailed {
			expected: expected_hash,
			actual: actual_hash,
		});
	}

	info!("eBPF bytecode integrity verified");
	Ok(())
}

struct TracepointConfig {
	program_name: &'static str,
	category: &'static str,
	name: &'static str,
}

const TRACEPOINTS: &[TracepointConfig] = &[
	TracepointConfig {
		program_name: "sys_enter_execve",
		category: "syscalls",
		name: "sys_enter_execve",
	},
	TracepointConfig {
		program_name: "sys_exit_execve",
		category: "syscalls",
		name: "sys_exit_execve",
	},
	TracepointConfig {
		program_name: "sys_enter_openat",
		category: "syscalls",
		name: "sys_enter_openat",
	},
	TracepointConfig {
		program_name: "sys_enter_connect",
		category: "syscalls",
		name: "sys_enter_connect",
	},
	TracepointConfig {
		program_name: "sys_enter_clone",
		category: "syscalls",
		name: "sys_enter_clone",
	},
	TracepointConfig {
		program_name: "sys_exit_exit_group",
		category: "syscalls",
		name: "sys_exit_exit_group",
	},
	TracepointConfig {
		program_name: "sys_enter_setuid",
		category: "syscalls",
		name: "sys_enter_setuid",
	},
	TracepointConfig {
		program_name: "sys_enter_setgid",
		category: "syscalls",
		name: "sys_enter_setgid",
	},
	TracepointConfig {
		program_name: "sys_enter_ptrace",
		category: "syscalls",
		name: "sys_enter_ptrace",
	},
	TracepointConfig {
		program_name: "sys_enter_mmap",
		category: "syscalls",
		name: "sys_enter_mmap",
	},
	TracepointConfig {
		program_name: "sys_enter_mprotect",
		category: "syscalls",
		name: "sys_enter_mprotect",
	},
	TracepointConfig {
		program_name: "sys_enter_unshare",
		category: "syscalls",
		name: "sys_enter_unshare",
	},
	TracepointConfig {
		program_name: "sys_enter_setns",
		category: "syscalls",
		name: "sys_enter_setns",
	},
	TracepointConfig {
		program_name: "sys_enter_mount",
		category: "syscalls",
		name: "sys_enter_mount",
	},
];

pub struct EbpfAuditLoader {
	bpf: Ebpf,
	attached_count: usize,
}

impl EbpfAuditLoader {
	pub fn new() -> Result<Self> {
		Self::from_path(Path::new(DEFAULT_EBPF_PATH))
	}

	pub fn from_path(path: &Path) -> Result<Self> {
		info!(path = %path.display(), "Loading eBPF bytecode");

		verify_ebpf_integrity(path)?;

		let bytecode = std::fs::read(path)?;
		Self::from_bytes(&bytecode)
	}

	pub fn from_bytes(bytecode: &[u8]) -> Result<Self> {
		let mut bpf = aya::EbpfLoader::new().load(bytecode)?;

		let mut attached_count = 0;

		for config in TRACEPOINTS {
			match Self::attach_tracepoint(&mut bpf, config) {
				Ok(()) => {
					attached_count += 1;
					info!(
						program = config.program_name,
						category = config.category,
						name = config.name,
						"Attached tracepoint"
					);
				}
				Err(e) => {
					warn!(
						program = config.program_name,
						category = config.category,
						name = config.name,
						error = %e,
						"Failed to attach tracepoint, continuing without it"
					);
				}
			}
		}

		info!(
			attached_count,
			total = TRACEPOINTS.len(),
			"eBPF programs loaded"
		);

		Ok(Self {
			bpf,
			attached_count,
		})
	}

	fn attach_tracepoint(bpf: &mut Ebpf, config: &TracepointConfig) -> Result<()> {
		let program: &mut TracePoint = bpf
			.program_mut(config.program_name)
			.ok_or_else(|| LoaderError::ProgramNotFound(config.program_name.to_string()))?
			.try_into()
			.map_err(|e| LoaderError::Attach {
				program: config.program_name.to_string(),
				tracepoint: format!("{}:{}", config.category, config.name),
				source: e,
			})?;

		program.load().map_err(|e| LoaderError::Attach {
			program: config.program_name.to_string(),
			tracepoint: format!("{}:{}", config.category, config.name),
			source: e,
		})?;

		program
			.attach(config.category, config.name)
			.map_err(|e| LoaderError::Attach {
				program: config.program_name.to_string(),
				tracepoint: format!("{}:{}", config.category, config.name),
				source: e,
			})?;

		Ok(())
	}

	pub fn ring_buffer(&mut self) -> Result<RingBuf<&mut aya::maps::MapData>> {
		let map = self
			.bpf
			.map_mut(RING_BUFFER_MAP_NAME)
			.ok_or(LoaderError::RingBufferNotFound)?;
		RingBuf::try_from(map).map_err(LoaderError::Map)
	}

	pub fn attached_count(&self) -> usize {
		self.attached_count
	}

	pub fn total_programs(&self) -> usize {
		TRACEPOINTS.len()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_tracepoint_count() {
		assert_eq!(TRACEPOINTS.len(), 14);
	}

	#[test]
	fn test_all_tracepoints_have_valid_names() {
		for config in TRACEPOINTS {
			assert!(!config.program_name.is_empty());
			assert!(!config.category.is_empty());
			assert!(!config.name.is_empty());
			assert!(config.name.starts_with("sys_enter_") || config.name.starts_with("sys_exit_"));
		}
	}
}
