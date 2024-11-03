use std::cmp::max;

use super::{multisig::*, seeds::*};
use crate::utils::{
    input_compressed_account, new_compressed_account, output_compressed_account,
    validate_merkle_trees,
};
use crate::MultisigCreateV2;
use crate::{errors::*, id};
use anchor_lang::system_program;
use anchor_lang::{prelude::*, Bumps};
use light_hasher::bytes::AsByteVec;
use light_sdk::address::{derive_address, derive_address_seed};
use light_sdk::compressed_account::serialize_and_hash_account;
use light_sdk::merkle_context::{
    PackedAddressMerkleContext, PackedMerkleContext, PackedMerkleOutputContext,
};
use light_sdk::proof::CompressedProof;
use light_sdk::traits::{
    InvokeAccounts, InvokeCpiAccounts, InvokeCpiContextAccount, LightSystemAccount, SignerAccounts,
};
use light_sdk::utils::{create_cpi_inputs_for_account_update, create_cpi_inputs_for_new_account};
use light_sdk::verify::verify;
use light_sdk::{light_account, CPI_AUTHORITY_PDA_SEED};
use light_utils::hash_to_bn254_field_size_be;

/// Initialization parameters for a compressed multisig account
#[derive(AnchorSerialize, AnchorDeserialize, Debug, Clone)]
pub struct InitializeCompressedMultisigArgs {
    /// Compressed proof for account verification
    pub compressed_proof: CompressedProof,

    /// Root index in the address tree
    pub address_tree_root_index: u16,

    /// Index of the merkle tree account in remaining accounts
    pub merkle_tree_account_index: u8,

    /// Index of the address tree account in remaining accounts
    pub address_tree_account_index: u8,

    /// Index of the address queue account in remaining accounts
    pub address_queue_account_index: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Debug, Clone)]
pub struct MutateCompressedMultisigArgs {
    /// Compressed proof for account verification
    pub compressed_proof: CompressedProof,

    /// Address of the compressed multisig account
    pub address: [u8; 32],

    /// State/Data of the current multisig account
    pub multisig_data: LightMultisigData,

    /// Root index in the address tree
    pub merkle_tree_root_index: u16,

    /// Merkle tree context
    pub merkle_context: PackedMerkleContext,
}

#[light_account]
#[derive(Clone, Debug, Default)]
pub struct LightMultisig {
    /// Key that is used to seed the multisig PDA.
    #[truncate]
    pub create_key: Pubkey,
    /// The authority that can change the multisig config.
    /// This is a very important parameter as this authority can change the members and threshold.
    ///
    /// The convention is to set this to `Pubkey::default()`.
    /// In this case, the multisig becomes autonomous, so every config change goes through
    /// the normal process of voting by the members.
    ///
    /// However, if this parameter is set to any other key, all the config changes for this multisig
    /// will need to be signed by the `config_authority`. We call such a multisig a "controlled multisig".
    #[truncate]
    pub config_authority: Pubkey,
    /// Threshold for signatures.
    pub threshold: u16,
    /// How many seconds must pass between transaction voting settlement and execution.
    pub time_lock: u32,
    /// Last transaction index. 0 means no transactions have been created.
    pub transaction_index: u64,
    /// Last stale transaction index. All transactions up until this index are stale.
    /// This index is updated when multisig config (members/threshold/time_lock) changes.
    pub stale_transaction_index: u64,
    /// The address where the rent for the accounts related to executed, rejected, or cancelled
    /// transactions can be reclaimed. If set to `None`, the rent reclamation feature is turned off.
    #[truncate]
    pub rent_collector: OptionPubkey,
    /// Bump for the multisig PDA seed.
    pub bump: u8,
    /// Members of the multisig.
    #[truncate]
    pub members: MemberList,
}

