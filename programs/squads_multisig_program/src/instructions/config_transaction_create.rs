use anchor_lang::prelude::*;
use light_sdk::light_system_accounts;
use light_sdk::LightTraits;
use light_sdk::CPI_AUTHORITY_PDA_SEED;

use crate::errors::*;
use crate::state::*;
use crate::utils::get_cpi_authority_seeds;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct ConfigTransactionCreateArgs {
    pub actions: Vec<ConfigAction>,
    pub memo: Option<String>,
    pub compression_args: MutateOrVerifyCompressedMultisigArgs,
}

#[light_system_accounts]
#[derive(Accounts, LightTraits)]
#[instruction(args: ConfigTransactionCreateArgs)]
pub struct ConfigTransactionCreate<'info> {
    // Multisig Account is mutable
    // CHECK: Validation happens implicitly in multisig.compressed_mutate()
    #[account(
        mut,
        seeds = [SEED_PREFIX, SEED_MULTISIG, args.compression_args.multisig_data.create_key.as_ref()],
        bump = args.compression_args.multisig_data.bump,
    )]
    pub multisig: AccountInfo<'info>,

    #[account(
        init,
        payer = rent_payer,
        space = ConfigTransaction::size(&args.actions),
        seeds = [
            SEED_PREFIX,
            multisig.key().as_ref(),
            SEED_TRANSACTION,
            &args.compression_args.multisig_data.transaction_index.checked_add(1).unwrap().to_le_bytes(),
        ],
        bump
    )]
    pub transaction: Account<'info, ConfigTransaction>,

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

impl <'info>ConfigTransactionCreate<'info> {
    fn validate(&self, args: &ConfigTransactionCreateArgs) -> Result<()> {
        // Parse multisig data
        let multisig = LightMultisig::from(&args.compression_args.multisig_data);
        // multisig
        require_keys_eq!(
            multisig.config_authority,
            Pubkey::default(),
            MultisigError::NotSupportedForControlled
        );

        // creator
        require!(
            multisig.is_member(self.creator.key()).is_some(),
            MultisigError::NotAMember
        );
        require!(
            multisig.member_has_permission(self.creator.key(), Permission::Initiate),
            MultisigError::Unauthorized
        );

        // args

        // Config transaction must have at least one action
        require!(!args.actions.is_empty(), MultisigError::NoActions);

        // time_lock must not exceed the maximum allowed.
        for action in &args.actions {
            if let ConfigAction::SetTimeLock { new_time_lock, .. } = action {
                require!(
                    *new_time_lock <= MAX_TIME_LOCK,
                    MultisigError::TimeLockExceedsMaxAllowed
                );
            }
        }

        Ok(())
    }

    /// Create a new config transaction.
    #[access_control(ctx.accounts.validate(&args))]
    pub fn config_transaction_create(
        ctx: Context<'_, '_, 'info, 'info, Self>,
        args: ConfigTransactionCreateArgs,
    ) -> Result<()> {
        let mut multisig = LightMultisig::from(&args.compression_args.multisig_data);

        let multisig_account_info = &mut ctx.accounts.multisig;
        let transaction = &mut ctx.accounts.transaction;
        let creator = &mut ctx.accounts.creator;

        let multisig_key = multisig_account_info.key();

        // Increment the transaction index.
        let transaction_index = multisig.transaction_index.checked_add(1).unwrap();

        // Initialize the transaction fields.
        transaction.multisig = multisig_key;
        transaction.creator = creator.key();
        transaction.index = transaction_index;
        transaction.bump = ctx.bumps.transaction;
        transaction.actions = args.actions;

        // Updated last transaction index in the multisig account.
        multisig.transaction_index = transaction_index;

        // Get cpi_authority_seeds
        let cpi_authority_bump: u8 = ctx.bumps.cpi_authority;
        let cpi_authority_seeds = get_cpi_authority_seeds(&cpi_authority_bump);

        // Implicitly checks the multisig state.
        // This will fail if the multisig data that was passed in is invalid.
        multisig.compressed_mutate(ctx, args.compression_args, &[&cpi_authority_seeds])?;
        multisig.invariant()?;

        // Logs for indexing.
        msg!("transaction index: {}", transaction_index);

        Ok(())
    }
}
