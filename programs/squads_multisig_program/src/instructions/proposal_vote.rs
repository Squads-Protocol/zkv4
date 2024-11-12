use anchor_lang::prelude::*;

use crate::errors::*;
use crate::state::*;
use light_sdk::light_system_accounts;
use light_sdk::LightTraits;
use light_sdk::CPI_AUTHORITY_PDA_SEED;

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct ProposalVoteArgs {
    pub memo: Option<String>,
    pub compression_args: MutateOrVerifyCompressedMultisigArgs,
}

#[light_system_accounts]
#[derive(Accounts, LightTraits)]
#[instruction(args: ProposalVoteArgs)]
pub struct ProposalVote<'info> {
    // CHECK: Validation happens explicitly in validate()
    #[account(
        seeds = [SEED_PREFIX, SEED_MULTISIG, args.compression_args.multisig_data.create_key.as_ref()],
        bump = args.compression_args.multisig_data.bump,
    )]
    pub multisig: AccountInfo<'info>,

    #[fee_payer]
    #[account(mut)]
    pub member: Signer<'info>,

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

#[derive(Accounts)]
pub struct ProposalCancelV2<'info> {
    // The context needed for the ProposalVote instruction
    pub proposal_vote: ProposalVote<'info>,

    pub system_program: Program<'info, System>,
}

impl<'info> ProposalVote<'info> {
    fn validate(
        &self,
        ctx: &Context<'_, '_, 'info, 'info, Self>,
        args: &ProposalVoteArgs,
        vote: Vote,
    ) -> Result<()> {
        let Self {
            proposal, member, ..
        } = self;
        // Parse multisig from compressed data
        let multisig = LightMultisig::from(&args.compression_args.multisig_data);

        // Since the multisig is read-only, explicit validation of its state is
        // required.
        multisig.compressed_verify_state(ctx, &args.compression_args)?;
        // member
        require!(
            multisig.is_member(member.key()).is_some(),
            MultisigError::NotAMember
        );
        require!(
            multisig.member_has_permission(member.key(), Permission::Vote),
            MultisigError::Unauthorized
        );

        // proposal
        match vote {
            Vote::Approve | Vote::Reject => {
                require!(
                    matches!(proposal.status, ProposalStatus::Active { .. }),
                    MultisigError::InvalidProposalStatus
                );
                // CANNOT approve or reject a stale proposal
                require!(
                    proposal.transaction_index > multisig.stale_transaction_index,
                    MultisigError::StaleProposal
                );
            }
            Vote::Cancel => {
                require!(
                    matches!(proposal.status, ProposalStatus::Approved { .. }),
                    MultisigError::InvalidProposalStatus
                );
                // CAN cancel a stale proposal.
            }
        }

        Ok(())
    }

    /// Approve a multisig proposal on behalf of the `member`.
    /// The proposal must be `Active`.
    #[access_control(ctx.accounts.validate(&ctx, &args, Vote::Approve))]
    pub fn proposal_approve(
        ctx: Context<'_, '_, 'info, 'info, Self>,
        args: ProposalVoteArgs,
    ) -> Result<()> {
        let multisig = LightMultisig::from(&args.compression_args.multisig_data);
        let proposal = &mut ctx.accounts.proposal;
        let member = &mut ctx.accounts.member;

        proposal.approve(member.key(), usize::from(multisig.threshold))?;

        Ok(())
    }

    /// Reject a multisig proposal on behalf of the `member`.
    /// The proposal must be `Active`.
    #[access_control(ctx.accounts.validate(&ctx, &args, Vote::Reject))]
    pub fn proposal_reject(
        ctx: Context<'_, '_, 'info, 'info, Self>,
        args: ProposalVoteArgs,
    ) -> Result<()> {
        let multisig = LightMultisig::from(&args.compression_args.multisig_data);
        let proposal = &mut ctx.accounts.proposal;
        let member = &mut ctx.accounts.member;

        let cutoff = LightMultisig::cutoff(&multisig);

        proposal.reject(member.key(), cutoff)?;

        Ok(())
    }

    /// Cancel a multisig proposal on behalf of the `member`.
    /// The proposal must be `Approved`.
    #[access_control(ctx.accounts.validate(&ctx, &args, Vote::Cancel))]
    pub fn proposal_cancel(
        ctx: Context<'_, '_, 'info, 'info, Self>,
        args: ProposalVoteArgs,
    ) -> Result<()> {
        let multisig = LightMultisig::from(&args.compression_args.multisig_data);
        let proposal = &mut ctx.accounts.proposal;
        let member = &mut ctx.accounts.member;

        proposal
            .cancelled
            .retain(|k| multisig.is_member(*k).is_some());

        proposal.cancel(member.key(), usize::from(multisig.threshold))?;

        Ok(())
    }
}

impl<'info> ProposalCancelV2<'info> {
    /// Cancel a multisig proposal on behalf of the `member`.
    /// The proposal must be `Approved`.
    pub fn proposal_cancel_v2(
        ctx: Context<'_, '_, 'info, 'info, Self>,
        args: ProposalVoteArgs,
    ) -> Result<()> {
        // Readonly accounts
        let multisig = LightMultisig::from(&args.compression_args.multisig_data);

        // Account infos necessary for reallocation
        let proposal_account_info = &ctx.accounts.proposal_vote.proposal.to_account_info();
        let member_account_info = &ctx.accounts.proposal_vote.member.to_account_info();
        let system_program_account_info = &ctx.accounts.system_program.to_account_info();

        // Create context for cancel instruction
        let cancel_context = Context::new(
            ctx.program_id,
            &mut ctx.accounts.proposal_vote,
            ctx.remaining_accounts,
            ctx.bumps.proposal_vote,
        );

        // Call cancel instruction
        ProposalVote::proposal_cancel(cancel_context, args)?;

        // Reallocate the proposal size if needed
        Proposal::realloc_if_needed(
            proposal_account_info.clone(),
            multisig.members.len(),
            Some(member_account_info.clone()),
            Some(system_program_account_info.clone()),
        )?;
        Ok(())
    }
}

pub enum Vote {
    Approve,
    Reject,
    Cancel,
}
