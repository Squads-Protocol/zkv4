import {
  AccountMeta,
  ComputeBudgetInstruction,
  ComputeBudgetProgram,
  PublicKey,
  TransactionMessage,
  VersionedTransaction,
} from "@solana/web3.js";
import { Member } from "../generated";
import * as instructions from "../instructions";
import { LightArgs, LightSpecificAccounts } from "../instructions";

/**
 * Returns unsigned `VersionedTransaction` that needs to be signed by `creator` and `createKey` before sending it.
 */
export function multisigCreateV2({
  blockhash,
  treasury,
  configAuthority,
  createKey,
  creator,
  multisigPda,
  threshold,
  members,
  timeLock,
  rentCollector,
  memo,
  programId,
  lightAccounts,
  lightArgs,
  remainingAccounts
}: {
  blockhash: string;
  treasury: PublicKey;
  createKey: PublicKey;
  creator: PublicKey;
  multisigPda: PublicKey;
  configAuthority: PublicKey | null;
  threshold: number;
  members: Member[];
  timeLock: number;
  rentCollector: PublicKey | null;
  lightAccounts: LightSpecificAccounts;
  lightArgs: LightArgs;
  memo?: string;
    programId?: PublicKey;
  remainingAccounts?: AccountMeta[];
}): VersionedTransaction {
  const ix = instructions.multisigCreateV2({
    treasury,
    creator,
    multisigPda,
    configAuthority,
    threshold,
    members,
    timeLock,
    createKey,
    rentCollector,
    memo,
    programId,
    lightArgs,
    lightSpecificAccounts: lightAccounts,
    remainingAccounts
  });
  const computeBudgetIx = ComputeBudgetProgram.setComputeUnitLimit({
    units: 500_000,
  });
  const message = new TransactionMessage({
    payerKey: creator,
    recentBlockhash: blockhash,
    instructions: [computeBudgetIx,ix],
  }).compileToV0Message();

  return new VersionedTransaction(message);
}
