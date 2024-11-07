use account_compression::{
    utils::check_discrimininator::check_discriminator, StateMerkleTreeAccount,
};
use anchor_lang::{prelude::*, Bumps};
use core::mem;
use light_concurrent_merkle_tree::zero_copy::ConcurrentMerkleTreeZeroCopy;
use light_hasher::Poseidon;
use light_sdk::{
    compressed_account::PackedCompressedAccountWithMerkleContext, traits::InvokeAccounts,
};
use light_utils::hash_to_bn254_field_size_be;
pub use light_verifier::verify_merkle_proof_zkp;

use crate::errors::MultisigError;

#[inline(never)]
pub fn fetch_input_compressed_account_root<
    'a,
    'b,
    'c: 'info,
    'info,
    A: InvokeAccounts<'info> + Bumps,
>(
    input_compressed_account_with_merkle_context: &'a PackedCompressedAccountWithMerkleContext,
    ctx: &'a Context<'a, 'b, 'c, 'info, A>,
) -> Result<[u8; 32]> {
    let merkle_tree = &ctx.remaining_accounts[input_compressed_account_with_merkle_context
        .merkle_context
        .merkle_tree_pubkey_index as usize];
    let merkle_tree = merkle_tree.try_borrow_data()?;
    check_discriminator::<StateMerkleTreeAccount>(&merkle_tree)?;

    let merkle_tree = ConcurrentMerkleTreeZeroCopy::<Poseidon, 26>::from_bytes_zero_copy(
        &merkle_tree[8 + mem::size_of::<StateMerkleTreeAccount>()..],
    )
    .map_err(|_err| MultisigError::InvalidMerkleProof)?;

    let fetched_roots = &merkle_tree.roots;
    Ok(fetched_roots[input_compressed_account_with_merkle_context.root_index as usize])
}

#[inline(never)]
pub fn hash_input_compressed_account<'a, 'b, 'c: 'info, 'info>(
    remaining_accounts: &'a [AccountInfo<'info>],
    input_compressed_account_with_merkle_context: &'a PackedCompressedAccountWithMerkleContext,
) -> Result<([u8; 32], Option<[u8; 32]>)> {
    if input_compressed_account_with_merkle_context
        .merkle_context
        .queue_index
        .is_some()
    {
        unimplemented!("Queue index is not supported.");
    }

    let owner_pubkey = input_compressed_account_with_merkle_context
        .compressed_account
        .owner;
    let hashed_owner = hash_to_bn254_field_size_be(&owner_pubkey.to_bytes())
        .unwrap()
        .0;

    let merkle_tree_pubkey = remaining_accounts[input_compressed_account_with_merkle_context
        .merkle_context
        .merkle_tree_pubkey_index as usize]
        .key();
    let hashed_mt = hash_to_bn254_field_size_be(&merkle_tree_pubkey.to_bytes())
        .unwrap()
        .0;

    let leaf = input_compressed_account_with_merkle_context
        .compressed_account
        .hash_with_hashed_values::<Poseidon>(
            &hashed_owner,
            &hashed_mt,
            &input_compressed_account_with_merkle_context
                .merkle_context
                .leaf_index,
        )?;

    let address = input_compressed_account_with_merkle_context
        .compressed_account
        .address;

    Ok((leaf, address))
}
