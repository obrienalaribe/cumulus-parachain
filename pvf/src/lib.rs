// Copyright 2017-2020 Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Basic parachain that adds a number as part of its state.

#![no_std]
#![cfg_attr(not(feature = "std"), feature(core_intrinsics, lang_items, alloc_error_handler))]

use parity_scale_codec::{Decode, Encode};

#[cfg(not(feature = "std"))]
mod validate_block;

// #[cfg(not(feature = "std"))]
// #[global_allocator]
// static ALLOC: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

/// Wasm binary unwrapped. If built with `BUILD_DUMMY_WASM_BINARY`, the function panics.
#[cfg(feature = "std")]
pub fn wasm_binary_unwrap() -> &'static [u8] {
	WASM_BINARY.expect(
		"Development wasm binary is not available. Testing is only \
						supported with the flag disabled.",
	)
}

/// Head data for this parachain.
#[derive(Default, Clone, Hash, Eq, PartialEq, Encode, Decode, Debug)]
pub struct HeadData {
	/// Block number.
	pub number: u64,
	/// Parent block hash.
	pub parent_hash: [u8; 32],
	/// Post-execution state hash.
	pub post_state: [u8; 32],
}

/// Block data for this parachain.
#[derive(Default, Clone, Encode, Decode, Debug)]
pub struct BlockData {
	/// State to begin from.
	pub state: u64,
	/// Amount to add (wrapping).
	pub add: u64,
}

pub fn hash(data: &[u8]) -> [u8; 32] {
	blake3::hash(data).into()
}

pub fn hash_state(state: u64) -> [u8; 32] {
	hash(state.encode().as_slice())
}

impl HeadData {
	pub fn hash(&self) -> [u8; 32] {
		hash(&self.encode())
	}
}

/// Start state mismatched with parent header's state hash.
#[derive(Debug)]
pub struct StateMismatch;

/// Execute a block body on top of given parent head, producing new parent head
/// if valid.
pub fn execute(
	parent_hash: [u8; 32],
	parent_head: HeadData,
	block_data: &BlockData,
) -> Result<HeadData, StateMismatch> {
	assert_eq!(parent_hash, parent_head.hash());

	if hash_state(block_data.state) != parent_head.post_state {
		return Err(StateMismatch)
	}

	let new_state = block_data.state.wrapping_add(block_data.add);

	Ok(HeadData { number: parent_head.number + 1, parent_hash, post_state: hash_state(new_state) })
}
