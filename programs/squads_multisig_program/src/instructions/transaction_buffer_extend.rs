use anchor_lang::prelude::*;
use light_sdk::{light_system_accounts, LightTraits, CPI_AUTHORITY_PDA_SEED};

use crate::errors::*;
use crate::state::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct TransactionBufferExtendArgs {
    // Buffer to extend the TransactionBuffer with.
    pub buffer: Vec<u8>,
    /// Compression args for the multisig account
        pub compression_args: MutateOrVerifyCompressedMultisigArgs,
}

#[light_system_accounts]
#[derive(Accounts, LightTraits)]
#[instruction(args: TransactionBufferExtendArgs)]
pub struct TransactionBufferExtend<'info> {
    // CHECK: Multisig is read-only which means validation needs to happen
    // explicitly via multisig.compressed_verify_state()
    #[account(
        seeds = [SEED_PREFIX, SEED_MULTISIG, args.compression_args.multisig_data.create_key.as_ref()],
        bump = args.compression_args.multisig_data.bump,
    )]
    pub multisig: AccountInfo<'info>,

    #[account(
        mut,
        // Only the creator can extend the buffer
        constraint = transaction_buffer.creator == creator.key() @ MultisigError::Unauthorized,
        seeds = [
            SEED_PREFIX,
            multisig.key().as_ref(),
            SEED_TRANSACTION_BUFFER,
            creator.key().as_ref(),
            &transaction_buffer.buffer_index.to_le_bytes()
        ],
        bump
    )]
    pub transaction_buffer: Account<'info, TransactionBuffer>,

    /// The member of the multisig that created the TransactionBuffer.
    pub creator: Signer<'info>,

    // Fee Payer
    #[fee_payer]
    #[account(mut)]
    pub payer: Signer<'info>,

    // Light Related Accounts
    #[authority]
    #[account(
        seeds = [CPI_AUTHORITY_PDA_SEED],
        bump,
    )]
    pub cpi_authority: AccountInfo<'info>,
    #[self_program]
    pub squads_program: Program<'info, crate::program::SquadsMultisigProgram>,
}

impl<'info> TransactionBufferExtend<'info> {
    fn validate(
        &self,
        ctx: &Context<'_, '_, 'info, 'info, TransactionBufferExtend<'info>>,
        args: &TransactionBufferExtendArgs,
    ) -> Result<()> {
        let Self {
            creator,
            transaction_buffer,
            ..
        } = self;

        let multisig = LightMultisig::from(&args.compression_args.multisig_data);

        // Validate the multisig state
        multisig.compressed_verify_state(&ctx, &args.compression_args)?;

        // creator is still a member in the multisig
        require!(
            multisig.is_member(creator.key()).is_some(),
            MultisigError::NotAMember
        );

        // creator still has initiate permissions
        require!(
            multisig.member_has_permission(creator.key(), Permission::Initiate),
            MultisigError::Unauthorized
        );

        // Extended Buffer size must not exceed final buffer size
        // Calculate remaining space in the buffer
        let current_buffer_size = transaction_buffer.buffer.len() as u16;
        let remaining_space = transaction_buffer
            .final_buffer_size
            .checked_sub(current_buffer_size)
            .unwrap();

        // Check if the new data exceeds the remaining space
        let new_data_size = args.buffer.len() as u16;
        require!(
            new_data_size <= remaining_space,
            MultisigError::FinalBufferSizeExceeded
        );

        Ok(())
    }

    /// Create a new vault transaction.
    #[access_control(ctx.accounts.validate(&ctx, &args))]
    pub fn transaction_buffer_extend(
        ctx: Context<'_, '_, 'info, 'info, TransactionBufferExtend<'info>>,
        args: TransactionBufferExtendArgs,
    ) -> Result<()> {
        // Mutable Accounts
        let transaction_buffer = &mut ctx.accounts.transaction_buffer;

        // Required Data
        let buffer_slice_extension = args.buffer;

        // Extend the Buffer inside the TransactionBuffer
        transaction_buffer
            .buffer
            .extend_from_slice(&buffer_slice_extension);

        // Invariant function on the transaction buffer
        transaction_buffer.invariant()?;

        Ok(())
    }
}
