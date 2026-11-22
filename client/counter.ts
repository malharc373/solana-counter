// Minimal @solana/kit client for the counter program.
// Usage: npm run counter -- <show|init|inc|dec|close>
// Env: RPC_URL (default devnet), KEYPAIR (default ~/.config/solana/id.json)

import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import {
  AccountRole,
  address,
  appendTransactionMessageInstruction,
  createKeyPairSignerFromBytes,
  createSolanaRpc,
  createSolanaRpcSubscriptions,
  createTransactionMessage,
  fetchEncodedAccount,
  getAddressDecoder,
  getAddressEncoder,
  getProgramDerivedAddress,
  getSignatureFromTransaction,
  getU64Decoder,
  pipe,
  sendAndConfirmTransactionFactory,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  signTransactionMessageWithSigners,
  type Address,
  type KeyPairSigner,
} from "@solana/kit";

const PROGRAM_ID = address("HoBHp5ZXWSwcVP7BH15K1qnn7Zn8mhgTZSEYUJLmwmeW");
const SYSTEM_PROGRAM = address("11111111111111111111111111111111");
const RPC_URL = process.env.RPC_URL ?? "https://api.devnet.solana.com";
const WS_URL = RPC_URL.replace(/^http/, "ws");

// Anchor discriminators = first 8 bytes of sha256("global:<ix_name>"), copied from target/idl/counter.json
const IX = {
  init: new Uint8Array([175, 175, 109, 31, 13, 152, 155, 237]),
  inc: new Uint8Array([11, 18, 104, 9, 104, 174, 59, 33]),
  dec: new Uint8Array([106, 227, 168, 59, 248, 27, 150, 101]),
  close: new Uint8Array([98, 165, 201, 177, 108, 65, 206, 96]),
};

const rpc = createSolanaRpc(RPC_URL);
const rpcSubscriptions = createSolanaRpcSubscriptions(WS_URL);
const sendAndConfirm = sendAndConfirmTransactionFactory({ rpc, rpcSubscriptions });

async function loadSigner(): Promise<KeyPairSigner> {
  const path = process.env.KEYPAIR ?? `${homedir()}/.config/solana/id.json`;
  const bytes = new Uint8Array(JSON.parse(readFileSync(path, "utf8")));
  return createKeyPairSignerFromBytes(bytes);
}

async function counterPda(authority: Address): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: PROGRAM_ID,
    seeds: ["counter", getAddressEncoder().encode(authority)],
  });
  return pda;
}

async function show(authority: Address) {
  const pda = await counterPda(authority);
  const acc = await fetchEncodedAccount(rpc, pda);
  if (!acc.exists) {
    console.log(`No counter yet at ${pda}. Run: npm run counter -- init`);
    return;
  }
  // Layout: 8-byte discriminator | authority (32) | count u64 (8) | bump u8 (1)
  const d = acc.data;
  const owner = getAddressDecoder().decode(d.slice(8, 40));
  const count = getU64Decoder().decode(d.slice(40, 48));
  console.log(`counter ${pda}\n  authority ${owner}\n  count     ${count}\n  bump      ${d[48]}\n  rent      ${acc.lamports} lamports`);
}

async function send(signer: KeyPairSigner, kind: keyof typeof IX) {
  const pda = await counterPda(signer.address);
  const accounts =
    kind === "init"
      ? [
          { address: signer.address, role: AccountRole.WRITABLE_SIGNER, signer },
          { address: pda, role: AccountRole.WRITABLE },
          { address: SYSTEM_PROGRAM, role: AccountRole.READONLY },
        ]
      : [
          { address: pda, role: AccountRole.WRITABLE },
          {
            address: signer.address,
            role: kind === "close" ? AccountRole.WRITABLE_SIGNER : AccountRole.READONLY_SIGNER,
            signer,
          },
        ];

  const { value: blockhash } = await rpc.getLatestBlockhash().send();
  const message = pipe(
    createTransactionMessage({ version: 0 }),
    (m) => setTransactionMessageFeePayerSigner(signer, m),
    (m) => setTransactionMessageLifetimeUsingBlockhash(blockhash, m),
    (m) => appendTransactionMessageInstruction({ programAddress: PROGRAM_ID, accounts, data: IX[kind] }, m),
  );
  const tx = await signTransactionMessageWithSigners(message);
  await sendAndConfirm(tx, { commitment: "confirmed" });
  const sig = getSignatureFromTransaction(tx);
  const cluster = RPC_URL.includes("devnet") ? "?cluster=devnet" : "";
  console.log(`${kind} ✓ https://explorer.solana.com/tx/${sig}${cluster}`);
}

const cmd = process.argv[2] ?? "show";
const signer = await loadSigner();
if (cmd === "show") {
  await show(signer.address);
} else if (cmd in IX) {
  await send(signer, cmd as keyof typeof IX);
  if (cmd !== "close") await show(signer.address);
} else {
  console.error("usage: npm run counter -- <show|init|inc|dec|close>");
  process.exit(1);
}
