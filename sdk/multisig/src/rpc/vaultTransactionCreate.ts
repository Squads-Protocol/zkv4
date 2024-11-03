import { bn, defaultTestStateTreeAccounts, deriveAddress, deriveAddressSeed, getIndexOrAdd, Rpc, toAccountMetas } from "@lightprotocol/stateless.js";
import {
  AddressLookupTableAccount,
  Connection,
  PublicKey,
  SendOptions,
  Signer,
  TransactionMessage,
  TransactionSignature
} from "@solana/web3.js";
import { translateAndThrowAnchorError } from "../errors";
import { LightMultisig, MutateCompressedMultisigArgs, PackedMerkleContext, PROGRAM_ID } from "../generated";
import * as transactions from "../transactions";

/** Create a new vault transaction. */
export async function vaultTransactionCreate({
  connection,
  zkRpc,
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
  signers,
  sendOptions,
  programId = PROGRAM_ID
}: {
  connection: Connection;
  zkRpc: Rpc;
  feePayer: Signer;
  multisigPda: PublicKey;
  transactionIndex: bigint;
  /** Member of the multisig that is creating the transaction. */
  creator: PublicKey;
  /** Payer for the transaction account rent. If not provided, `creator` is used. */
  rentPayer?: PublicKey;
  vaultIndex: number;
  /** Number of ephemeral signing PDAs required by the transaction. */
  ephemeralSigners: number;
  /** Transaction message to wrap into a multisig transaction. */
  transactionMessage: TransactionMessage;
  /** `AddressLookupTableAccount`s referenced in `transaction_message`. */
  addressLookupTableAccounts?: AddressLookupTableAccount[];
  memo?: string;
  signers?: Signer[];
  sendOptions?: SendOptions;
  programId?: PublicKey;
}): Promise<TransactionSignature> {
  const blockhash = (await connection.getLatestBlockhash()).blockhash;
  const defaultMerkleTree = defaultTestStateTreeAccounts().merkleTree;
  const defaultNullifierQueue = defaultTestStateTreeAccounts().nullifierQueue;
  const defaultAddressTree = defaultTestStateTreeAccounts().addressTree;
  const defaultAddressQueue = defaultTestStateTreeAccounts().addressQueue;

  const compressedMultisigSeeds = deriveAddressSeed([multisigPda.toBytes()], programId);
  const compressedMultisigAddress = deriveAddress(compressedMultisigSeeds, defaultAddressTree);

  const compressedMultisig = await zkRpc.getCompressedAccount(bn(compressedMultisigAddress.toBytes()));
  if (!compressedMultisig || !compressedMultisig.data) throw new Error("Failed to fetch compressed multisig account");
  const compressedMultisigProof = await zkRpc.getValidityProofV0([
    {
      hash: bn(compressedMultisig.hash),
      tree: defaultMerkleTree,
      queue: defaultNullifierQueue,
    }
  ], undefined);

  //const multisigAccount = LightMultisig.fromAccountInfo(multisigAccountInfo)[0];
  const multisigAccount = LightMultisig.deserialize(Buffer.from([
    ...new Uint8Array(compressedMultisig.data.discriminator),
    ...compressedMultisig.data.data
  ]))[0];
  console.log("Proof: ", compressedMultisigProof);
  const remainingPubkeys: PublicKey[] = [];
  const merkleContext: PackedMerkleContext = {
    merkleTreePubkeyIndex: getIndexOrAdd(remainingPubkeys, compressedMultisigProof.merkleTrees[0]),
    nullifierQueuePubkeyIndex: getIndexOrAdd(remainingPubkeys, compressedMultisigProof.nullifierQueues[0]),
    leafIndex: compressedMultisigProof.leafIndices[0],
    queueIndex: null,
  };
  console.log("Merkle Context: ", merkleContext);
  console.log("Remaining Pubkeys: ", remainingPubkeys);

  const remainingAccounts = toAccountMetas(remainingPubkeys);
  const compressedMultisigArgs: MutateCompressedMultisigArgs = {
    compressedProof: compressedMultisigProof.compressedProof,
    address: [...compressedMultisigAddress.toBytes()],
    multisigData: multisigAccount,
    merkleContext: merkleContext,
    merkleTreeRootIndex: compressedMultisigProof.rootIndices[0],

  };

  const tx = transactions.vaultTransactionCreate({
    blockhash,
    feePayer: feePayer.publicKey,
    multisigPda,
    transactionIndex,
    creator,
    rentPayer,
    vaultIndex,
    ephemeralSigners,
    transactionMessage,
    addressLookupTableAccounts,
    compressionArgs: compressedMultisigArgs,
    memo,
    programId,
    remainingAccounts

  });

  tx.sign([feePayer, ...(signers ?? [])]);

  try {
    if (sendOptions) {
      return await connection.sendRawTransaction(tx.serialize(), sendOptions);
    } else {
      return await connection.sendTransaction(tx, sendOptions);
    }
  } catch (err) {
    translateAndThrowAnchorError(err);
  }
}
