use anchor_lang::prelude::*;
use anchor_lang::system_program;
use crate::ParamsMultisigCreateV2;

use light_sdk::compressed_account::LightAccounts;
use light_sdk::{
    compressed_account::LightAccount, context::LightContext, light_account, light_accounts,
    merkle_context::PackedAddressMerkleContext,
};
use solana_program::native_token::LAMPORTS_PER_SOL;

use crate::errors::MultisigError;
use crate::state::*;

// Dummy Account context for multisigCreate, since Anchor doesn't allow empty instructions.
#[derive(Accounts)]
pub struct Deprecated<'info> {
    ///CHECK: Dummy Account
    pub null: AccountInfo<'info>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct MultisigCreateV2Args {
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




#[light_accounts]
pub struct MultisigCreateV2<'info> {
    /// Global program config account.
    #[account(seeds = [SEED_PREFIX, SEED_PROGRAM_CONFIG], bump)]
    pub program_config: Account<'info, ProgramConfig>,

    /// The treasury where the creation fee is transferred to.
    /// CHECK: validation is performed in the `MultisigCreate::validate()` method.
    #[account(mut)]
    pub treasury: AccountInfo<'info>,

    #[light_account(
        init,
        seeds = [SEED_PREFIX, SEED_MULTISIG, create_key.key().as_ref()],
    )]
    pub multisig: LightAccount<LightMultisig>,

    /// An ephemeral signer that is used as a seed for the Multisig PDA.
    /// Must be a signer to prevent front-running attack by someone else but the original creator.
    pub create_key: Signer<'info>,

    /// The creator of the multisig.
    #[fee_payer]
    #[account(mut)]
    pub creator: Signer<'info>,

    /// CHECK: Checked in light-system-program.
    #[authority]
    pub cpi_authority: AccountInfo<'info>,
    #[self_program]
    pub squads_program: Program<'info, crate::program::SquadsMultisigProgram>,
}

impl<'info> MultisigCreateV2<'info> {
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
    pub fn multisig_create(
        mut ctx: LightContext<Self, LightMultisigCreateV2>,
        args: MultisigCreateV2Args,
    ) -> Result<()> {
        // Sort the members by pubkey.
        let mut members = args.members;
        members.sort_by_key(|m| m.key);

        // Initialize the multisig.
        let create_key = &ctx.accounts.create_key;
        let multisig_bump = Pubkey::find_program_address(&[SEED_PREFIX, SEED_MULTISIG, create_key.key().as_ref()], &crate::id()).1;

        ctx.light_accounts.multisig.config_authority = args.config_authority.unwrap_or_default();
        ctx.light_accounts.multisig.threshold = args.threshold;
        ctx.light_accounts.multisig.time_lock = args.time_lock;
        ctx.light_accounts.multisig.transaction_index = 0;
        ctx.light_accounts.multisig.stale_transaction_index = 0;
        ctx.light_accounts.multisig.create_key = ctx.accounts.create_key.key();
        ctx.light_accounts.multisig.bump = multisig_bump;
        ctx.light_accounts.multisig.members = MemberList(members);
        ctx.light_accounts.multisig.rent_collector = args.rent_collector;

        ctx.light_accounts.multisig.invariant()?;

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
