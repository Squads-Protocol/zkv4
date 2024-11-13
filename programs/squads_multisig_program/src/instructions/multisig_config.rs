use anchor_lang::prelude::*;
use light_sdk::{light_system_accounts, LightTraits, CPI_AUTHORITY_PDA_SEED};

use crate::errors::*;
use crate::state::*;
use crate::utils::get_cpi_authority_seeds;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct MultisigAddMemberArgs {
    pub new_member: Member,
    /// Memo is used for indexing only.
    pub memo: Option<String>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct MultisigRemoveMemberArgs {
    pub old_member: Pubkey,
    /// Memo is used for indexing only.
    pub memo: Option<String>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct MultisigChangeThresholdArgs {
    pub new_threshold: u16,
    /// Memo is used for indexing only.
    pub memo: Option<String>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct MultisigSetTimeLockArgs {
    pub time_lock: u32,
    /// Memo is used for indexing only.
    pub memo: Option<String>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct MultisigSetConfigAuthorityArgs {
    pub config_authority: Pubkey,
    /// Memo is used for indexing only.
    pub memo: Option<String>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct MultisigSetRentCollectorArgs {
    pub rent_collector: Option<Pubkey>,
    /// Memo is used for indexing only.
    pub memo: Option<String>,
}

#[light_system_accounts]
#[derive(Accounts, LightTraits)]
#[instruction(compression_args: MutateOrVerifyCompressedMultisigArgs)]
pub struct MultisigConfig<'info> {
    /// The multisig is mutable in this case, so the validation is done
    /// implicitly when calling multisig.compressed_mutate()
    #[account(
        seeds = [SEED_PREFIX, SEED_MULTISIG, &compression_args.multisig_data.create_key.as_ref()],
        bump = compression_args.multisig_data.bump,
    )]
    pub multisig: AccountInfo<'info>,

    /// Multisig `config_authority` that must authorize the configuration change.
    pub config_authority: Signer<'info>,

    /// The account that will be charged or credited in case the multisig account needs to reallocate space,
    /// for example when adding a new member or a spending limit.
    /// This is usually the same as `config_authority`, but can be a different account if needed.
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

impl<'info> MultisigConfig<'info> {
    fn validate(&self, args: &MutateOrVerifyCompressedMultisigArgs) -> Result<()> {
        let multisig = LightMultisig::from(&args.multisig_data);

        require_keys_eq!(
            self.config_authority.key(),
            multisig.config_authority,
            MultisigError::Unauthorized
        );

        Ok(())
    }

    /// Add a member/key to the multisig and reallocate space if necessary.
    ///
    /// NOTE: This instruction must be called only by the `config_authority` if one is set (Controlled Multisig).
    ///       Uncontrolled Mustisigs should use `config_transaction_create` instead.
    #[access_control(ctx.accounts.validate(&compression_args))]
    pub fn multisig_add_member(
        ctx: Context<'_, '_, 'info, 'info, Self>,
        args: MultisigAddMemberArgs,
        compression_args: MutateOrVerifyCompressedMultisigArgs,
    ) -> Result<()> {
        let MultisigAddMemberArgs { new_member, .. } = args;
        let mut multisig = LightMultisig::from(&compression_args.multisig_data);

        // Make sure that the new member is not already in the multisig.
        require!(
            multisig.is_member(new_member.key).is_none(),
            MultisigError::DuplicateMember
        );

        multisig.add_member(new_member);

        // Make sure the multisig account can fit the newly set rent_collector.
        Multisig::realloc_if_needed(
            ctx.accounts.multisig.to_account_info(),
            multisig.members.len(),
            Some(ctx.accounts
                .rent_payer.to_account_info()),
            Some(ctx.accounts.system_program.to_account_info()),
        )?;

        multisig.invalidate_prior_transactions();
        multisig.invariant()?;

        // Mutate the compressed multisig account
        let cpi_authority_bump: u8 = ctx.bumps.cpi_authority;
        let cpi_authority_signer_seeds = get_cpi_authority_seeds(&cpi_authority_bump);
        multisig.compressed_mutate(ctx, compression_args, &[&cpi_authority_signer_seeds])?;

        Ok(())
    }

    /// Remove a member/key from the multisig.
    ///
    /// NOTE: This instruction must be called only by the `config_authority` if one is set (Controlled Multisig).
    ///       Uncontrolled Mustisigs should use `config_transaction_create` instead.
    #[access_control(ctx.accounts.validate(&compression_args))]
    pub fn multisig_remove_member(
        ctx: Context<'_, '_, 'info, 'info, Self>,
        args: MultisigRemoveMemberArgs,
        compression_args: MutateOrVerifyCompressedMultisigArgs,
    ) -> Result<()> {
        let MultisigRemoveMemberArgs { old_member, .. } = args;
        let mut multisig = LightMultisig::from(&compression_args.multisig_data);

        require!(multisig.members.len() > 1, MultisigError::RemoveLastMember);

        multisig.remove_member(old_member)?;
        multisig.invalidate_prior_transactions();
        multisig.invariant()?;

        // Mutate the compressed multisig account
        let cpi_authority_bump: u8 = ctx.bumps.cpi_authority;
        let cpi_authority_signer_seeds = get_cpi_authority_seeds(&cpi_authority_bump);
        multisig.compressed_mutate(ctx, compression_args, &[&cpi_authority_signer_seeds])?;

        Ok(())
    }

    /// NOTE: This instruction must be called only by the `config_authority` if one is set (Controlled Multisig).
    ///       Uncontrolled Mustisigs should use `config_transaction_create` instead.
    #[access_control(ctx.accounts.validate(&compression_args))]
    pub fn multisig_change_threshold(
        ctx: Context<'_, '_, 'info, 'info, Self>,
        args: MultisigChangeThresholdArgs,
        compression_args: MutateOrVerifyCompressedMultisigArgs,
    ) -> Result<()> {
        let MultisigChangeThresholdArgs { new_threshold, .. } = args;
        let mut multisig = LightMultisig::from(&compression_args.multisig_data);

        multisig.threshold = new_threshold;
        multisig.invalidate_prior_transactions();
        multisig.invariant()?;

        // Mutate the compressed multisig account
        let cpi_authority_bump: u8 = ctx.bumps.cpi_authority;
        let cpi_authority_signer_seeds = get_cpi_authority_seeds(&cpi_authority_bump);
        multisig.compressed_mutate(ctx, compression_args, &[&cpi_authority_signer_seeds])?;

        Ok(())
    }

    /// Set the `time_lock` config parameter for the multisig.
    ///
    /// NOTE: This instruction must be called only by the `config_authority` if one is set (Controlled Multisig).
    ///       Uncontrolled Mustisigs should use `config_transaction_create` instead.
    #[access_control(ctx.accounts.validate(&compression_args))]
    pub fn multisig_set_time_lock(
        ctx: Context<'_, '_, 'info, 'info, Self>,
        args: MultisigSetTimeLockArgs,
        compression_args: MutateOrVerifyCompressedMultisigArgs,
    ) -> Result<()> {
        let MultisigSetTimeLockArgs { time_lock, .. } = args;
        let mut multisig = LightMultisig::from(&compression_args.multisig_data);

        multisig.time_lock = time_lock;
        multisig.invalidate_prior_transactions();
        multisig.invariant()?;

        // Mutate the compressed multisig account
        let cpi_authority_bump: u8 = ctx.bumps.cpi_authority;
        let cpi_authority_signer_seeds = get_cpi_authority_seeds(&cpi_authority_bump);
        multisig.compressed_mutate(ctx, compression_args, &[&cpi_authority_signer_seeds])?;

        Ok(())
    }

    /// Set the multisig `config_authority`.
    ///
    /// NOTE: This instruction must be called only by the `config_authority` if one is set (Controlled Multisig).
    ///       Uncontrolled Mustisigs should use `config_transaction_create` instead.
    #[access_control(ctx.accounts.validate(&compression_args))]
    pub fn multisig_set_config_authority(
        ctx: Context<'_, '_, 'info, 'info, Self>,
        args: MultisigSetConfigAuthorityArgs,
        compression_args: MutateOrVerifyCompressedMultisigArgs,
    ) -> Result<()> {
        let MultisigSetConfigAuthorityArgs { config_authority, .. } = args;
        let mut multisig = LightMultisig::from(&compression_args.multisig_data);

        multisig.config_authority = config_authority;
        multisig.invalidate_prior_transactions();
        multisig.invariant()?;

        // Mutate the compressed multisig account
        let cpi_authority_bump: u8 = ctx.bumps.cpi_authority;
        let cpi_authority_signer_seeds = get_cpi_authority_seeds(&cpi_authority_bump);
        multisig.compressed_mutate(ctx, compression_args, &[&cpi_authority_signer_seeds])?;

        Ok(())
    }

    /// Set the multisig `rent_collector` and reallocate space if necessary.
    ///
    /// NOTE: This instruction must be called only by the `config_authority` if one is set (Controlled Multisig).
    ///       Uncontrolled Mustisigs should use `config_transaction_create` instead.
    #[access_control(ctx.accounts.validate(&compression_args))]
    pub fn multisig_set_rent_collector(
        ctx: Context<'_, '_, 'info, 'info, Self>,
        args: MultisigSetRentCollectorArgs,
        compression_args: MutateOrVerifyCompressedMultisigArgs,
    ) -> Result<()> {
        let MultisigSetRentCollectorArgs { rent_collector, .. } = args;
        let mut multisig = LightMultisig::from(&compression_args.multisig_data);

        multisig.rent_collector = OptionPubkey( rent_collector );

        // Make sure the multisig account can fit the newly set rent_collector.
        Multisig::realloc_if_needed(
            ctx.accounts.multisig.to_account_info(),
            multisig.members.len(),
            Some(ctx.accounts.rent_payer.to_account_info()),
            Some(ctx.accounts.system_program.to_account_info()),
        )?;

        // We don't need to invalidate prior transactions here because changing
        // `rent_collector` doesn't affect the consensus parameters of the multisig.

        multisig.invariant()?;

        // Mutate the compressed multisig account
        let cpi_authority_bump: u8 = ctx.bumps.cpi_authority;
        let cpi_authority_signer_seeds = get_cpi_authority_seeds(&cpi_authority_bump);
        multisig.compressed_mutate(ctx, compression_args, &[&cpi_authority_signer_seeds])?;

        Ok(())
    }
}
