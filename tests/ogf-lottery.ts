import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { createAssociatedTokenAccount, createAssociatedTokenAccountIdempotent, createAssociatedTokenAccountIdempotentInstruction, createMint, getAccount, getAssociatedTokenAddressSync, mintTo } from "@solana/spl-token";
import { OgfLottery } from "../target/types/ogf_lottery";
import { PublicKey, Keypair, Transaction } from "@solana/web3.js";
import { BN } from "bn.js";
import { assert } from "chai";
const DECIMALS = 6;
describe("ogf-lottery", () => {
  // Configure the client to use the local cluster.
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const wallet = provider.wallet as anchor.Wallet;
  const program = anchor.workspace.OgfLottery as Program<OgfLottery>;
  const createToken = async () => {
    const mint = await createMint(
      provider.connection,
      wallet.payer,
      wallet.publicKey,
      wallet.publicKey,
      DECIMALS
    );
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
  it("Is initialized!", async () => {
    // Add your test here.
    const { mint } = await createToken();
    tokenMint = mint;

    const tx = await program.methods.initialize().accounts({
      signer: wallet.publicKey,
    }).rpc();
    console.log(tx)
    const tx2 = await program.methods.initialize2().accounts({
      signer: wallet.publicKey,
      mint,
    }).rpc();
    console.log(tx2)
    console.log(tx, tx2);
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
    assert.deepStrictEqual(bidAccount.bidIds, [0], "Bid Ids wrong");
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
    assert.deepStrictEqual(bidAccount2.bidIds, [0, 1, 2, 3]);
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
    await new Promise(resolve => setTimeout(resolve, 1000));
    assert(signerTokenAccount.amount > BigInt(0), "Signer token account did not receive any token");
  });
});
