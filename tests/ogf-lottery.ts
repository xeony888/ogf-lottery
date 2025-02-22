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
    await mintTo(
      provider.connection,
      wallet.payer,
      mint,
      tokenAccount,
      wallet.payer,
      100000 * 10 ** DECIMALS
    );
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
    await program.methods.initialize().accounts({
      signer: wallet.publicKey,
      mint,
    }).rpc();
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
    const bid = await program.methods.bid(1, 0).accounts({
      signer: wallet.publicKey
    }).transaction();
    const transaction = new Transaction().add(newPool, release, bid);
    await provider.sendAndConfirm(transaction);
    const [poolAddress] = PublicKey.findProgramAddressSync(
      [Buffer.from("pool"), new BN(1).toArrayLike(Buffer, "le", 2)],
      program.programId
    );
    const [bidAddress] = PublicKey.findProgramAddressSync(
      [Buffer.from("bid"), new BN(1).toArrayLike(Buffer, "le", 2), new BN(0).toArrayLike(Buffer, "le", 2)],
      program.programId
    );
    let poolAccount = await program.account.pool.fetch(poolAddress);
    let bidAccount = await program.account.bidder.fetch(bidAddress);
    assert(bidAccount.bidder.equals(wallet.publicKey), "Invalid bidder public key");
    assert(poolAccount.id === 1, "Pool account id incorrect")
    assert(poolAccount.bidDeadline.gt(new BN(0)), "Bid deadline not greater than 0");
    assert(poolAccount.bids === 1, "Bids not equal to 1")
    assert(poolAccount.balance.gt(new BN(0)), "Released balance not greater than 0");
    assert(poolAccount.releases.gt(new BN(0)), "Releases not equal to 1");
    await program.methods.bid(1, 1).accounts({
      signer: wallet.publicKey
    }).rpc()
    poolAccount = await program.account.pool.fetch(poolAddress);
    assert(poolAccount.bids === 2, "Bids not equal to 2");
    await new Promise(resolve => setTimeout(resolve, 6 * 1000)); // wait 11 seconds
    try {
      await program.methods.bid(1, 2).accounts({
        signer: wallet.publicKey
      }).rpc();
      assert(false);
    } catch (e) {
      if (e.name === "AssertionError") {
        throw new Error("Did not fail when bidding after bid deadline");
      }
    }
    const newPool1 = await program.methods.newPool(2).accounts({
      signer: wallet.publicKey,
      prevPool: poolAddress,
    }).transaction();
    const release1 = await program.methods.release(2).accounts({
      signer: wallet.publicKey
    }).transaction();
    const bid1 = await program.methods.bid(2, 0).accounts({
      signer: wallet.publicKey
    }).transaction();
    const transaction1 = new Transaction().add(newPool1, release1, bid1);
    await provider.sendAndConfirm(transaction1);
    const [pool1Address] = PublicKey.findProgramAddressSync(
      [Buffer.from("pool"), new BN(2).toArrayLike(Buffer, "le", 2)],
      program.programId
    );
    let pool1Data = await program.account.pool.fetch(pool1Address);
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
    const tx1 = await program.methods.claim(1, 1).accounts({
      signer: wallet.publicKey,
      signerTokenAccount: signerTokenAccountAddress,
    }).transaction();
    const transaction2 = new Transaction().add(ix, tx, tx1);
    await provider.sendAndConfirm(transaction2);
    const signerTokenAccount = await getAccount(provider.connection, signerTokenAccountAddress);
    await new Promise(resolve => setTimeout(resolve, 1000));
    assert(signerTokenAccount.amount > BigInt(0), "Signer token account did not receive any token");
  });
});
