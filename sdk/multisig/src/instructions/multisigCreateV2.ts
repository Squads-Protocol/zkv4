import { AccountMeta, PublicKey, TransactionInstruction } from "@solana/web3.js";
import {
  createMultisigCreateV2Instruction,
  Member,
  PackedAddressMerkleContext,
  PROGRAM_ID,
} from "../generated";
import { getProgramConfigPda } from "../pda";
import { CompressedProof, PackedMerkleContext} from "@lightprotocol/stateless.js";
export interface LightSpecificAccounts {
  accountCompressionAuthority: PublicKey;
  accountCompressionProgram: PublicKey;
  cpiAuthority: PublicKey;
  squadsProgram: PublicKey;
  lightSystemProgram: PublicKey;
  registeredProgramPda: PublicKey;
  noopProgram: PublicKey;
}

export interface LightArgs {
  inputs: Uint8Array[];
  proof: CompressedProof;
  merkleContext: PackedMerkleContext;
  merkleTreeRootIndex: number;
  addressMerkleContext: PackedAddressMerkleContext;
  addressMerkleTreeRootIndex: number;
}


export function multisigCreateV2({
  treasury,
  creator,
  multisigPda,
  configAuthority,
  threshold,
  members,
  timeLock,
  createKey,
  rentCollector,
  lightSpecificAccounts,
  lightArgs,
  memo,
  programId = PROGRAM_ID,
  remainingAccounts,
}: {
  treasury: PublicKey;
  creator: PublicKey;
  multisigPda: PublicKey;
  configAuthority: PublicKey | null;
  threshold: number;
  members: Member[];
  timeLock: number;
  createKey: PublicKey;
  rentCollector: PublicKey | null;
  lightSpecificAccounts: LightSpecificAccounts
  lightArgs: LightArgs
  memo?: string;
  programId?: PublicKey;
  remainingAccounts?: AccountMeta[];
}): TransactionInstruction {
  const programConfigPda = getProgramConfigPda({ programId })[0];

  return createMultisigCreateV2Instruction(
    {
      programConfig: programConfigPda,
      treasury,
      creator,
      createKey,
      accountCompressionAuthority: lightSpecificAccounts.accountCompressionAuthority,
      accountCompressionProgram: lightSpecificAccounts.accountCompressionProgram,
      cpiAuthority: lightSpecificAccounts.cpiAuthority,
      squadsProgram: lightSpecificAccounts.squadsProgram,
      lightSystemProgram: lightSpecificAccounts.lightSystemProgram,
      registeredProgramPda: lightSpecificAccounts.registeredProgramPda,
      noopProgram: lightSpecificAccounts.noopProgram,
      anchorRemainingAccounts: remainingAccounts,
    },
    {
      inputs: lightArgs.inputs,
      proof: lightArgs.proof,
      merkleContext: lightArgs.merkleContext,
      merkleTreeRootIndex: lightArgs.merkleTreeRootIndex,
      addressMerkleContext: lightArgs.addressMerkleContext,
      addressMerkleTreeRootIndex: lightArgs.addressMerkleTreeRootIndex,
      args: {
        configAuthority,
        threshold,
        members,
        timeLock,
        rentCollector,
        memo: memo ?? null,
      },
    },
    programId
  );
}