/// This type exists so that it can be used easily with solita. Using an account
/// as an arg creates weird issue wrt to the account discriminator.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct LightMultisigData {
    pub create_key: Pubkey,
    pub config_authority: Pubkey,
    pub threshold: u16,
    pub time_lock: u32,
    pub transaction_index: u64,
    pub stale_transaction_index: u64,
    pub rent_collector: OptionPubkey,
    pub bump: u8,
    pub members: MemberList,
}

impl From<&LightMultisigData> for LightMultisig {
    fn from(value: &LightMultisigData) -> Self {
        Self {
            create_key: value.create_key,
            config_authority: value.config_authority,
            threshold: value.threshold,
            time_lock: value.time_lock,
            transaction_index: value.transaction_index,
            stale_transaction_index: value.stale_transaction_index,
            rent_collector: value.rent_collector.clone(),
            bump: value.bump,
            members: value.members.clone(),
        }
    }
}
impl LightMultisig {
    pub fn size(members_length: usize) -> usize {
        8  + // anchor account discriminator
        32 + // create_key
        32 + // config_authority
        2  + // threshold
        4  + // time_lock
        8  + // transaction_index
        8  + // stale_transaction_index
        1  + // rent_collector Option discriminator
        32 + // rent_collector (always 32 bytes, even if None, just to keep the realloc logic simpler)
        1  + // bump
        4  + // members vector length
        members_length * Member::INIT_SPACE // members
    }

    pub fn num_voters(members: &[Member]) -> usize {
        members
            .iter()
            .filter(|m| m.permissions.has(Permission::Vote))
            .count()
    }

    pub fn num_proposers(members: &[Member]) -> usize {
        members
            .iter()
            .filter(|m| m.permissions.has(Permission::Initiate))
            .count()
    }

    pub fn num_executors(members: &[Member]) -> usize {
        members
            .iter()
            .filter(|m| m.permissions.has(Permission::Execute))
            .count()
    }

