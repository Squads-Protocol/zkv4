use anchor_lang::prelude::*;
use light_sdk::{light_system_accounts, LightTraits, CPI_AUTHORITY_PDA_SEED};

use crate::errors::*;
use crate::state::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct MultisigRemoveSpendingLimitArgs {
    /// Memo is used for indexing only.
    pub memo: Option<String>,
    /// Compression related arguments.
    pub compression_args: MutateOrVerifyCompressedMultisigArgs,
}

#[light_system_accounts]
#[derive(Accounts, LightTraits)]
#[instruction(args: MultisigRemoveSpendingLimitArgs)]
pub struct MultisigRemoveSpendingLimit<'info> {
    // CHECK: Multisig is read-only which means validation needs to happen
    // explicitly via multisig.compressed_verify_state()
    #[account(
        seeds = [SEED_PREFIX, SEED_MULTISIG, args.compression_args.multisig_data.create_key.as_ref()],
        bump = args.compression_args.multisig_data.bump,
    )]
    pub multisig: AccountInfo<'info>,

    /// Multisig `config_authority` that must authorize the configuration change.
    pub config_authority: Signer<'info>,

    #[account(mut, close = rent_collector)]
    pub spending_limit: Account<'info, SpendingLimit>,

    /// This is usually the same as `config_authority`, but can be a different account if needed.
    /// CHECK: can be any account.
    #[account(mut)]
    pub rent_collector: AccountInfo<'info>,

    // Light Related Accounts
    #[fee_payer]
    #[account(mut)]
    pub fee_payer: Signer<'info>,
    #[authority]
    #[account(
        seeds = [CPI_AUTHORITY_PDA_SEED],
        bump,
    )]
    pub cpi_authority: AccountInfo<'info>,
    #[self_program]
    pub squads_program: Program<'info, crate::program::SquadsMultisigProgram>,
}

impl<'info> MultisigRemoveSpendingLimit<'info> {
    fn validate(
        &self,
        ctx: &Context<'_, '_, 'info, 'info, Self>,
        args: &MultisigRemoveSpendingLimitArgs,
    ) -> Result<()> {
        let multisig = LightMultisig::from(&args.compression_args.multisig_data);

        // Validate the multisig state
        multisig.compressed_verify_state(ctx, &args.compression_args)?;

        // config_authority
        require_keys_eq!(
            self.config_authority.key(),
            multisig.config_authority,
            MultisigError::Unauthorized
        );

        // `spending_limit`
        require_keys_eq!(
            self.spending_limit.multisig,
            self.multisig.key(),
            MultisigError::InvalidAccount
        );

        Ok(())
    }

    /// Remove the spending limit from the controlled multisig.
    /// NOTE: This instruction must be called only by the `config_authority` if one is set (Controlled Multisig).
    ///       Uncontrolled Mustisigs should use `config_transaction_create` instead.
    #[access_control(ctx.accounts.validate(&ctx, &args))]
    pub fn multisig_remove_spending_limit(
        ctx: Context<'_, '_, 'info, 'info, Self>,
        args: MultisigRemoveSpendingLimitArgs,
    ) -> Result<()> {
        Ok(())
    }
}
