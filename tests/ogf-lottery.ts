import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { createAssociatedTokenAccount, createAssociatedTokenAccountIdempotent, createAssociatedTokenAccountIdempotentInstruction, createMint, getAccount, getAssociatedTokenAddressSync, mintTo } from "@solana/spl-token";
import { OgfLottery } from "../target/types/ogf_lottery";
import { PublicKey, Keypair, Transaction, SystemProgram } from "@solana/web3.js";
import { BN } from "bn.js";
import { assert } from "chai";
const DECIMALS = 6;
// Real OGF mint; Anchor.toml preloads it into the test validator from tests/fixtures/ogf_mint.json
// with the admin wallet as mint authority.
const OGF_MINT = new PublicKey("EQyRaajDZLEEdSrU8Hws29LWjDJczGKB1CV6jrWcZJn9");
describe("ogf-lottery", () => {
  // Configure the client to use the local cluster.
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const wallet = provider.wallet as anchor.Wallet;
  const program = anchor.workspace.OgfLottery as Program<OgfLottery>;
  const expectProgramError = async (promise: Promise<unknown>, code: string) => {
    try {
      await promise;
    } catch (e) {
      assert.instanceOf(e, anchor.AnchorError, `expected AnchorError ${code}, got ${e}`);
      assert.equal((e as anchor.AnchorError).error.errorCode.code, code);
      return;
    }
    assert.fail(`expected ${code} but transaction succeeded`);
  };
  const createToken = async () => {
    const mint = OGF_MINT;
    const tokenAccount = await createAssociatedTokenAccount(
      provider.connection,
      wallet.payer,
      mint,
      wallet.publicKey
    );
    const tokenAccount2 = await createAssociatedTokenAccount(
      provider.connection,
      wallet.payer,
      mint,
      new PublicKey("FUcoeKT9Nod5mWxDJJrbq4SycLAqNyxe5eMnmChbZ89p")
    )
    await mintTo(
      provider.connection,
      wallet.payer,
      mint,
      tokenAccount,
      wallet.payer,
      100000 * 10 ** DECIMALS
    );
    await mintTo(
      provider.connection,
      wallet.payer,
      mint,
      tokenAccount2,
      wallet.payer,
      10000000 * 10 ** DECIMALS
    )
    return {
      mint,
      tokenAccount
    }
  }
  let tokenMint: PublicKey;
  it("rejects initialization by a non-admin signer", async () => {
    const attacker = Keypair.generate();
    await provider.sendAndConfirm(new Transaction().add(SystemProgram.transfer({
      fromPubkey: wallet.publicKey,
      toPubkey: attacker.publicKey,
      lamports: 1_000_000_000,
    })));
    const [globalAddress] = PublicKey.findProgramAddressSync([Buffer.from("global")], program.programId);
    await expectProgramError(
      program.methods.initialize().accounts({ signer: attacker.publicKey }).signers([attacker]).rpc(),
      "InvalidSigner"
    );
    assert.isNull(await provider.connection.getAccountInfo(globalAddress), "non-admin bound global state");
  });
  it("rejects initialize2 by a non-admin signer and for a wrong mint, then accepts admin with the OGF mint", async () => {
    const { mint } = await createToken();
    tokenMint = mint;
    await program.methods.initialize().accounts({
      signer: wallet.publicKey,
    }).rpc();
    const [globalAddress] = PublicKey.findProgramAddressSync([Buffer.from("global")], program.programId);
    const [programTokenAddress] = PublicKey.findProgramAddressSync([Buffer.from("token")], program.programId);

    const attacker = Keypair.generate();
    await provider.sendAndConfirm(new Transaction().add(SystemProgram.transfer({
      fromPubkey: wallet.publicKey,
      toPubkey: attacker.publicKey,
      lamports: 1_000_000_000,
    })));
    await expectProgramError(
      program.methods.initialize2().accounts({ signer: attacker.publicKey, mint }).signers([attacker]).rpc(),
      "InvalidSigner"
    );
    const wrongMint = await createMint(provider.connection, wallet.payer, wallet.publicKey, null, DECIMALS);
    await expectProgramError(
      program.methods.initialize2().accounts({ signer: wallet.publicKey, mint: wrongMint }).rpc(),
      "WrongMint"
    );
    let globalData = await program.account.globalData.fetch(globalAddress);
    assert(globalData.mint.equals(PublicKey.default), "mint bound despite rejected initialize2");
    assert.isNull(await provider.connection.getAccountInfo(programTokenAddress), "program token account created despite rejection");

    await program.methods.initialize2().accounts({
      signer: wallet.publicKey,
      mint,
    }).rpc();
    globalData = await program.account.globalData.fetch(globalAddress);
    assert(globalData.mint.equals(OGF_MINT), "global mint is not the OGF mint");
    const programTokenAccount = await getAccount(provider.connection, programTokenAddress);
    assert(programTokenAccount.mint.equals(OGF_MINT), "program token account not on the OGF mint");
  });
  it("deposits and withdraws", async () => {
    const signerTokenAccount = getAssociatedTokenAddressSync(tokenMint, wallet.publicKey);
    const [programHolderAccount] = PublicKey.findProgramAddressSync(
      [Buffer.from("token")],
      program.programId
    );
    await program.methods.depositToken(new BN(100000 * 10 ** DECIMALS)).accounts({
      signer: wallet.publicKey,
      signerTokenAccount
    }).rpc();
    const programHolderAccountDataBefore = await getAccount(provider.connection, programHolderAccount);
    assert(programHolderAccountDataBefore.amount === BigInt(100000 * 10 ** DECIMALS));
    await program.methods.withdrawToken(new BN(50000 * 10 ** DECIMALS)).accounts({
      signer: wallet.publicKey,
      signerTokenAccount,
    }).rpc();
    const programHolderAccountDataAfter = await getAccount(provider.connection, programHolderAccount);
    assert(programHolderAccountDataAfter.amount === BigInt(50000 * 10 ** DECIMALS));
    const signerTokenAccountData = await getAccount(provider.connection, signerTokenAccount);
    assert(signerTokenAccountData.amount === BigInt(50000 * 10 ** DECIMALS));
  });
  it("performs pool functionality", async () => {
    const [prevPool] = PublicKey.findProgramAddressSync(
      [Buffer.from("pool"), new BN(0).toArrayLike(Buffer, "le", 2)],
      program.programId
    );
    const newPool = await program.methods.newPool(1).accounts({
      signer: wallet.publicKey,
      prevPool,
    }).transaction();
    const release = await program.methods.release(1).accounts({
      signer: wallet.publicKey
    }).transaction();
    const bidCreate = await program.methods.createBid(1, 0).accounts({
      signer: wallet.publicKey
    }).transaction()
    const bid = await program.methods.bid(1, 0).accounts({
      signer: wallet.publicKey
    }).transaction();
    const transaction = new Transaction().add(newPool, release, bidCreate, bid);
    await provider.sendAndConfirm(transaction);
    const [poolAddress] = PublicKey.findProgramAddressSync(
      [Buffer.from("pool"), new BN(1).toArrayLike(Buffer, "le", 2)],
      program.programId
    );
    const [bidAddress] = PublicKey.findProgramAddressSync(
      [Buffer.from("bid"), new BN(1).toArrayLike(Buffer, "le", 2), new BN(0).toArrayLike(Buffer, "le", 2), wallet.publicKey.toBuffer()],
      program.programId
    );
    console.log({ poolAddress: poolAddress.toString(), bidAddress: bidAddress.toString() })
    let poolAccount = await program.account.pool.fetch(poolAddress);
    let bidAccount = await program.account.bidAccount.fetch(bidAddress);
    assert(bidAccount.bidder.equals(wallet.publicKey), "Invalid bidder public key");
    assert(poolAccount.id === 1, "Pool account id incorrect")
    assert(poolAccount.bidDeadline.gt(new BN(0)), "Bid deadline not greater than 0");
    assert(poolAccount.bids === 1, "Bids not equal to 1")
    assert.deepStrictEqual(bidAccount.bidIds, [1], "Bid Ids wrong");
    assert(poolAccount.balance.gt(new BN(0)), "Released balance not greater than 0");
    await program.methods.bid(1, 0).accounts({
      signer: wallet.publicKey
    }).rpc()
    poolAccount = await program.account.pool.fetch(poolAddress);
    assert(poolAccount.bids === 2, "Bids not equal to 2");
    await new Promise(resolve => setTimeout(resolve, 11000)); // wait 11 seconds
    try {
      await program.methods.bid(1, 0).accounts({
        signer: wallet.publicKey
      }).rpc();
      assert(false);
    } catch (e) {
      if (e.name === "AssertionError") {
        throw new Error("Did not fail when bidding after bid deadline");
      }
    }
    await new Promise(resolve => setTimeout(resolve, 10000));
    console.log({ bidDeadline: poolAccount.bidDeadline.toNumber(), now: Date.now() / 1000 });
    const newPool1 = await program.methods.newPool(2).accounts({
      signer: wallet.publicKey,
      prevPool: poolAddress,
    }).transaction();
    const release1 = await program.methods.release(2).accounts({
      signer: wallet.publicKey
    }).transaction();
    const bidCreate1 = await program.methods.createBid(2, 0).accounts({
      signer: wallet.publicKey
    }).transaction()
    const bid1 = await program.methods.bid(2, 0).accounts({
      signer: wallet.publicKey
    }).transaction();
    const transaction1 = new Transaction().add(newPool1, release1, bidCreate1, bid1);
    await provider.sendAndConfirm(transaction1);
    const bid2 = await program.methods.bid(2, 0).accounts({
      signer: wallet.publicKey
    }).rpc();
    const bid3 = await program.methods.bid(2, 0).accounts({
      signer: wallet.publicKey
    }).rpc();
    const bid4 = await program.methods.bid(2, 0).accounts({
      signer: wallet.publicKey
    }).rpc();
    const [pool1Address] = PublicKey.findProgramAddressSync(
      [Buffer.from("pool"), new BN(2).toArrayLike(Buffer, "le", 2)],
      program.programId
    );
    let pool1Data = await program.account.pool.fetch(pool1Address);
    const [bidAddress2] = PublicKey.findProgramAddressSync(
      [Buffer.from("bid"), new BN(2).toArrayLike(Buffer, "le", 2), new BN(0).toArrayLike(Buffer, "le", 2), wallet.publicKey.toBuffer()],
      program.programId
    );
    const bidAccount2 = await program.account.bidAccount.fetch(bidAddress2);
    assert.deepStrictEqual(bidAccount2.bidIds, [1, 2, 3, 4]);
    assert(pool1Data.id === 2, "Pool id not equal to 2");
    const signerTokenAccountAddress = getAssociatedTokenAddressSync(tokenMint, wallet.publicKey);
    const ix = createAssociatedTokenAccountIdempotentInstruction(
      wallet.publicKey,
      signerTokenAccountAddress,
      wallet.publicKey,
      tokenMint
    );
    const tx = await program.methods.claim(1, 0).accounts({
      signer: wallet.publicKey,
      signerTokenAccount: signerTokenAccountAddress,
    }).transaction();
    const transaction2 = new Transaction().add(ix, tx);
    await provider.sendAndConfirm(transaction2);
    const signerTokenAccount = await getAccount(provider.connection, signerTokenAccountAddress);
    await new Promise(resolve => setTimeout(resolve, 10000));
    await program.methods.newPool(3).accounts({
      signer: wallet.publicKey,
      prevPool: poolAddress,
    }).transaction();
    await program.methods.release(3).accounts({
      signer: wallet.publicKey
    }).transaction();
    await program.methods.claim(2, 0).accounts({
      signer: wallet.publicKey,
      signerTokenAccount: signerTokenAccountAddress
    }).rpc()
    assert(signerTokenAccount.amount > BigInt(0), "Signer token account did not receive any token");
  });
});
