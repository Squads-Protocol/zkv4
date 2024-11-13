use anchor_lang::prelude::*;
use light_sdk::{light_system_accounts, LightTraits, CPI_AUTHORITY_PDA_SEED};

use crate::errors::*;
use crate::state::*;
use crate::TransactionMessage;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct BatchAddTransactionArgs {
    pub compression_args: MutateOrVerifyCompressedMultisigArgs,
    /// Number of ephemeral signing PDAs required by the transaction.
    pub ephemeral_signers: u8,
    pub transaction_message: Vec<u8>,
}

#[light_system_accounts]
#[derive(Accounts, LightTraits)]
#[instruction(args: BatchAddTransactionArgs)]
pub struct BatchAddTransaction<'info> {
    /// Multisig account this batch belongs to.
    // CHECK: Multisig is read-only which means validation needs to happen
    // explicitly via multisig.compressed_verify_state()
    #[account(
        seeds = [SEED_PREFIX, SEED_MULTISIG, args.compression_args.multisig_data.create_key.as_ref()],
        bump = args.compression_args.multisig_data.bump,
    )]
    pub multisig: AccountInfo<'info>,

    /// The proposal account associated with the batch.
    #[account(
        seeds = [
            SEED_PREFIX,
            multisig.key().as_ref(),
            SEED_TRANSACTION,
            &batch.index.to_le_bytes(),
            SEED_PROPOSAL,
        ],
        bump = proposal.bump,
    )]
    pub proposal: Account<'info, Proposal>,

    #[account(
        mut,
        seeds = [
            SEED_PREFIX,
            multisig.key().as_ref(),
            SEED_TRANSACTION,
            &batch.index.to_le_bytes(),
        ],
        bump = batch.bump,
    )]
    pub batch: Account<'info, Batch>,

    /// `VaultBatchTransaction` account to initialize and add to the `batch`.
    #[account(
        init,
        payer = rent_payer,
        space = VaultBatchTransaction::size(args.ephemeral_signers, &args.transaction_message)?,
        seeds = [
            SEED_PREFIX,
            multisig.key().as_ref(),
            SEED_TRANSACTION,
            &batch.index.to_le_bytes(),
            SEED_BATCH_TRANSACTION,
            &batch.size.checked_add(1).unwrap().to_le_bytes(),
        ],
        bump
    )]
    pub transaction: Account<'info, VaultBatchTransaction>,

    /// Member of the multisig.
    pub member: Signer<'info>,

    /// The payer for the batch transaction account rent.
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

impl<'info> BatchAddTransaction<'info> {
    fn validate(
        &self,
        ctx: &Context<'_, '_, 'info, 'info, Self>,
        args: &BatchAddTransactionArgs,
    ) -> Result<()> {
        let Self {
            member,
            proposal,
            batch,
            ..
        } = self;

        let multisig = LightMultisig::from(&args.compression_args.multisig_data);

        // Validate the multisig state
        multisig.compressed_verify_state(ctx, &args.compression_args)?;

        // `member`
        require!(
            multisig.is_member(member.key()).is_some(),
            MultisigError::NotAMember
        );
        require!(
            multisig.member_has_permission(member.key(), Permission::Initiate),
            MultisigError::Unauthorized
        );
        // Only batch creator can add transactions to it.
        require!(member.key() == batch.creator, MultisigError::Unauthorized);

        // `proposal`
        require!(
            matches!(proposal.status, ProposalStatus::Draft { .. }),
            MultisigError::InvalidProposalStatus
        );

        // `batch` is validated by its seeds.

        Ok(())
    }

    /// Add a transaction to the batch.
    #[access_control(ctx.accounts.validate(&ctx, &args))]
    pub fn batch_add_transaction(
        ctx: Context<'_, '_, 'info, 'info, Self>,
        args: BatchAddTransactionArgs,
    ) -> Result<()> {
        let batch = &mut ctx.accounts.batch;
        let transaction = &mut ctx.accounts.transaction;

        let batch_key = batch.key();

        let transaction_message =
            TransactionMessage::deserialize(&mut args.transaction_message.as_slice())?;

        let ephemeral_signer_bumps: Vec<u8> = (0..args.ephemeral_signers)
            .map(|ephemeral_signer_index| {
                let ephemeral_signer_seeds = &[
                    SEED_PREFIX,
                    batch_key.as_ref(),
                    SEED_EPHEMERAL_SIGNER,
                    &ephemeral_signer_index.to_le_bytes(),
                ];

                let (_, bump) =
                    Pubkey::find_program_address(ephemeral_signer_seeds, ctx.program_id);

                bump
            })
            .collect();

        transaction.bump = ctx.bumps.transaction;
        transaction.ephemeral_signer_bumps = ephemeral_signer_bumps;
        transaction.message = transaction_message.try_into()?;

        // Increment the batch size.
        batch.size = batch.size.checked_add(1).expect("overflow");

        // Logs for indexing.
        msg!("batch index: {}", batch.index);
        msg!("batch size: {}", batch.size);

        Ok(())
    }
}
