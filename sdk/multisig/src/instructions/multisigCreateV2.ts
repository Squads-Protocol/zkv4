import { AccountMeta, PublicKey, TransactionInstruction } from "@solana/web3.js";
import {
  createMultisigCreateV2Instruction,
  InitializeCompressedMultisigArgs,
  Member,
  PROGRAM_ID,
} from "../generated";
import { getProgramConfigPda } from "../pda";
export interface LightSpecificAccounts {
  accountCompressionAuthority: PublicKey;
  accountCompressionProgram: PublicKey;
  cpiAuthority: PublicKey;
  squadsProgram: PublicKey;
  lightSystemProgram: PublicKey;
  registeredProgramPda: PublicKey;
  noopProgram: PublicKey;
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
  compressionArgs,
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
  compressionArgs: InitializeCompressedMultisigArgs
  memo?: string;
  programId?: PublicKey;
  remainingAccounts?: AccountMeta[];
}): TransactionInstruction {
  const programConfigPda = getProgramConfigPda({ programId })[0];

  return createMultisigCreateV2Instruction(
    {
      programConfig: programConfigPda,
      multisig: multisigPda,
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
      args: {
        configAuthority,
        threshold,
        members,
        timeLock,
        rentCollector,
        memo: memo ?? null,
        compressionArgs
      },
    },
    programId
  );
}
