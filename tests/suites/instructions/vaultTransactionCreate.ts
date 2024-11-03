import { createRpc } from "@lightprotocol/stateless.js";
import {
    Keypair,
    LAMPORTS_PER_SOL,
    PublicKey,
    TransactionMessage
} from "@solana/web3.js";
import * as multisig from "@sqds/multisig";
import {
    createAutonomousMultisigV2,
    createLocalhostConnection,
    createTestTransferInstruction,
    generateMultisigMembers,
    getTestProgramId,
    TestMembers
} from "../../utils";

const { Multisig, Proposal } = multisig.accounts;

const programId = getTestProgramId();
const connection = createLocalhostConnection();
const zkRpc = createRpc(connection)



describe("Instructions / vault_transaction_create", () => {
    let multisigPda: PublicKey;
    let members: TestMembers;
    before(async () => {
        members = await generateMultisigMembers(connection);

        const createKey = Keypair.generate();
        multisigPda = multisig.getMultisigPda({
            createKey: createKey.publicKey,
            programId,
        })[0];
        const [vaultPda] = multisig.getVaultPda({
            multisigPda,
            index: 0,
            programId,
        });

        // Create new autonomous multisig with rentCollector set to its default vault.
        await createAutonomousMultisigV2({
            connection,
            createKey,
            members,
            threshold: 2,
            timeLock: 0,
            rentCollector: vaultPda,
            programId,
            sendOptions: { skipPreflight: true },
        });

        // Airdrop some SOL to the vault
        let signature = await connection.requestAirdrop(
            vaultPda,
            10 * LAMPORTS_PER_SOL
        );
        await connection.confirmTransaction(signature);
    });

    it("create a new vault transaction", async () => {
        // Derive vault pda
        const [vaultPda] = multisig.getVaultPda({
            multisigPda,
            index: 0,
            programId,
        });
        // Test transfer instruction.
        const testPayee = Keypair.generate();
        const testIx = await createTestTransferInstruction(
            vaultPda,
            testPayee.publicKey,
            1 * LAMPORTS_PER_SOL
        );
        const testTransferMessage = new TransactionMessage({
            payerKey: vaultPda,
            recentBlockhash: (await connection.getLatestBlockhash()).blockhash,
            instructions: [testIx],
        });

        await multisig.rpc.vaultTransactionCreate({
            connection,
            zkRpc,
            feePayer: members.proposer,
            multisigPda,
            transactionIndex: 1n,
            creator: members.proposer.publicKey,
            vaultIndex: 0,
            ephemeralSigners: 0,
            transactionMessage: testTransferMessage,
            addressLookupTableAccounts: [],
            programId,
            sendOptions: { skipPreflight: true },
        });
    });
});