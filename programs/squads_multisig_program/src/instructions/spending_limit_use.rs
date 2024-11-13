use anchor_lang::prelude::*;
use anchor_spl::token_2022::TransferChecked;
use anchor_spl::token_interface;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};
use light_sdk::{light_system_accounts, LightTraits, CPI_AUTHORITY_PDA_SEED};

use crate::errors::*;
use crate::state::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct SpendingLimitUseArgs {
    /// Amount of tokens to transfer.
    pub amount: u64,
    /// Decimals of the token mint. Used for double-checking against incorrect order of magnitude of `amount`.
    pub decimals: u8,
    /// Memo used for indexing.
    pub memo: Option<String>,
    /// Compression args for the multisig account
    pub compression_args: MutateOrVerifyCompressedMultisigArgs,
}

#[light_system_accounts]
#[derive(Accounts, LightTraits)]
#[instruction(args: SpendingLimitUseArgs)]
pub struct SpendingLimitUse<'info> {
    /// The multisig account the `spending_limit` is for.
    // CHECK: Multisig is read-only which means validation needs to happen
    // explicitly via multisig.compressed_verify_state()
    #[account(
        seeds = [SEED_PREFIX, SEED_MULTISIG, args.compression_args.multisig_data.create_key.as_ref()],
        bump = args.compression_args.multisig_data.bump,
    )]
    pub multisig: AccountInfo<'info>,

    pub member: Signer<'info>,

    #[fee_payer]
    pub payer: Signer<'info>,

    /// The SpendingLimit account to use.
    #[account(
        mut,
        seeds = [
            SEED_PREFIX,
            multisig.key().as_ref(),
            SEED_SPENDING_LIMIT,
            spending_limit.create_key.key().as_ref(),
        ],
        bump = spending_limit.bump,
    )]
    pub spending_limit: Account<'info, SpendingLimit>,

    /// Multisig vault account to transfer tokens from.
    /// CHECK: All the required checks are done by checking the seeds.
    #[account(
        mut,
        seeds = [
            SEED_PREFIX,
            multisig.key().as_ref(),
            SEED_VAULT,
            &spending_limit.vault_index.to_le_bytes(),
        ],
        bump
    )]
    pub vault: AccountInfo<'info>,

    /// Destination account to transfer tokens to.
    /// CHECK: We do the checks in `SpendingLimitUse::validate`.
    #[account(mut)]
    pub destination: AccountInfo<'info>,

    /// The mint of the tokens to transfer in case `spending_limit.mint` is an SPL token.
    /// CHECK: We do the checks in `SpendingLimitUse::validate`.
    pub mint: Option<InterfaceAccount<'info, Mint>>,

    /// Multisig vault token account to transfer tokens from in case `spending_limit.mint` is an SPL token.
    #[account(
        mut,
        token::mint = mint,
        token::authority = vault,
    )]
    pub vault_token_account: Option<InterfaceAccount<'info, TokenAccount>>,

    /// Destination token account in case `spending_limit.mint` is an SPL token.
    #[account(
        mut,
        token::mint = mint,
        token::authority = destination,
    )]
    pub destination_token_account: Option<InterfaceAccount<'info, TokenAccount>>,

    /// In case `spending_limit.mint` is an SPL token.
    pub token_program: Option<Interface<'info, TokenInterface>>,

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

impl<'info> SpendingLimitUse<'info> {
    fn validate(
        &self,
        ctx: &Context<'_, '_, 'info, 'info, SpendingLimitUse<'info>>,
        args: &SpendingLimitUseArgs,
    ) -> Result<()> {
        let Self {
            member,
            spending_limit,
            mint,
            ..
        } = self;

        let multisig = LightMultisig::from(&args.compression_args.multisig_data);

        // Validate the multisig state
        multisig.compressed_verify_state(ctx, &args.compression_args)?;

        // member
        require!(
            spending_limit.members.contains(&member.key()),
            MultisigError::Unauthorized
        );

        // spending_limit - needs no checking.

        // mint
        if spending_limit.mint == Pubkey::default() {
            // SpendingLimit is for SOL, there should be no mint account in this case.
            require!(mint.is_none(), MultisigError::InvalidMint);
        } else {
            // SpendingLimit is for an SPL token, `mint` must match `spending_limit.mint`.
            require!(
                spending_limit.mint == mint.as_ref().unwrap().key(),
                MultisigError::InvalidMint
            );
        }

        // vault - checked in the #[account] attribute.

        // vault_token_account - checked in the #[account] attribute.

        // destination
        if !spending_limit.destinations.is_empty() {
            require!(
                spending_limit
                    .destinations
                    .contains(&self.destination.key()),
                MultisigError::InvalidDestination
            );
        }

        // destination_token_account - checked in the #[account] attribute.

        Ok(())
    }

