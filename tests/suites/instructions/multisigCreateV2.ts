import {
  AccountInfo,
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  TransactionMessage,
  VersionedTransaction
} from "@solana/web3.js";
import * as multisig from "@sqds/multisig";
import assert from "assert";
import {
  comparePubkeys,
  createAutonomousMultisigV2,
  createControlledMultisigV2,
  createLocalhostConnection,
  generateFundedKeypair,
  generateMultisigMembers,
  getTestProgramConfigAuthority,
  getTestProgramId,
  getTestProgramTreasury,
  setupCompressionParams,
  TestMembers,
} from "../../utils";

import {
  bn,
  createRpc
} from "@lightprotocol/stateless.js";

const { Multisig } = multisig.accounts;
const { Permission, Permissions } = multisig.types;

const connection = createLocalhostConnection();
const zkRpc = createRpc(connection);

const programId = getTestProgramId();
const programConfigAuthority = getTestProgramConfigAuthority();
const programTreasury = getTestProgramTreasury();
const programConfigPda = multisig.getProgramConfigPda({ programId })[0];


describe("Instructions / multisig_create_v2", () => {
  let members: TestMembers;
  let programTreasury: PublicKey;

  before(async () => {
    members = await generateMultisigMembers(connection);

    const programConfigPda = multisig.getProgramConfigPda({ programId })[0];
    const programConfig = await multisig.accounts.ProgramConfig.fromAccountAddress(
      connection,
      programConfigPda
    );
    programTreasury = programConfig.treasury;
  });

  it("error: duplicate member", async () => {
    const creator = await generateFundedKeypair(connection);
    const createKey = Keypair.generate();
    const [multisigPda] = multisig.getMultisigPda({
      createKey: createKey.publicKey,
      programId,
    });

    const { lightAccounts, lightArgs, remainingAccounts } = await setupCompressionParams(createKey.publicKey, programId);

    await assert.rejects(
      () =>
        multisig.rpc.multisigCreateV2({
          connection,
          treasury: programTreasury,
          creator,
          multisigPda,
          configAuthority: null,
          timeLock: 0,
          threshold: 1,
          members: [
            {
              key: members.almighty.publicKey,
              permissions: Permissions.all(),
            },
            {
              key: members.almighty.publicKey,
              permissions: Permissions.all(),
            },
          ],
          createKey,
          rentCollector: null,
          programId,
          lightAccounts,
          lightArgs,
          remainingAccounts
        }),
      /Found multiple members with the same pubkey/
    );
  });

  it("error: empty members", async () => {
    const creator = await generateFundedKeypair(connection);
    const createKey = Keypair.generate();
    const [multisigPda] = multisig.getMultisigPda({
      createKey: createKey.publicKey,
      programId,
    });

    const { lightAccounts, lightArgs, remainingAccounts } = await setupCompressionParams(createKey.publicKey, programId);

    await assert.rejects(
      () =>
        multisig.rpc.multisigCreateV2({
          connection,
          treasury: programTreasury,
          createKey,
          creator,
          multisigPda,
          configAuthority: null,
          timeLock: 0,
          threshold: 1,
          members: [],
          rentCollector: null,
          programId,
          lightAccounts,
          lightArgs,
          remainingAccounts
        }),
      /Members don't include any proposers/
    );
  });

  it("error: member has unknown permission", async () => {
    const creator = await generateFundedKeypair(connection);
    const member = Keypair.generate();
    const createKey = Keypair.generate();
    const [multisigPda] = multisig.getMultisigPda({
      createKey: createKey.publicKey,
      programId,
    });

    const { lightAccounts, lightArgs, remainingAccounts } = await setupCompressionParams(createKey.publicKey, programId);

    await assert.rejects(
      () =>
        multisig.rpc.multisigCreateV2({
          connection,
          treasury: programTreasury,
          createKey,
          creator,
          multisigPda,
          configAuthority: null,
          timeLock: 0,
          threshold: 1,
          members: [
            {
              key: member.publicKey,
              permissions: {
                mask: 1 | 2 | 4 | 8,
              },
            },
          ],
          rentCollector: null,
          programId,
          lightAccounts,
          lightArgs,
          remainingAccounts
        }),
      /Member has unknown permission/
    );
  });

  it("error: invalid threshold (< 1)", async () => {
    const creator = await generateFundedKeypair(connection);
    const createKey = Keypair.generate();
    const [multisigPda] = multisig.getMultisigPda({
      createKey: createKey.publicKey,
      programId,
    });

    const { lightAccounts, lightArgs, remainingAccounts } = await setupCompressionParams(createKey.publicKey, programId);

    await assert.rejects(
      () =>
        multisig.rpc.multisigCreateV2({
          connection,
          treasury: programTreasury,
          createKey,
          creator,
          multisigPda,
          configAuthority: null,
          timeLock: 0,
          threshold: 0,
          members: Object.values(members).map((m) => ({
            key: m.publicKey,
            permissions: Permissions.all(),
          })),
          rentCollector: null,
          programId,
          lightAccounts,
          lightArgs,
          remainingAccounts
        }),
      /Invalid threshold, must be between 1 and number of members/
    );
  });

  it("error: invalid threshold (> members with permission to Vote)", async () => {
    const creator = await generateFundedKeypair(connection);
    const createKey = Keypair.generate();
    const [multisigPda] = multisig.getMultisigPda({
      createKey: createKey.publicKey,
      programId,
    });

    const { lightAccounts, lightArgs, remainingAccounts } = await setupCompressionParams(createKey.publicKey, programId);

    await assert.rejects(
      () =>
        multisig.rpc.multisigCreateV2({
          connection,
          treasury: programTreasury,
          createKey,
          creator,
          multisigPda,
          configAuthority: null,
          timeLock: 0,
          members: [
            {
              key: members.almighty.publicKey,
              permissions: Permissions.all(),
            },
            {
              key: members.proposer.publicKey,
              permissions: Permissions.fromPermissions([Permission.Initiate]),
            },
            {
              key: members.voter.publicKey,
              permissions: Permissions.fromPermissions([Permission.Vote]),
            },
            {
              key: members.executor.publicKey,
              permissions: Permissions.fromPermissions([Permission.Execute]),
            },
          ],
          threshold: 3,
          rentCollector: null,
          programId,
          lightAccounts,
          lightArgs,
          remainingAccounts
        }),
      /Invalid threshold, must be between 1 and number of members with Vote permission/
    );
  });

  it("create a new autonomous multisig", async () => {
    const createKey = Keypair.generate();
    const [_multisigPda, multisigBump] = multisig.getMultisigPda({ createKey: createKey.publicKey, programId });
    const [zkmultisigPda, signature] = await createAutonomousMultisigV2({
      connection,
      createKey,
      members,
      threshold: 2,
      timeLock: 0,
      rentCollector: null,
      programId,

    });
    console.log("Autonomous Multisig created with signature", signature);
    // Wait 2s
    await new Promise(resolve => setTimeout(resolve, 2000));
    const compressedMultisigAccount = await zkRpc.getCompressedAccount(bn(zkmultisigPda.toBytes()), undefined);
    if (!compressedMultisigAccount || !compressedMultisigAccount.data) throw new Error("Failed to fetch compressed account");
    const multisigAccountInfo: AccountInfo<Buffer> = {
      owner: compressedMultisigAccount.owner,
      lamports: Number(compressedMultisigAccount.lamports),
      executable: false,
      data: Buffer.from([
        ...new Uint8Array(compressedMultisigAccount.data.discriminator),
        ...compressedMultisigAccount.data.data
      ]),
    }

    const multisigAccount = Multisig.fromAccountInfo(
      multisigAccountInfo
    )[0];

    assert.strictEqual(
      multisigAccount.configAuthority.toBase58(),
      PublicKey.default.toBase58()
    );
    assert.strictEqual(multisigAccount.threshold, 2);
    assert.deepEqual(
      multisigAccount.members,
      [
        {
          key: members.almighty.publicKey,
          permissions: {
            mask: Permission.Initiate | Permission.Vote | Permission.Execute,
          },
        },
        {
          key: members.proposer.publicKey,
          permissions: {
            mask: Permission.Initiate,
          },
        },
        {
          key: members.voter.publicKey,
          permissions: {
            mask: Permission.Vote,
          },
        },
        {
          key: members.executor.publicKey,
          permissions: {
            mask: Permission.Execute,
          },
        },
      ].sort((a, b) => comparePubkeys(a.key, b.key))
    );
    assert.strictEqual(multisigAccount.rentCollector, null);
    assert.strictEqual(multisigAccount.transactionIndex.toString(), "0");
    assert.strictEqual(multisigAccount.staleTransactionIndex.toString(), "0");
    assert.strictEqual(
      multisigAccount.createKey.toBase58(),
      createKey.publicKey.toBase58()
    );
    assert.strictEqual(multisigAccount.bump, multisigBump);
  });

  it("create a new autonomous multisig with rent reclamation enabled", async () => {
    const createKey = Keypair.generate();
    const rentCollector = Keypair.generate().publicKey;

    const [zkmultisigPda, signature] = await createAutonomousMultisigV2({
      connection,
      createKey,
      members,
      threshold: 2,
      timeLock: 0,
      rentCollector,
      programId,
      sendOptions: { skipPreflight: true },
    });
    console.log("Autonomous Multisig w/ rent reclamation enabled", signature);

    await new Promise(resolve => setTimeout(resolve, 2000));
    const compressedMultisigAccount = await zkRpc.getCompressedAccount(bn(zkmultisigPda.toBytes()), undefined);
    if (!compressedMultisigAccount || !compressedMultisigAccount.data) throw new Error("Failed to fetch compressed account");
    const multisigAccountInfo: AccountInfo<Buffer> = {
      owner: compressedMultisigAccount.owner,
      lamports: Number(compressedMultisigAccount.lamports),
      executable: false,
      data: Buffer.from([
        ...new Uint8Array(compressedMultisigAccount.data.discriminator),
        ...compressedMultisigAccount.data.data
      ]),
    }


    const multisigAccount = Multisig.fromAccountInfo(
      multisigAccountInfo
    )[0];

    assert.strictEqual(
      multisigAccount.rentCollector?.toBase58(),
      rentCollector.toBase58()
    );
  });

  it("create a new controlled multisig", async () => {
    const createKey = Keypair.generate();
    const configAuthority = await generateFundedKeypair(connection);

    const [zkmultisigPda, signature] = await createControlledMultisigV2({
      connection,
      createKey,
      configAuthority: configAuthority.publicKey,
      members,
      threshold: 2,
      timeLock: 0,
      rentCollector: null,
      programId,
    });
    console.log("Controlled Multisig created with signature", signature);

    await new Promise(resolve => setTimeout(resolve, 2000));
    const compressedMultisigAccount = await zkRpc.getCompressedAccount(bn(zkmultisigPda.toBytes()), undefined);
    if (!compressedMultisigAccount || !compressedMultisigAccount.data) throw new Error("Failed to fetch compressed account");
    const multisigAccountInfo: AccountInfo<Buffer> = {
      owner: compressedMultisigAccount.owner,
      lamports: Number(compressedMultisigAccount.lamports),
      executable: false,
      data: Buffer.from([
        ...new Uint8Array(compressedMultisigAccount.data.discriminator),
        ...compressedMultisigAccount.data.data
      ]),
    }

    const multisigAccount = Multisig.fromAccountInfo(
      multisigAccountInfo
    )[0];

    assert.strictEqual(
      multisigAccount.configAuthority.toBase58(),
      configAuthority.publicKey.toBase58()
    );
  });

  it("create a new multisig and pay creation fee", async () => {
    //region Airdrop to the program config authority
    let signature = await connection.requestAirdrop(
      programConfigAuthority.publicKey,
      LAMPORTS_PER_SOL
    );
    await connection.confirmTransaction(signature);

    const multisigCreationFee = 0.1 * LAMPORTS_PER_SOL;

    //region Configure the global multisig creation fee
    const setCreationFeeIx = multisig.generated.createProgramConfigSetMultisigCreationFeeInstruction(
      {
        programConfig: programConfigPda,
        authority: programConfigAuthority.publicKey,
      },
      {
        args: { newMultisigCreationFee: multisigCreationFee },
      },
      programId
    );

    const message = new TransactionMessage({
      recentBlockhash: (await connection.getLatestBlockhash()).blockhash,
      payerKey: programConfigAuthority.publicKey,
      instructions: [setCreationFeeIx],
    }).compileToV0Message();

    const tx = new VersionedTransaction(message);
    tx.sign([programConfigAuthority]);
    signature = await connection.sendTransaction(tx);
    await connection.confirmTransaction(signature);

    let programConfig = await multisig.accounts.ProgramConfig.fromAccountAddress(
      connection,
      programConfigPda
    );
    assert.strictEqual(
      programConfig.multisigCreationFee.toString(),
      multisigCreationFee.toString()
    );

    //region Create a new multisig
    const creator = await generateFundedKeypair(connection);
    const createKey = Keypair.generate();
    const { lightAccounts, lightArgs, remainingAccounts, } = await setupCompressionParams(createKey.publicKey, programId);

    const creatorBalancePre = await connection.getBalance(creator.publicKey);

    const multisigPda = multisig.getMultisigPda({
      createKey: createKey.publicKey,
      programId,
    })[0];

    signature = await multisig.rpc.multisigCreateV2({
      connection,
      treasury: programTreasury,
      createKey,
      creator,
      multisigPda,
      configAuthority: null,
      timeLock: 0,
      threshold: 2,
      members: [
        { key: members.almighty.publicKey, permissions: Permissions.all() },
        {
          key: members.proposer.publicKey,
          permissions: Permissions.fromPermissions([Permission.Initiate]),
        },
        {
          key: members.voter.publicKey,
          permissions: Permissions.fromPermissions([Permission.Vote]),
        },
        {
          key: members.executor.publicKey,
          permissions: Permissions.fromPermissions([Permission.Execute]),
        },
      ],
      rentCollector: null,
      programId,
      sendOptions: { skipPreflight: true },
      lightAccounts,
      lightArgs,
      remainingAccounts
    });
    await connection.confirmTransaction(signature);

    const creatorBalancePost = await connection.getBalance(creator.publicKey);
    const rentAndNetworkFee = 15692;

    assert.strictEqual(
      creatorBalancePost,
      creatorBalancePre - rentAndNetworkFee - multisigCreationFee
    );

    //region Reset the global multisig creation fee
    const resetCreationFeeIx = multisig.generated.createProgramConfigSetMultisigCreationFeeInstruction(
      {
        programConfig: programConfigPda,
        authority: programConfigAuthority.publicKey,
      },
      {
        args: { newMultisigCreationFee: 0 },
      },
      programId
    );

    const message2 = new TransactionMessage({
      recentBlockhash: (await connection.getLatestBlockhash()).blockhash,
      payerKey: programConfigAuthority.publicKey,
      instructions: [resetCreationFeeIx],
    }).compileToV0Message();

    const tx2 = new VersionedTransaction(message2);
    tx2.sign([programConfigAuthority]);
    signature = await connection.sendTransaction(tx2);
    await connection.confirmTransaction(signature);

    programConfig = await multisig.accounts.ProgramConfig.fromAccountAddress(
      connection,
      programConfigPda
    );
    assert.strictEqual(programConfig.multisigCreationFee.toString(), "0");
  });
});