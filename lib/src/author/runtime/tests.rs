// Smoldot
// Copyright (C) 2019-2022  Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

#![cfg(test)]

use crate::{trie, verify::inherents};
use core::iter;

#[test]
fn block_building_works() {
    let chain_specs = crate::chain_spec::ChainSpec::from_json_bytes(
        &include_bytes!("example-chain-specs.json")[..],
    )
    .unwrap();
    let genesis_storage = chain_specs.genesis_storage().into_genesis_items().unwrap();

    let (chain_info, genesis_runtime) = chain_specs.to_chain_information().unwrap();
    let genesis_hash = chain_info.as_ref().finalized_block_header.hash(4);

    let mut builder = super::build_block(super::Config {
        block_number_bytes: 4,
        parent_runtime: genesis_runtime,
        parent_hash: &genesis_hash,
        parent_number: 0,
        block_body_capacity: 0,
        consensus_digest_log_item: super::ConfigPreRuntime::Aura(crate::header::AuraPreDigest {
            slot_number: 1234u64,
        }),
        max_log_level: 0,
    });

    loop {
        match builder {
            super::BlockBuild::Finished(Ok(success)) => {
                let decoded = crate::header::decode(&success.scale_encoded_header, 4).unwrap();
                assert_eq!(decoded.number, 1);
                assert_eq!(*decoded.parent_hash, genesis_hash);
                break;
            }
            super::BlockBuild::Finished(Err((err, _))) => panic!("{}", err),
            super::BlockBuild::ApplyExtrinsic(ext) => builder = ext.finish(),
            super::BlockBuild::ApplyExtrinsicResult { .. } => unreachable!(),
            super::BlockBuild::InherentExtrinsics(ext) => {
                builder = ext.inject_inherents(inherents::InherentData { timestamp: 1234 });
            }
            super::BlockBuild::StorageGet(get) => {
                let value = genesis_storage
                    .iter()
                    .find(|(k, _)| *k == get.key().as_ref())
                    .map(|(_, v)| iter::once(v));
                builder = get.inject_value(value.map(|v| (v, super::TrieEntryVersion::V0)));
            }
            super::BlockBuild::ClosestDescendantMerkleValue(req) => {
                builder = req.resume_unknown();
            }
            super::BlockBuild::NextKey(req) => {
                let mut search = trie::branch_search::BranchSearch::NextKey(
                    trie::branch_search::start_branch_search(trie::branch_search::Config {
                        key_before: req.key().collect::<Vec<_>>().into_iter(),
                        or_equal: req.or_equal(),
                        prefix: req.prefix().collect::<Vec<_>>().into_iter(),
                        no_branch_search: !req.branch_nodes(),
                    }),
                );

                let next_key = loop {
                    match search {
                        trie::branch_search::BranchSearch::Found {
                            branch_trie_node_key,
                        } => break branch_trie_node_key,
                        trie::branch_search::BranchSearch::NextKey(req) => {
                            let result = genesis_storage.iter().fold(None, |iter, (key, _)| {
                                if key < &req.key_before().collect::<Vec<_>>()[..]
                                    || (key == req.key_before().collect::<Vec<_>>()
                                        && !req.or_equal())
                                    || !key.starts_with(&req.prefix().collect::<Vec<_>>())
                                {
                                    return iter;
                                }

                                if iter.map_or(false, |iter| iter < key) {
                                    iter
                                } else {
                                    Some(key)
                                }
                            });

                            search = req.inject(result.map(|k| k.iter().copied()));
                        }
                    }
                };

                builder = req.inject_key(next_key.map(|nk| nk.into_iter()));
            }
        }
    }
}
