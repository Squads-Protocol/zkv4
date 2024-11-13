use anchor_lang::prelude::*;
use light_sdk::{light_system_accounts, LightTraits, CPI_AUTHORITY_PDA_SEED};

use crate::errors::*;
use crate::state::MAX_BUFFER_SIZE;
use crate::state::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct TransactionBufferCreateArgs {
    /// Index of the buffer account to seed the account derivation
    pub buffer_index: u8,
    /// Index of the vault this transaction belongs to.
    pub vault_index: u8,
    /// Hash of the final assembled transaction message.
    pub final_buffer_hash: [u8; 32],
    /// Final size of the buffer.
    pub final_buffer_size: u16,
    /// Initial slice of the buffer.
    pub buffer: Vec<u8>,
    /// Compression args for the multisig account
    pub compression_args: MutateOrVerifyCompressedMultisigArgs,
}

#[light_system_accounts]
#[derive(Accounts, LightTraits)]
#[instruction(args: TransactionBufferCreateArgs)]
pub struct TransactionBufferCreate<'info> {
    // CHECK: Multisig is read-only which means validation needs to happen
    // explicitly via multisig.compressed_verify_state()
    #[account(
        seeds = [SEED_PREFIX, SEED_MULTISIG, args.compression_args.multisig_data.create_key.as_ref()],
        bump = args.compression_args.multisig_data.bump,
    )]
    pub multisig: AccountInfo<'info>,

    #[account(
        init,
        payer = rent_payer,
        space = TransactionBuffer::size(args.final_buffer_size)?,
        seeds = [
            SEED_PREFIX,
            multisig.key().as_ref(),
            SEED_TRANSACTION_BUFFER,
            creator.key().as_ref(),
            &args.buffer_index.to_le_bytes(),
        ],
        bump
    )]
    pub transaction_buffer: Account<'info, TransactionBuffer>,

    /// The member of the multisig that is creating the transaction.
    pub creator: Signer<'info>,

    /// The payer for the transaction account rent.
    #[fee_payer]
    #[account(mut)]
    pub rent_payer: Signer<'info>,

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

impl <'info>TransactionBufferCreate<'info> {
    fn validate(
        &self,
        ctx: &Context<'_, '_, 'info, 'info, TransactionBufferCreate<'info>>,
        args: &TransactionBufferCreateArgs,
    ) -> Result<()> {
        let Self { creator, .. } = self;

        let multisig = LightMultisig::from(&args.compression_args.multisig_data);

        // Validate the multisig state
        multisig.compressed_verify_state(ctx, &args.compression_args)?;

        // creator is a member in the multisig
        require!(
            multisig.is_member(creator.key()).is_some(),
            MultisigError::NotAMember
        );
        // creator has initiate permissions
        require!(
            multisig.member_has_permission(creator.key(), Permission::Initiate),
            MultisigError::Unauthorized
        );

        // Final Buffer Size must not exceed 4000 bytes
        require!(
            args.final_buffer_size as usize <= MAX_BUFFER_SIZE,
            MultisigError::FinalBufferSizeExceeded
        );
        Ok(())
    }

    /// Create a new vault transaction.
    #[access_control(ctx.accounts.validate(&ctx, &args))]
    pub fn transaction_buffer_create(
        ctx: Context<'_, '_, 'info, 'info, TransactionBufferCreate<'info>>,
        args: TransactionBufferCreateArgs,
    ) -> Result<()> {
        // Mutable Accounts
        let transaction_buffer = &mut ctx.accounts.transaction_buffer;

        // Readonly Accounts
        let multisig_account_info = &ctx.accounts.multisig;
        let creator = &mut ctx.accounts.creator;

        // Get the buffer index.
        let buffer_index = args.buffer_index;

        // Initialize the transaction fields.
        transaction_buffer.multisig = multisig_account_info.key();
        transaction_buffer.creator = creator.key();
        transaction_buffer.vault_index = args.vault_index;
        transaction_buffer.buffer_index = buffer_index;
        transaction_buffer.final_buffer_hash = args.final_buffer_hash;
        transaction_buffer.final_buffer_size = args.final_buffer_size;
        transaction_buffer.buffer = args.buffer;

        // Invariant function on the transaction buffer
        transaction_buffer.invariant()?;

        Ok(())
    }
}
