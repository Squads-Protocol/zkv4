use anchor_lang::prelude::*;
use anchor_lang::system_program;

use light_sdk::compressed_account::LightAccounts;
use light_sdk::light_system_accounts;
use light_sdk::LightTraits;
use light_sdk::CPI_AUTHORITY_PDA_SEED;
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
pub struct MultisigCreateArgsV2 {
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
    /// Parameters for creating the compressed multisig
    pub compression_args: InitializeCompressedMultisigArgs,
}

#[light_system_accounts]
#[derive(Accounts, LightTraits)]
pub struct MultisigCreateV2<'info> {
    /// Global program config account.
    #[account(seeds = [SEED_PREFIX, SEED_PROGRAM_CONFIG], bump)]
    pub program_config: Account<'info, ProgramConfig>,

    /// The treasury where the creation fee is transferred to.
    /// CHECK: validation is performed in the `MultisigCreate::validate()` method.
    #[account(mut)]
    pub treasury: AccountInfo<'info>,

    /// An ephemeral signer that is used as a seed for the Multisig PDA.
    /// Must be a signer to prevent front-running attack by someone else but the original creator.
    pub create_key: Signer<'info>,

    /// The creator of the multisig.
    #[fee_payer]
    #[account(mut)]
    pub creator: Signer<'info>,

    /// CHECK: Checked in light-system-program.
    #[authority]
    #[account(seeds = [CPI_AUTHORITY_PDA_SEED], bump)]
    pub cpi_authority: AccountInfo<'info>,
    #[self_program]
    pub squads_program: Program<'info, crate::program::SquadsMultisigProgram>,
}

impl<'info> MultisigCreateV2<'info> {
    pub fn validate(&self) -> Result<()> {
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
    pub fn multisig_create(ctx: Context<'_, '_, 'info, 'info, Self>, args: MultisigCreateArgsV2) -> Result<()> {
        // Sort the members by pubkey.
        let mut members = args.members;
        members.sort_by_key(|m| m.key);

        // Initialize the multisig.
        let create_key = &ctx.accounts.create_key;
        let multisig_bump = Pubkey::find_program_address(
            &[SEED_PREFIX, SEED_MULTISIG, create_key.key().as_ref()],
            &crate::id(),
        )
        .1;
        let multisig = LightMultisig {
            create_key: create_key.key(),
            config_authority: args.config_authority.unwrap_or_default(),
            threshold: args.threshold,
            time_lock: args.time_lock,
            transaction_index: 0,
            stale_transaction_index: 0,
            bump: multisig_bump,
            members: MemberList(members),
            rent_collector: OptionPubkey(args.rent_collector),
        };

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

        multisig.compressed_create(&ctx, args.compression_args)?;
        Ok(())
    }
}
