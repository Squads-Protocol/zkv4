use anchor_lang::prelude::*;
use light_sdk::{light_system_accounts, LightTraits, CPI_AUTHORITY_PDA_SEED};

use crate::errors::*;
use crate::state::*;
use crate::utils::get_cpi_authority_seeds;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct BatchCreateArgs {
    pub compression_args: MutateOrVerifyCompressedMultisigArgs,
    /// Index of the vault this transaction belongs to.
    pub vault_index: u8,
    pub memo: Option<String>,
}

#[light_system_accounts]
#[derive(Accounts, LightTraits)]
#[instruction(args: BatchCreateArgs)]
pub struct BatchCreate<'info> {
    // CHECK: Multisig is mutable which means validation will happen
    // implicitly via multisig.compressed_mutate()
    #[account(
        mut,
        seeds = [SEED_PREFIX, SEED_MULTISIG, args.compression_args.multisig_data.create_key.as_ref()],
        bump = args.compression_args.multisig_data.bump,
    )]
    pub multisig: AccountInfo<'info>,

    #[account(
        init,
        payer = rent_payer,
        space = 8 + Batch::INIT_SPACE,
        seeds = [
            SEED_PREFIX,
            multisig.key().as_ref(),
            SEED_TRANSACTION,
            &args.compression_args.multisig_data.transaction_index.checked_add(1).unwrap().to_le_bytes(),
        ],
        bump
    )]
    pub batch: Account<'info, Batch>,

    /// The member of the multisig that is creating the batch.
    pub creator: Signer<'info>,

    /// The payer for the batch account rent.
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

impl<'info> BatchCreate<'info> {
    fn validate(&self, args: &BatchCreateArgs) -> Result<()> {
        let Self { creator, .. } = self;

        let multisig = LightMultisig::from(&args.compression_args.multisig_data);
        // creator
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

    /// Create a new batch.
    #[access_control(ctx.accounts.validate(&args))]
    pub fn batch_create(
        ctx: Context<'_, '_, 'info, 'info, Self>,
        args: BatchCreateArgs,
    ) -> Result<()> {
        let mut multisig = LightMultisig::from(&args.compression_args.multisig_data);
        let multisig_account_info = &mut ctx.accounts.multisig;
        let creator = &mut ctx.accounts.creator;
        let batch = &mut ctx.accounts.batch;

        let multisig_key = multisig_account_info.key();

        // Increment the transaction index.
        let index = multisig.transaction_index.checked_add(1).expect("overflow");

        let vault_seeds = &[
            SEED_PREFIX,
            multisig_key.as_ref(),
            SEED_VAULT,
            &args.vault_index.to_le_bytes(),
        ];
        let (_, vault_bump) = Pubkey::find_program_address(vault_seeds, ctx.program_id);

        batch.multisig = multisig_key;
        batch.creator = creator.key();
        batch.index = index;
        batch.bump = ctx.bumps.batch;
        batch.vault_index = args.vault_index;
        batch.vault_bump = vault_bump;
        batch.size = 0;
        batch.executed_transaction_index = 0;

        batch.invariant()?;

        // Updated last transaction index in the multisig account.
        multisig.transaction_index = index;

        let cpi_authority_bump = ctx.bumps.cpi_authority;
        let cpi_authority_seeds = get_cpi_authority_seeds(&cpi_authority_bump);

        // Mutate the compressed multisig account.
        multisig.compressed_mutate(ctx, args.compression_args, &[&cpi_authority_seeds])?;
        multisig.invariant()?;

        // Logs for indexing.
        msg!("batch index: {}", index);

        Ok(())
    }
}
