import {
  AccountMeta,
  AddressLookupTableAccount,
  ComputeBudgetProgram,
  PublicKey,
  TransactionMessage,
  VersionedTransaction
} from "@solana/web3.js";
import { MutateCompressedMultisigArgs } from "../generated/types/MutateCompressedMultisigArgs";
import * as instructions from "../instructions/index";

/**
 * Returns unsigned `VersionedTransaction` that needs to be
 * signed by `creator`, `rentPayer` and `feePayer` before sending it.
 */
export function vaultTransactionCreate({
  blockhash,
  feePayer,
  multisigPda,
  transactionIndex,
  creator,
  rentPayer,
  vaultIndex,
  ephemeralSigners,
  transactionMessage,
  addressLookupTableAccounts,
  memo,
  programId,
  compressionArgs,
  remainingAccounts
}: {
  blockhash: string;
  feePayer: PublicKey;
  multisigPda: PublicKey;
  transactionIndex: bigint;
  /** Member of the multisig that is creating the transaction. */
  creator: PublicKey;
  /** Payer for the transaction account rent. If not provided, `creator` is used. */
  rentPayer?: PublicKey;
  vaultIndex: number;
  /** Number of additional signing PDAs required by the transaction. */
  ephemeralSigners: number;
  /** Transaction message to wrap into a multisig transaction. */
  transactionMessage: TransactionMessage;
  /** `AddressLookupTableAccount`s referenced in `transaction_message`. */
  addressLookupTableAccounts?: AddressLookupTableAccount[];
  /** Compression related arguments for the compressed multisig account. */
  compressionArgs: MutateCompressedMultisigArgs;
  /** Remaining accounts used for compression */
  remainingAccounts: AccountMeta[];
  memo?: string;
  programId?: PublicKey;
}): VersionedTransaction {
  const computeBudgetIx = ComputeBudgetProgram.setComputeUnitLimit({
    units: 500_000,
  });

  const message = new TransactionMessage({
    payerKey: feePayer,
    recentBlockhash: blockhash,
    instructions: [
      computeBudgetIx,
      instructions.vaultTransactionCreate({
        multisigPda,
        transactionIndex,
        creator,
        rentPayer,
        vaultIndex,
        ephemeralSigners,
        transactionMessage,
        addressLookupTableAccounts,
        memo,
        programId,
        compressionArgs,
        remainingAccounts

      }),
    ],
  }).compileToV0Message();

  return new VersionedTransaction(message);
}
