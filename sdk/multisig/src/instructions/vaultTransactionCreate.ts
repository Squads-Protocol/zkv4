import { defaultStaticAccountsStruct, LightSystemProgram } from "@lightprotocol/stateless.js";
import {
  AccountMeta,
  AddressLookupTableAccount,
  PublicKey,
  TransactionMessage
} from "@solana/web3.js";
import {
  createVaultTransactionCreateInstruction,
  MutateCompressedMultisigArgs,
  PROGRAM_ID,
} from "../generated";
import { getTransactionPda, getVaultPda } from "../pda";
import { transactionMessageToMultisigTransactionMessageBytes } from "../utils";

export function vaultTransactionCreate({
  multisigPda,
  transactionIndex,
  creator,
  rentPayer,
  vaultIndex,
  ephemeralSigners,
  transactionMessage,
  addressLookupTableAccounts,
  compressionArgs,
  remainingAccounts,
  memo,
  programId = PROGRAM_ID,
}: {
  multisigPda: PublicKey;
  transactionIndex: bigint;
  creator: PublicKey;
  rentPayer?: PublicKey;
  vaultIndex: number;
  /** Number of additional signing PDAs required by the transaction. */
  ephemeralSigners: number;
  /** Transaction message to wrap into a multisig transaction. */
  transactionMessage: TransactionMessage;
  /** `AddressLookupTableAccount`s referenced in `transaction_message`. */
  addressLookupTableAccounts?: AddressLookupTableAccount[];
  /** Data and proof required for the compressed multisig account. */
  compressionArgs: MutateCompressedMultisigArgs;
  remainingAccounts: AccountMeta[];
  memo?: string;
  programId?: PublicKey;
}) {
  const [vaultPda] = getVaultPda({
    multisigPda,
    index: vaultIndex,
    programId,
  });

  const [transactionPda] = getTransactionPda({
    multisigPda,
    index: transactionIndex,
    programId,
  });

  const transactionMessageBytes =
    transactionMessageToMultisigTransactionMessageBytes({
      message: transactionMessage,
      addressLookupTableAccounts,
      vaultPda,
    });
  // Defualt static accounts from Light Protocol
  const {
    registeredProgramPda,
    noopProgram,
    accountCompressionProgram,
    accountCompressionAuthority,
  } = defaultStaticAccountsStruct();

  const cpiAuthority = PublicKey.findProgramAddressSync(
    [
      Buffer.from("cpi_authority"),
    ],
    programId
  )[0];
  return createVaultTransactionCreateInstruction(
    {
      multisig: multisigPda,
      transaction: transactionPda,
      creator,
      rentPayer: rentPayer ?? creator,
      lightSystemProgram: LightSystemProgram.programId,
      noopProgram,
      registeredProgramPda,
      accountCompressionProgram,
      accountCompressionAuthority,
      cpiAuthority,
      squadsProgram: programId,
      anchorRemainingAccounts: remainingAccounts,
    },
    {
      args: {
        vaultIndex,
        ephemeralSigners,
        transactionMessage: transactionMessageBytes,
        memo: memo ?? null,
        compressionArgs,
      },
    },
    programId
  );
}
