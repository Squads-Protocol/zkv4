use anchor_lang::prelude::*;
use light_sdk::light_system_accounts;
use light_sdk::LightTraits;
use light_sdk::CPI_AUTHORITY_PDA_SEED;

use crate::errors::*;
use crate::state::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct ProposalCreateArgs {
    /// Index of the multisig transaction this proposal is associated with.
    pub transaction_index: u64,
    /// Whether the proposal should be initialized with status `Draft`.
    pub draft: bool,
    /// Args needed for compression
    pub compression_args: MutateOrVerifyCompressedMultisigArgs,
}
#[light_system_accounts]
#[derive(Accounts, LightTraits)]
#[instruction(args: ProposalCreateArgs)]
pub struct ProposalCreate<'info> {
    // CHECK: Validation happens explicitly in validate() since the multisig is read-only
    #[account(
        seeds = [SEED_PREFIX, SEED_MULTISIG, args.compression_args.multisig_data.create_key.as_ref()],
        bump = args.compression_args.multisig_data.bump,
    )]
    pub multisig: AccountInfo<'info>,

    #[account(
        init,
        payer = rent_payer,
        space = Proposal::size(args.compression_args.multisig_data.members.len()),
        seeds = [
            SEED_PREFIX,
            multisig.key().as_ref(),
            SEED_TRANSACTION,
            &args.transaction_index.to_le_bytes(),
            SEED_PROPOSAL,
        ],
        bump
    )]
    pub proposal: Account<'info, Proposal>,

    /// The member of the multisig that is creating the proposal.
    #[fee_payer]
    pub creator: Signer<'info>,

    /// The payer for the proposal account rent.
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

impl<'info> ProposalCreate<'info> {
    fn validate(
        &self,
        ctx: &Context<'_, '_, 'info, 'info, Self>,
        args: &ProposalCreateArgs,
    ) -> Result<()> {
        let Self { creator, .. } = self;
        // Parse the multisig from the compression args
        let multisig = LightMultisig::from(&args.compression_args.multisig_data);

        // Validate the state of the multisig
        multisig.compressed_verify_state(ctx, &args.compression_args)?;

        let creator_key = creator.key();

        // args
        // We can only create a proposal for an existing transaction.
        require!(
            args.transaction_index <= multisig.transaction_index,
            MultisigError::InvalidTransactionIndex
        );

        // We can't create a proposal for a stale transaction.
        require!(
            args.transaction_index > multisig.stale_transaction_index,
            MultisigError::StaleProposal
        );

        // creator
        // Has to be a member.
        require!(
            multisig.is_member(self.creator.key()).is_some(),
            MultisigError::NotAMember
        );

        // Must have at least one of the following permissions: Initiate or Vote.
        require!(
            multisig.member_has_permission(creator_key, Permission::Initiate)
                || multisig.member_has_permission(creator_key, Permission::Vote),
            MultisigError::Unauthorized
        );

        Ok(())
    }

    /// Create a new multisig proposal.
    #[access_control(ctx.accounts.validate(&ctx, &args))]
    pub fn proposal_create(ctx: Context<'_, '_, 'info, 'info, Self>, args: ProposalCreateArgs) -> Result<()> {
        let proposal = &mut ctx.accounts.proposal;

        proposal.multisig = ctx.accounts.multisig.key();
        proposal.transaction_index = args.transaction_index;
        proposal.status = if args.draft {
            ProposalStatus::Draft {
                timestamp: Clock::get()?.unix_timestamp,
            }
        } else {
            ProposalStatus::Active {
                timestamp: Clock::get()?.unix_timestamp,
            }
        };
        proposal.bump = ctx.bumps.proposal;
        proposal.approved = vec![];
        proposal.rejected = vec![];
        proposal.cancelled = vec![];

        Ok(())
    }
}
