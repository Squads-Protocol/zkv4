#![allow(deprecated)]
use anchor_lang::prelude::*;
use anchor_lang::system_program;
use solana_program::native_token::LAMPORTS_PER_SOL;

use crate::errors::MultisigError;
use crate::state::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct MultisigCreateArgs {
    /// The authority that can configure the multisig: add/remove members, change the threshold, etc.
    /// Should be set to `None` for autonomous multisigs.
    pub config_authority: Option<Pubkey>,
    /// The number of signatures required to execute a transaction.
    pub threshold: u16,
    /// The members of the multisig.
    pub members: Vec<Member>,
    /// How many seconds must pass between transaction voting, settlement, and execution.
    pub time_lock: u32,
    /// The address where the rent for the accounts related to executed, rejected, or cancelled
    /// transactions can be reclaimed. If set to `None`, the rent reclamation feature is turned off.
    pub rent_collector: Option<Pubkey>,
    /// Memo is used for indexing only.
    pub memo: Option<String>,
}

#[deprecated(
    since = "0.4.0",
    note = "This instruction is deprecated and will be removed soon. Please use `multisig_create_v2` to ensure future compatibility."
)]
#[derive(Accounts)]
#[instruction(args: MultisigCreateArgs)]
pub struct MultisigCreate<'info> {
    #[account(
        init,
        payer = creator,
        space = Multisig::size(args.members.len(), args.rent_collector.is_some()),
        seeds = [SEED_PREFIX, SEED_MULTISIG, create_key.key().as_ref()],
        bump
    )]
    pub multisig: Account<'info, Multisig>,

    /// An ephemeral signer that is used as a seed for the Multisig PDA.
    /// Must be a signer to prevent front-running attack by someone else but the original creator.
    pub create_key: Signer<'info>,

    /// The creator of the multisig.
    #[account(mut)]
    pub creator: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[allow(deprecated)]
impl MultisigCreate<'_> {
    fn validate(&self) -> Result<()> {
        Ok(())
    }

    /// Creates a multisig.
    #[allow(deprecated)]
    #[access_control(ctx.accounts.validate())]
    pub fn multisig_create(ctx: Context<Self>, args: MultisigCreateArgs) -> Result<()> {
        msg!("WARNING: This instruction is deprecated and will be removed soon. Please use `multisig_create_v2` to ensure future compatibility.");

        // Sort the members by pubkey.
        let mut members = args.members;
        members.sort_by_key(|m| m.key);

        // Initialize the multisig.
        let multisig = &mut ctx.accounts.multisig;
        multisig.config_authority = args.config_authority.unwrap_or_default();
        multisig.threshold = args.threshold;
        multisig.time_lock = args.time_lock;
        multisig.transaction_index = 0;
        multisig.stale_transaction_index = 0;
        multisig.create_key = ctx.accounts.create_key.key();
        multisig.bump = ctx.bumps.multisig;
        multisig.members = members;
        multisig.rent_collector = args.rent_collector;

        multisig.invariant()?;

        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(args: MultisigCreateArgs)]
pub struct MultisigCreateV2<'info> {
    /// Global program config account.
    pub program_config: Account<'info, ProgramConfig>,

    /// The treasury where the creation fee is transferred to.
    /// CHECK: validation is performed in the `MultisigCreate::validate()` method.
    #[account(mut)]
    pub treasury: AccountInfo<'info>,

    #[account(
        init,
        payer = creator,
        space = Multisig::size(args.members.len(), args.rent_collector.is_some()),
        seeds = [SEED_PREFIX, SEED_MULTISIG, create_key.key().as_ref()],
        bump
    )]
    pub multisig: Account<'info, Multisig>,

    /// An ephemeral signer that is used as a seed for the Multisig PDA.
    /// Must be a signer to prevent front-running attack by someone else but the original creator.
    pub create_key: Signer<'info>,

    /// The creator of the multisig.
    #[account(mut)]
    pub creator: Signer<'info>,

    pub system_program: Program<'info, System>,
}

impl MultisigCreateV2<'_> {
    fn validate(&self) -> Result<()> {
        //region treasury
        require_keys_eq!(
            self.treasury.key(),
            self.program_config.treasury,
            MultisigError::InvalidAccount
        );
        //endregion

        Ok(())
    }

    /// Creates a multisig.
    #[access_control(ctx.accounts.validate())]
    pub fn multisig_create(ctx: Context<Self>, args: MultisigCreateArgs) -> Result<()> {
        // Sort the members by pubkey.
        let mut members = args.members;
        members.sort_by_key(|m| m.key);

        // Initialize the multisig.
        let multisig = &mut ctx.accounts.multisig;
        multisig.config_authority = args.config_authority.unwrap_or_default();
        multisig.threshold = args.threshold;
        multisig.time_lock = args.time_lock;
        multisig.transaction_index = 0;
        multisig.stale_transaction_index = 0;
        multisig.create_key = ctx.accounts.create_key.key();
        multisig.bump = ctx.bumps.multisig;
        multisig.members = members;
        multisig.rent_collector = args.rent_collector;

        multisig.invariant()?;

        let creation_fee = ctx.accounts.program_config.multisig_creation_fee;

        if creation_fee > 0 {
            system_program::transfer(
                CpiContext::new(
                    ctx.accounts.system_program.to_account_info(),
                    system_program::Transfer {
                        from: ctx.accounts.creator.to_account_info(),
                        to: ctx.accounts.treasury.to_account_info(),
                    },
                ),
                creation_fee,
            )?;
            msg!("Creation fee: {}", creation_fee / LAMPORTS_PER_SOL);
        }

        Ok(())
    }
}
