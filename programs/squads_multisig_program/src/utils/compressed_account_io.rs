use anchor_lang::prelude::*;
use borsh::BorshSerialize;
use light_hasher::{DataHasher, Discriminator, Poseidon};
use light_sdk::{
    compressed_account::{
        CompressedAccount, CompressedAccountData, OutputCompressedAccountWithPackedContext,
        PackedCompressedAccountWithMerkleContext,
    },
    merkle_context::PackedMerkleContext,
    CPI_AUTHORITY_PDA_SEED,
};

pub fn get_compressed_account<T>(
    account: &T,
    address: &[u8; 32],
    program_id: &Pubkey,
) -> Result<CompressedAccount>
where
    T: BorshSerialize + DataHasher + Discriminator,
{
    let data = account.try_to_vec()?;
    let data_hash = account.hash::<Poseidon>().map_err(ProgramError::from)?;
    let compressed_account_data = CompressedAccountData {
        discriminator: T::discriminator(),
        data,
        data_hash,
    };

    let compressed_account = CompressedAccount {
        owner: *program_id,
        lamports: 0,
        address: Some(*address),
        data: Some(compressed_account_data),
    };

    Ok(compressed_account)
}

pub fn input_compressed_account<T>(
    account: &T,
    address: &[u8; 32],
    program_id: &Pubkey,
    merkle_context: &PackedMerkleContext,
    merkle_tree_root_index: u16,
) -> Result<PackedCompressedAccountWithMerkleContext>
where
    T: BorshSerialize + DataHasher + Discriminator,
{
    let compressed_account = get_compressed_account(account, address, program_id)?;

    Ok(PackedCompressedAccountWithMerkleContext {
        compressed_account,
        merkle_context: *merkle_context,
        root_index: merkle_tree_root_index,
        read_only: false,
    })
}

pub fn output_compressed_account<T>(
    account: &T,
    address: &[u8; 32],
    program_id: &Pubkey,
    merkle_context: &PackedMerkleContext,
) -> Result<OutputCompressedAccountWithPackedContext>
where
    T: BorshSerialize + DataHasher + Discriminator,
{
    let compressed_account = get_compressed_account(account, address, program_id)?;

    Ok(OutputCompressedAccountWithPackedContext {
        compressed_account,
        merkle_tree_index: merkle_context.merkle_tree_pubkey_index,
    })
}

pub fn get_cpi_authority_seeds<'a>(bump: &'a u8) -> [&'a [u8]; 2] {
    let seeds = [CPI_AUTHORITY_PDA_SEED, std::slice::from_ref(bump)];
    seeds
}