    /// Check if the multisig account space needs to be reallocated to accommodate `members_length`.
    /// Returns `true` if the account was reallocated.
    pub fn realloc_if_needed<'a>(
        multisig: AccountInfo<'a>,
        members_length: usize,
        rent_payer: Option<AccountInfo<'a>>,
        system_program: Option<AccountInfo<'a>>,
    ) -> Result<bool> {
        // Sanity checks
        require_keys_eq!(*multisig.owner, id(), MultisigError::IllegalAccountOwner);

        let current_account_size = multisig.data.borrow().len();
        let account_size_to_fit_members = Multisig::size(members_length);

        // Check if we need to reallocate space.
        if current_account_size >= account_size_to_fit_members {
            return Ok(false);
        }

        let new_size = max(
            current_account_size + (10 * Member::INIT_SPACE), // We need to allocate more space. To avoid doing this operation too often, we increment it by 10 members.
            account_size_to_fit_members,
        );
        // Reallocate more space.
        AccountInfo::realloc(&multisig, new_size, false)?;

        // If more lamports are needed, transfer them to the account.
        let rent_exempt_lamports = Rent::get().unwrap().minimum_balance(new_size).max(1);
        let top_up_lamports =
            rent_exempt_lamports.saturating_sub(multisig.to_account_info().lamports());

        if top_up_lamports > 0 {
            let system_program = system_program.ok_or(MultisigError::MissingAccount)?;
            require_keys_eq!(
                *system_program.key,
                system_program::ID,
                MultisigError::InvalidAccount
            );

            let rent_payer = rent_payer.ok_or(MultisigError::MissingAccount)?;

            system_program::transfer(
                CpiContext::new(
                    system_program,
                    system_program::Transfer {
                        from: rent_payer,
                        to: multisig,
                    },
                ),
                top_up_lamports,
            )?;
        }

        Ok(true)
    }

    // Makes sure the multisig state is valid.
    // This must be called at the end of every instruction that modifies a Multisig account.
    pub fn invariant(&self) -> Result<()> {
        let Self {
            threshold,
            members,
            transaction_index,
            stale_transaction_index,
            ..
        } = self;

        // Max number of members is u16::MAX.
        require!(
            members.0.len() <= usize::from(u16::MAX),
            MultisigError::TooManyMembers
        );

        // There must be no duplicate members.
        let has_duplicates = members.0.windows(2).any(|win| win[0].key == win[1].key);
        require!(!has_duplicates, MultisigError::DuplicateMember);

        // Members must not have unknown permissions.
        require!(
            members.0.iter().all(|m| m.permissions.mask < 8), // 8 = Initiate | Vote | Execute
            MultisigError::UnknownPermission
        );

        // There must be at least one member with Initiate permission.
        let num_proposers = Self::num_proposers(&members.0);
        require!(num_proposers > 0, MultisigError::NoProposers);

        // There must be at least one member with Execute permission.
        let num_executors = Self::num_executors(&members.0);
        require!(num_executors > 0, MultisigError::NoExecutors);

        // There must be at least one member with Vote permission.
        let num_voters = Self::num_voters(&members.0);
        require!(num_voters > 0, MultisigError::NoVoters);

        // Threshold must be greater than 0.
        require!(*threshold > 0, MultisigError::InvalidThreshold);

        // Threshold must not exceed the number of voters.
        require!(
            usize::from(*threshold) <= num_voters,
            MultisigError::InvalidThreshold
        );

        // `state.stale_transaction_index` must be less than or equal to `state.transaction_index`.
        require!(
            stale_transaction_index <= transaction_index,
            MultisigError::InvalidStaleTransactionIndex
        );

        // Time Lock must not exceed the maximum allowed to prevent bricking the multisig.
        require!(
            self.time_lock <= MAX_TIME_LOCK,
            MultisigError::TimeLockExceedsMaxAllowed
        );

        Ok(())
    }

    /// Makes the transactions created up until this moment stale.
    /// Should be called whenever any multisig parameter related to the voting consensus is changed.
    pub fn invalidate_prior_transactions(&mut self) {
        self.stale_transaction_index = self.transaction_index;
    }

    /// Returns `Some(index)` if `member_pubkey` is a member, with `index` into the `members` vec.
    /// `None` otherwise.
    pub fn is_member(&self, member_pubkey: Pubkey) -> Option<usize> {
        self.members
            .0
            .binary_search_by_key(&member_pubkey, |m| m.key)
            .ok()
    }

    pub fn member_has_permission(&self, member_pubkey: Pubkey, permission: Permission) -> bool {
        match self.is_member(member_pubkey) {
            Some(index) => self.members.0[index].permissions.has(permission),
            _ => false,
        }
    }

    /// How many "reject" votes are enough to make the transaction "Rejected".
    /// The cutoff must be such that it is impossible for the remaining voters to reach the approval threshold.
    /// For example: total voters = 7, threshold = 3, cutoff = 5.
    pub fn cutoff(&self) -> usize {
        Self::num_voters(&self.members.0)
            .checked_sub(usize::from(self.threshold))
            .unwrap()
            .checked_add(1)
            .unwrap()
    }

    /// Add `new_member` to the multisig `members` vec and sort the vec.
    pub fn add_member(&mut self, new_member: Member) {
        self.members.0.push(new_member);
        self.members.0.sort_by_key(|m| m.key);
    }

    /// Remove `member_pubkey` from the multisig `members` vec.
    ///
    /// # Errors
    /// - `MultisigError::NotAMember` if `member_pubkey` is not a member.
    pub fn remove_member(&mut self, member_pubkey: Pubkey) -> Result<()> {
        let old_member_index = match self.is_member(member_pubkey) {
            Some(old_member_index) => old_member_index,
            None => return err!(MultisigError::NotAMember),
        };

        self.members.0.remove(old_member_index);

        Ok(())
    }
    pub fn compressed_create<'info>(
        &self,
        ctx: &Context<'_, '_, 'info, 'info, MultisigCreateV2<'info>>,
        args: InitializeCompressedMultisigArgs,
    ) -> Result<()> {
        // Validate the passed in merkle trees
        validate_merkle_trees(
            args.merkle_tree_account_index,
            Some(args.address_tree_account_index),
            Some(args.address_queue_account_index),
            None,
            ctx.remaining_accounts,
        )?;

        // Generate merkle contexts
        let merkle_output_context = PackedMerkleOutputContext {
            merkle_tree_pubkey_index: args.merkle_tree_account_index,
        };

        let address_merkle_context = PackedAddressMerkleContext {
            address_merkle_tree_pubkey_index: args.address_tree_account_index,
            address_queue_pubkey_index: args.address_queue_account_index,
        };

        let multisig_address_seed =
            derive_address_seed(&[&ctx.accounts.multisig.key().as_ref()], &id());

        // Get CPI parameters
        let (state_output_params, new_address_params) = new_compressed_account(
            self,
            &multisig_address_seed,
            &id(),
            &merkle_output_context,
            &address_merkle_context,
            args.address_tree_root_index,
            &ctx.remaining_accounts,
        )?;

        // Create and verify the account
        let bump = ctx.bumps.cpi_authority;
        let signer_seeds = [CPI_AUTHORITY_PDA_SEED, &[bump]];

        let cpi_inputs = create_cpi_inputs_for_new_account(
            args.compressed_proof,
            new_address_params,
            state_output_params,
            None,
        );

        verify(ctx, &cpi_inputs, &[&signer_seeds])
    }

    pub fn compressed_mutate<'info, T>(
        &self,
        ctx: Context<'_, '_, '_, 'info, T>,
        args: MutateCompressedMultisigArgs,
        cpi_authority_seeds: &[&[&[u8]]],
    ) -> Result<()>
    where
        T: InvokeAccounts<'info>
            + LightSystemAccount<'info>
            + InvokeCpiAccounts<'info>
            + SignerAccounts<'info>
            + InvokeCpiContextAccount<'info>
            + Bumps,
    {
        let multisig_data = LightMultisig::from(&args.multisig_data);

        let old_compressed_multisig = input_compressed_account(
            &multisig_data,
            &args.address,
            &crate::id(),
            &args.merkle_context,
            args.merkle_tree_root_index,
        )?;

        let new_compressed_multisig =
            output_compressed_account(self, &args.address, &crate::id(), &args.merkle_context)?;

        let cpi_inputs = create_cpi_inputs_for_account_update(
            args.compressed_proof,
            old_compressed_multisig,
            new_compressed_multisig,
            None,
        );

        verify(&ctx, &cpi_inputs, cpi_authority_seeds)
    }
}

