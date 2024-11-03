const { Keypair } = require("@solana/web3.js");
const { readFileSync } = require("fs");
const path = require("path");

const PROGRAM_NAME = "squads_multisig_program";

const programDir = path.join(__dirname, "..", "..", "programs", PROGRAM_NAME);
const idlDir = path.join(__dirname, "idl");
const sdkDir = path.join(__dirname, "src", "generated");
const binaryInstallDir = path.join(__dirname, "..", "..", ".crates");

const ignoredTypes = new Set([
	// Exclude `Permission` enum from the IDL because it is not correctly represented there.
	"Permission",
	// Exclude the types that use `SmallVec` because anchor doesn't have it in the IDL.
	"TransactionMessage",
	"CompiledInstruction",
	"MessageAddressTableLookup",
	// We need it as an account, so we're ignoring it as a type and pushing it
	// manually in the idl hook
	"LightMultisig"
]);

module.exports = {
	idlGenerator: "anchor",
	programName: PROGRAM_NAME,
	programId: "SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf",
	idlDir,
	sdkDir,
	binaryInstallDir,
	programDir,
	idlHook: idl => {
		// Add OptionPubkey type definition
		idl.types.push({
			name: "OptionPubkey",
			type: {
				kind: "struct",
				fields: [
					{
						name: "value",
						type: {
							option: "publicKey",
						},
					},
				],
			},
		});

		idl.types.push({
			name: "MemberList",
			type: {
				kind: "struct",
				fields: [
					{
						name: "value",
						type: {
							vec: {
								defined: "Member",
							},
						},
					},
				],
			},
		});

		idl.accounts.push({
			name: "LightMultisig",
			type: {
				kind: "struct",
				fields: [
					{
						name: "createKey",
						type: "publicKey",
					},
					{
						name: "configAuthority",
						type: "publicKey",
					},
					{
						name: "threshold",
						type: "u16",
					},
					{
						name: "timeLock",
						type: "u32",
					},
					{
						name: "transactionIndex",
						type: "u64",
					},
					{
						name: "staleTransactionIndex",
						type: "u64",
					},
					{
						name: "rentCollector",
						type: {
							defined: "OptionPubkey",
						},
					},
					{
						name: "bump",
						type: "u8",
					},
					{
						name: "members",
						type: {
							defined: "MemberList",
						},
					},
				],
			},
		});

		return {
			...idl,
			types: idl.types.filter(type => {
				return !ignoredTypes.has(type.name);
			}),
		};
	},
};
