import {
  AccountMeta,
  Connection,
  PublicKey,
  SendOptions,
  Signer,
  TransactionSignature,
} from "@solana/web3.js";
import { translateAndThrowAnchorError } from "../errors";
import { Member } from "../generated";
import { LightArgs, LightSpecificAccounts } from "../instructions";
import * as transactions from "../transactions";

/** Creates a new multisig. */
export async function multisigCreateV2({
  connection,
  treasury,
  createKey,
  creator,
  multisigPda,
  configAuthority,
  threshold,
  members,
  timeLock,
  rentCollector,
  memo,
  sendOptions,
  programId,
  lightAccounts,
  lightArgs,
  remainingAccounts
}: {
  connection: Connection;
  treasury: PublicKey;
  createKey: Signer;
  creator: Signer;
  multisigPda: PublicKey;
  configAuthority: PublicKey | null;
  threshold: number;
  members: Member[];
  timeLock: number;
  rentCollector: PublicKey | null;
  lightAccounts: LightSpecificAccounts,
  lightArgs: LightArgs
  memo?: string;
  sendOptions?: SendOptions;
  programId?: PublicKey;
  remainingAccounts?: AccountMeta[];
}): Promise<TransactionSignature> {
  const blockhash = (await connection.getLatestBlockhash()).blockhash;

  const tx = transactions.multisigCreateV2({
    blockhash,
    treasury,
    createKey: createKey.publicKey,
    creator: creator.publicKey,
    multisigPda,
    configAuthority,
    threshold,
    members,
    timeLock,
    rentCollector,
    memo,
    programId,
    lightAccounts,
    lightArgs,
    remainingAccounts
  });

  tx.sign([creator, createKey]);

  try {
    return await connection.sendRawTransaction(tx.serialize(), sendOptions);
  } catch (err) {
    translateAndThrowAnchorError(err);
  }
}