    /// Use a spending limit to transfer tokens from a multisig vault to a destination account.
    #[access_control(ctx.accounts.validate(&ctx, &args))]
    pub fn spending_limit_use(
        ctx: Context<'_, '_, 'info, 'info, SpendingLimitUse<'info>>,
        args: SpendingLimitUseArgs,
    ) -> Result<()> {
        let spending_limit = &mut ctx.accounts.spending_limit;
        let vault = &mut ctx.accounts.vault;
        let destination = &mut ctx.accounts.destination;

        let multisig_key = ctx.accounts.multisig.key();
        let vault_bump = ctx.bumps.vault;
        let now = Clock::get()?.unix_timestamp;

        // Reset `spending_limit.remaining_amount` if the `spending_limit.period` has passed.
        if let Some(reset_period) = spending_limit.period.to_seconds() {
            let passed_since_last_reset = now.checked_sub(spending_limit.last_reset).unwrap();

            if passed_since_last_reset > reset_period {
                spending_limit.remaining_amount = spending_limit.amount;

                let periods_passed = passed_since_last_reset.checked_div(reset_period).unwrap();

                // last_reset = last_reset + periods_passed * reset_period,
                spending_limit.last_reset = spending_limit
                    .last_reset
                    .checked_add(periods_passed.checked_mul(reset_period).unwrap())
                    .unwrap();
            }
        }

        // Update `spending_limit.remaining_amount`.
        // This will also check if `amount` doesn't exceed `spending_limit.remaining_amount`.
        spending_limit.remaining_amount = spending_limit
            .remaining_amount
            .checked_sub(args.amount)
            .ok_or(MultisigError::SpendingLimitExceeded)?;

        // Transfer tokens.
        if spending_limit.mint == Pubkey::default() {
            // Transfer using the system_program::transfer.
            let system_program = &ctx.accounts.system_program;

            // Sanity check for the decimals. Similar to the one in token_interface::transfer_checked.
            require!(args.decimals == 9, MultisigError::DecimalsMismatch);

            anchor_lang::system_program::transfer(
                CpiContext::new_with_signer(
                    system_program.to_account_info(),
                    anchor_lang::system_program::Transfer {
                        from: vault.clone(),
                        to: destination.clone(),
                    },
                    &[&[
                        SEED_PREFIX,
                        multisig_key.as_ref(),
                        SEED_VAULT,
                        &spending_limit.vault_index.to_le_bytes(),
                        &[vault_bump],
                    ]],
                ),
                args.amount,
            )?
        } else {
            // Transfer using the token_program::transfer_checked.
            let mint = &ctx
                .accounts
                .mint
                .as_ref()
                .ok_or(MultisigError::MissingAccount)?;
            let vault_token_account = &ctx
                .accounts
                .vault_token_account
                .as_ref()
                .ok_or(MultisigError::MissingAccount)?;
            let destination_token_account = &ctx
                .accounts
                .destination_token_account
                .as_ref()
                .ok_or(MultisigError::MissingAccount)?;
            let token_program = &ctx
                .accounts
                .token_program
                .as_ref()
                .ok_or(MultisigError::MissingAccount)?;

            msg!(
                "token_program {} mint {} vault {} destination {} amount {} decimals {}",
                &token_program.key,
                &mint.key(),
                &vault.key,
                &destination.key,
                &args.amount,
                &args.decimals
            );

            token_interface::transfer_checked(
                CpiContext::new_with_signer(
                    token_program.to_account_info(),
                    TransferChecked {
                        from: vault_token_account.to_account_info(),
                        mint: mint.to_account_info(),
                        to: destination_token_account.to_account_info(),
                        authority: vault.clone(),
                    },
                    &[&[
                        SEED_PREFIX,
                        multisig_key.as_ref(),
                        SEED_VAULT,
                        &spending_limit.vault_index.to_le_bytes(),
                        &[vault_bump],
                    ]],
                ),
                args.amount,
                args.decimals,
            )?;
        }

        Ok(())
    }
}