/// Stuff for light account serialization
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, Default)]
pub struct MemberList(pub Vec<Member>);

impl AsByteVec for MemberList {
    fn as_byte_vec(&self) -> Vec<Vec<u8>> {
        self.0
            .iter()
            .map(|member| {
                let member_bytes = member.try_to_vec().unwrap();
                let truncated_member_bytes = hash_to_bn254_field_size_be(&member_bytes.as_slice())
                    .unwrap()
                    .0;
                truncated_member_bytes.to_vec()
            })
            .collect()
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct OptionPubkey(pub Option<Pubkey>);

impl Default for OptionPubkey {
    fn default() -> Self {
        Self(None)
    }
}

// Implement AsByteVec for TruncatedOptionPubkey
impl AsByteVec for OptionPubkey {
    fn as_byte_vec(&self) -> Vec<Vec<u8>> {
        match &self.0 {
            Some(pubkey) => {
                let pubkey_bytes = pubkey.try_to_vec().unwrap();
                let truncated_pubkey_bytes = hash_to_bn254_field_size_be(&pubkey_bytes.as_slice())
                    .unwrap()
                    .0;
                vec![truncated_pubkey_bytes.to_vec()]
            }
            None => vec![vec![0u8; 32]], // or whatever default you want for None case
        }
    }
}
