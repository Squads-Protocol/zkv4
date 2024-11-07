use anchor_lang::prelude::*;
use light_sdk::{
    compressed_account::LightAccount, context::LightContext, light_account, light_accounts,
    merkle_context::PackedAddressMerkleContext,
};
use light_sdk::{light_system_accounts, LightTraits, CPI_AUTHORITY_PDA_SEED};

use crate::errors::*;
use crate::state::*;
use crate::utils::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct VaultTransactionCreateArgs {
    /// Index of the vault this transaction belongs to.
    pub vault_index: u8,
    /// Number of ephemeral signing PDAs required by the transaction.
    pub ephemeral_signers: u8,
    pub transaction_message: Vec<u8>,
    pub memo: Option<String>,
    pub compression_args: MutateCompressedMultisigArgs,
}
#[light_system_accounts]
#[derive(Accounts, LightTraits)]
#[instruction(args: VaultTransactionCreateArgs)]
pub struct VaultTransactionCreate<'info> {
    /// Multsig Account (empty) -- Validation of data gets performed by the
    /// light system program
    #[account(
        seeds = [SEED_PREFIX, SEED_MULTISIG, &args.compression_args.multisig_data.create_key.as_ref()],
        bump = args.compression_args.multisig_data.bump,
    )]
    pub multisig: AccountInfo<'info>,

    #[account(
        init,
        payer = rent_payer,
        space = VaultTransaction::size(args.ephemeral_signers, &args.transaction_message)?,
        seeds = [
            SEED_PREFIX,
            multisig.key().as_ref(),
            SEED_TRANSACTION,
            &args.compression_args.multisig_data.transaction_index.checked_add(1).unwrap().to_le_bytes(),
        ],
        bump
    )]
    pub transaction: Account<'info, VaultTransaction>,

    /// The member of the multisig that is creating the transaction.
    pub creator: Signer<'info>,

    /// The payer for the transaction account rent.
    #[fee_payer]
    #[account(mut)]
    pub rent_payer: Signer<'info>,

    #[authority]
    #[account(
        seeds = [CPI_AUTHORITY_PDA_SEED],
        bump,
    )]
    pub cpi_authority: AccountInfo<'info>,

    #[self_program]
    pub squads_program: Program<'info, crate::program::SquadsMultisigProgram>,
}

impl<'info> VaultTransactionCreate<'info> {
    pub fn validate(&self, args: &VaultTransactionCreateArgs) -> Result<()> {
        let Self {
            creator,
            transaction,
            ..
        } = self;
        let multisig = LightMultisig::from(&args.compression_args.multisig_data);

        // We're validating on user passed data, but its not dangerous since the
        // instruction will fail if the data is invalid.
        require!(
            multisig.is_member(creator.key()).is_some(),
            MultisigError::NotAMember
        );
        require!(
            multisig.member_has_permission(creator.key(), Permission::Initiate),
            MultisigError::Unauthorized
        );

        Ok(())
    }

    /// Create a new vault transaction.
    #[access_control(ctx.accounts.validate(&args))]
    pub fn vault_transaction_create(
        ctx: Context<'_, '_, 'info, 'info, Self>,
        args: VaultTransactionCreateArgs,
    ) -> Result<()> {
        let mut multisig = LightMultisig::from(&args.compression_args.multisig_data);
        multisig.compressed_verify_state(&ctx, &args.compression_args)?;

        let multisig_key = ctx.accounts.multisig.key();
        let transaction = &mut ctx.accounts.transaction;
        let creator = &mut ctx.accounts.creator;

        let transaction_message =
            TransactionMessage::deserialize(&mut args.transaction_message.as_slice())?;

        let transaction_key = transaction.key();
        let transaction_bump = Pubkey::find_program_address(
            &[
                &SEED_PREFIX.to_vec(),
                &SEED_MULTISIG.to_vec(),
                &multisig_key.to_bytes(),
                &SEED_TRANSACTION.to_vec(),
                &multisig
                    .transaction_index
                    .checked_add(1)
                    .unwrap()
                    .to_le_bytes(),
            ],
            &crate::id(),
        )
        .1;
        let vault_seeds = &[
            SEED_PREFIX,
            multisig_key.as_ref(),
            SEED_VAULT,
            &args.vault_index.to_le_bytes(),
        ];
        let (_, vault_bump) = Pubkey::find_program_address(vault_seeds, ctx.program_id);

        let ephemeral_signer_bumps: Vec<u8> = (0..args.ephemeral_signers)
            .map(|ephemeral_signer_index| {
                let ephemeral_signer_seeds = &[
                    SEED_PREFIX,
                    transaction_key.as_ref(),
                    SEED_EPHEMERAL_SIGNER,
                    &ephemeral_signer_index.to_le_bytes(),
                ];

                let (_, bump) =
                    Pubkey::find_program_address(ephemeral_signer_seeds, ctx.program_id);
                bump
            })
            .collect();

        // Increment the transaction index.
        let transaction_index = multisig.transaction_index.checked_add(1).unwrap();

        // Initialize the transaction fields.
        transaction.multisig = multisig_key;
        transaction.creator = creator.key();
        transaction.index = transaction_index;
        transaction.bump = transaction_bump;
        transaction.vault_bump = vault_bump;
        transaction.ephemeral_signer_bumps = ephemeral_signer_bumps;
        transaction.message = transaction_message.try_into()?;

        // Updated last transaction index in the multisig account.
        multisig.transaction_index = transaction_index;

        multisig.invariant()?;

        // Logs for indexing.
        msg!("transaction index: {}", transaction_index);

        // Mutate the compressed multisig account
        let cpi_authority_bump: u8 = ctx.bumps.cpi_authority;
        let cpi_authority_signer_seeds = [CPI_AUTHORITY_PDA_SEED, &[cpi_authority_bump]];

        multisig.compressed_mutate(ctx, args.compression_args, &[&cpi_authority_signer_seeds])?;

        Ok(())
    }
}

/// Unvalidated instruction data, must be treated as untrusted.
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct TransactionMessage {
    /// The number of signer pubkeys in the account_keys vec.
    pub num_signers: u8,
    /// The number of writable signer pubkeys in the account_keys vec.
    pub num_writable_signers: u8,
    /// The number of writable non-signer pubkeys in the account_keys vec.
    pub num_writable_non_signers: u8,
    /// The list of unique account public keys (including program IDs) that will be used in the provided instructions.
    pub account_keys: SmallVec<u8, Pubkey>,
    /// The list of instructions to execute.
    pub instructions: SmallVec<u8, CompiledInstruction>,
    /// List of address table lookups used to load additional accounts
    /// for this transaction.
    pub address_table_lookups: SmallVec<u8, MessageAddressTableLookup>,
}

// Concise serialization schema for instructions that make up transaction.
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct CompiledInstruction {
    pub program_id_index: u8,
    /// Indices into the tx's `account_keys` list indicating which accounts to pass to the instruction.
    pub account_indexes: SmallVec<u8, u8>,
    /// Instruction data.
    pub data: SmallVec<u16, u8>,
}

/// Address table lookups describe an on-chain address lookup table to use
/// for loading more readonly and writable accounts in a single tx.
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct MessageAddressTableLookup {
    /// Address lookup table account key
    pub account_key: Pubkey,
    /// List of indexes used to load writable account addresses
    pub writable_indexes: SmallVec<u8, u8>,
    /// List of indexes used to load readonly account addresses
    pub readonly_indexes: SmallVec<u8, u8>,
}
