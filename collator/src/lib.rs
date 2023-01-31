// Copyright 2020 Parity Technologies (UK) Ltd.
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

//! Collator for the PBA parachain.

use futures::channel::oneshot;
use parity_scale_codec::{Decode, Encode};
use pba_pvf::{execute, hash_state, BlockData, HeadData};
use polkadot_node_primitives::{
	Collation, CollationResult, CollationSecondedSignal, CollatorFn, MaybeCompressedPoV, PoV,
	Statement,
};
use polkadot_primitives::{CollatorId, CollatorPair};
use sp_core::{traits::SpawnNamed, Pair};
use std::{
	collections::HashMap,
	sync::{Arc, Mutex},
};

/// The amount we add when producing a new block.
const ADD: u64 = 7;

/// The state of the parachain.
struct StateDb {
	/// In real world, this is handled by the blockchain database.
	head_to_state: HashMap<HeadData, u64>,
}

impl StateDb {
	/// Init the genesis state.
	fn genesis() -> Self {
		let genesis_state =
			HeadData { number: 0, parent_hash: Default::default(), post_state: hash_state(0) };

		let mut map = HashMap::new();
		map.insert(genesis_state, 0);

		Self { head_to_state: map }
	}

	/// Advance the state and produce a new block based on the given `parent_head`.
	///
	/// Returns the new [`BlockData`] and the new [`HeadData`].
	fn advance(&mut self, parent_head: HeadData) -> (BlockData, HeadData) {
		let block = BlockData {
			state: self.head_to_state.get(&parent_head).copied().expect("unknown parent head"),
			add: ADD,
		};

		let new_head =
			execute(parent_head.hash(), parent_head, &block).expect("Produces valid block");

		self.head_to_state.insert(new_head.clone(), block.state.wrapping_add(ADD));

		(block, new_head)
	}
}

/// The collator of the parachain.
pub struct Collator {
	state: Arc<Mutex<StateDb>>,
	key: CollatorPair,
}

impl Collator {
	/// Create a new collator instance with the state initialized as genesis.
	pub fn new() -> Self {
		Self { state: Arc::new(Mutex::new(StateDb::genesis())), key: CollatorPair::generate().0 }
	}

	/// Get the SCALE encoded genesis head of the parachain.
	pub fn genesis_head(&self) -> Vec<u8> {
		StateDb::genesis().head_to_state.keys().next().unwrap().encode()
	}

	/// Get the validation code of the parachain.
	pub fn validation_code(&self) -> &[u8] {
		pba_pvf::wasm_binary_unwrap()
	}

	/// Get the collator key.
	pub fn collator_key(&self) -> CollatorPair {
		self.key.clone()
	}

	/// Get the collator id.
	pub fn collator_id(&self) -> CollatorId {
		self.key.public()
	}

	/// Create the collation function.
	///
	/// This collation function can be plugged into the overseer to generate collations for the parachain.
	pub fn create_collation_function(
		&self,
		spawner: impl SpawnNamed + Clone + 'static,
	) -> CollatorFn {
		use futures::FutureExt as _;

		let state = self.state.clone();

		Box::new(move |relay_parent, validation_data| {
			let parent = HeadData::decode(&mut &validation_data.parent_head.0[..])
				.expect("Decodes parent head");

			let (block_data, head_data) = state.lock().unwrap().advance(parent);

			log::info!(
				"created a new collation on relay-parent({}): {:?}",
				relay_parent,
				block_data,
			);

			let pov = PoV { block_data: block_data.encode().into() };

			let collation = Collation {
				upward_messages: Vec::new(),
				horizontal_messages: Vec::new(),
				new_validation_code: None,
				head_data: head_data.encode().into(),
				proof_of_validity: MaybeCompressedPoV::Raw(pov.clone()),
				processed_downward_messages: 0,
				hrmp_watermark: validation_data.relay_parent_number,
			};

			let compressed_pov = polkadot_node_primitives::maybe_compress_pov(pov);

			let (result_sender, recv) = oneshot::channel::<CollationSecondedSignal>();
			spawner.spawn(
				"pba-collator-seconded",
				None,
				async move {
					if let Ok(res) = recv.await {
						if !matches!(
							res.statement.payload(),
							Statement::Seconded(s) if s.descriptor.pov_hash == compressed_pov.hash(),
						) {
							log::error!(
								"Seconded statement should match our collation: {:?}",
								res.statement.payload()
							);
							std::process::exit(-1);
						}

						log::info!("Our collation was seconded! {:?}", res,);
					}
				}
				.boxed(),
			);

			async move { Some(CollationResult { collation, result_sender: Some(result_sender) }) }
				.boxed()
		})
	}
}
