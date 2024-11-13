use anchor_lang::prelude::*;
use light_sdk::{light_system_accounts, LightTraits, CPI_AUTHORITY_PDA_SEED};

use crate::errors::*;
use crate::state::*;
use crate::utils::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct BatchExecuteTransactionArgs {
    pub compression_args: MutateOrVerifyCompressedMultisigArgs,
}

#[light_system_accounts]
#[derive(Accounts, LightTraits)]
#[instruction(args: BatchExecuteTransactionArgs)]
pub struct BatchExecuteTransaction<'info> {
    /// Multisig account this batch belongs to.
    // CHECK: Multisig is read-only which means validation needs to happen
    // explicitly via multisig.compressed_verify_state()
    #[account(
        seeds = [SEED_PREFIX, SEED_MULTISIG, args.compression_args.multisig_data.create_key.as_ref()],
        bump = args.compression_args.multisig_data.bump,
    )]
    pub multisig: AccountInfo<'info>,

    /// Member of the multisig.
    pub member: Signer<'info>,

    /// The proposal account associated with the batch.
    /// If `transaction` is the last in the batch, the `proposal` status will be set to `Executed`.
    #[account(
        mut,
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

    /// Batch transaction to execute.
    #[account(
        seeds = [
            SEED_PREFIX,
            multisig.key().as_ref(),
            SEED_TRANSACTION,
            &batch.index.to_le_bytes(),
            SEED_BATCH_TRANSACTION,
            &batch.executed_transaction_index.checked_add(1).unwrap().to_le_bytes(),
        ],
        bump = transaction.bump,
    )]
    pub transaction: Account<'info, VaultBatchTransaction>,

    // Light Related Accounts
    #[fee_payer]
    #[account(mut)]
    pub payer: Signer<'info>,

    #[authority]
    #[account(
        seeds = [CPI_AUTHORITY_PDA_SEED],
        bump,
    )]
    pub cpi_authority: AccountInfo<'info>,
    #[self_program]
    pub squads_program: Program<'info, crate::program::SquadsMultisigProgram>,
    //
    // `remaining_accounts` must include the following accounts in the exact order:
    // 1. AddressLookupTable accounts in the order they appear in `message.address_table_lookups`.
    // 2. Accounts in the order they appear in `message.account_keys`.
    // 3. Accounts in the order they appear in `message.address_table_lookups`.
}

impl<'info> BatchExecuteTransaction<'info> {
    fn validate(
        &self,
        ctx: &Context<'_, '_, 'info, 'info, Self>,
        args: &BatchExecuteTransactionArgs,
    ) -> Result<()> {
        let Self {
            member, proposal, ..
        } = self;

        let multisig = LightMultisig::from(&args.compression_args.multisig_data);

        // Validate the multisig state
        multisig.compressed_verify_state(&ctx, &args.compression_args)?;

        // `member`
        require!(
            multisig.is_member(member.key()).is_some(),
            MultisigError::NotAMember
        );
        require!(
            multisig.member_has_permission(member.key(), Permission::Execute),
            MultisigError::Unauthorized
        );

        // `proposal`
        match proposal.status {
            ProposalStatus::Approved { timestamp } => {
                require!(
                    Clock::get()?.unix_timestamp - timestamp >= i64::from(multisig.time_lock),
                    MultisigError::TimeLockNotReleased
                );
            }
            _ => return err!(MultisigError::InvalidProposalStatus),
        };
        // Stale batch transaction proposals CAN be executed if they were approved
        // before becoming stale, hence no check for staleness here.

        // `batch` is validated by its seeds.

        // `transaction` is validated by its seeds.

        Ok(())
    }

    /// Execute a transaction from the batch.
    #[access_control(ctx.accounts.validate(&ctx, &args))]
    pub fn batch_execute_transaction(
        ctx: Context<'_, '_, 'info, 'info, Self>,
        args: BatchExecuteTransactionArgs,
    ) -> Result<()> {
        let multisig = LightMultisig::from(&args.compression_args.multisig_data);
        let multisig_account_info = &ctx.accounts.multisig;
        let proposal = &mut ctx.accounts.proposal;
        let batch = &mut ctx.accounts.batch;

        // NOTE: After `take()` is called, the VaultTransaction is reduced to
        // its default empty value, which means it should no longer be referenced or
        // used after this point to avoid faulty behavior.
        // Instead only make use of the returned `transaction` value.
        let transaction = ctx.accounts.transaction.take();

        let multisig_key = multisig_account_info.key();
        let batch_key = batch.key();

        let vault_seeds = &[
            SEED_PREFIX,
            multisig_key.as_ref(),
            SEED_VAULT,
            &batch.vault_index.to_le_bytes(),
            &[batch.vault_bump],
        ];

        let transaction_message = transaction.message;
        let num_lookups = transaction_message.address_table_lookups.len();

        let message_account_infos = ctx
            .remaining_accounts
            .get(num_lookups..)
            .ok_or(MultisigError::InvalidNumberOfAccounts)?;
        let address_lookup_table_account_infos = ctx
            .remaining_accounts
            .get(..num_lookups)
            .ok_or(MultisigError::InvalidNumberOfAccounts)?;

        let vault_pubkey = Pubkey::create_program_address(vault_seeds, ctx.program_id).unwrap();

        let (ephemeral_signer_keys, ephemeral_signer_seeds) =
            derive_ephemeral_signers(batch_key, &transaction.ephemeral_signer_bumps);

        let executable_message = ExecutableTransactionMessage::new_validated(
            transaction_message,
            message_account_infos,
            address_lookup_table_account_infos,
            &vault_pubkey,
            &ephemeral_signer_keys,
        )?;

        let protected_accounts = &[proposal.key(), batch_key];

        // Execute the transaction message instructions one-by-one.
        // NOTE: `execute_message()` calls `self.to_instructions_and_accounts()`
        // which in turn calls `take()` on
        // `self.message.instructions`, therefore after this point no more
        // references or usages of `self.message` should be made to avoid
        // faulty behavior.
        executable_message.execute_message(
            vault_seeds,
            &ephemeral_signer_seeds,
            protected_accounts,
        )?;

        // Increment the executed transaction index.
        batch.executed_transaction_index = batch
            .executed_transaction_index
            .checked_add(1)
            .expect("overflow");

        // If this is the last transaction in the batch, set the proposal status to `Executed`.
        if batch.executed_transaction_index == batch.size {
            proposal.status = ProposalStatus::Executed {
                timestamp: Clock::get()?.unix_timestamp,
            };
        }

        batch.invariant()?;

        Ok(())
    }
}
