use anchor_lang::prelude::*;
use light_sdk::{light_system_accounts, LightTraits, CPI_AUTHORITY_PDA_SEED};

use crate::errors::*;
use crate::state::*;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct ProposalActivateArgs {
    pub compression_args: MutateOrVerifyCompressedMultisigArgs,
}

#[light_system_accounts]
#[derive(Accounts, LightTraits)]
#[instruction(args: ProposalActivateArgs)]
pub struct ProposalActivate<'info> {
    // CHECK: Multisig is read-only which means validation needs to happen
    // explicitly via multisig.compressed_verify_state()
    #[account(
        seeds = [SEED_PREFIX, SEED_MULTISIG, args.compression_args.multisig_data.create_key.as_ref()],
        bump = args.compression_args.multisig_data.bump,
    )]
    pub multisig: AccountInfo<'info>,

    #[account(mut)]
    pub member: Signer<'info>,

    #[fee_payer]
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        mut,
        seeds = [
            SEED_PREFIX,
            multisig.key().as_ref(),
            SEED_TRANSACTION,
            &proposal.transaction_index.to_le_bytes(),
            SEED_PROPOSAL,
        ],
        bump = proposal.bump,
    )]
    pub proposal: Account<'info, Proposal>,

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

impl<'info> ProposalActivate<'info> {
    fn validate(
        &self,
        ctx: &Context<'_, '_, 'info, 'info, Self>,
        args: &ProposalActivateArgs,
    ) -> Result<()> {
        let Self {
            proposal, member, ..
        } = self;

        let multisig = LightMultisig::from(&args.compression_args.multisig_data);

        // Validate the multisig state
        multisig.compressed_verify_state(ctx, &args.compression_args)?;

        // `member`
        require!(
            multisig.is_member(member.key()).is_some(),
            MultisigError::NotAMember
        );
        require!(
            // We consider this action a part of the proposal initiation.
            multisig.member_has_permission(member.key(), Permission::Initiate),
            MultisigError::Unauthorized
        );

        // `proposal`
        require!(
            matches!(proposal.status, ProposalStatus::Draft { .. }),
            MultisigError::InvalidProposalStatus
        );
        require!(
            proposal.transaction_index > multisig.stale_transaction_index,
            MultisigError::StaleProposal
        );

        Ok(())
    }

    /// Update status of a multisig proposal from `Draft` to `Active`.
    #[access_control(ctx.accounts.validate(&ctx, &args))]
    pub fn proposal_activate(
        ctx: Context<'_, '_, 'info, 'info, Self>,
        args: ProposalActivateArgs,
    ) -> Result<()> {
        ctx.accounts.proposal.status = ProposalStatus::Active {
            timestamp: Clock::get()?.unix_timestamp,
        };

        Ok(())
    }
}
