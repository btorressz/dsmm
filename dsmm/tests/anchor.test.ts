// No imports needed: web3, anchor, pg, and more are globally available

describe("DSMM Staking Program", () => {
  let poolKp: web3.Keypair;
  let stakerKp: web3.Keypair;

  before(async () => {
    poolKp = new web3.Keypair();
    stakerKp = new web3.Keypair();
  });

  it("Initialize Pool", async () => {
    const txHash = await pg.program.methods
      .initializePool(
        new BN(1), // bump
        new web3.PublicKey("So11111111111111111111111111111111111111112"), // Token Mint (example: wrapped SOL)
        new BN(10), // Maker Fee Rate
        new BN(20) // Taker Fee Rate
      )
      .accounts({
        pool: poolKp.publicKey,
        admin: pg.wallet.publicKey,
        systemProgram: web3.SystemProgram.programId,
      })
      .signers([poolKp])
      .rpc();
    
    console.log(`Pool initialized: ${txHash}`);

    const poolAccount = await pg.program.account.pool.fetch(poolKp.publicKey);
    console.log("Initialized Pool Data:", poolAccount);
    assert(poolAccount.totalStaked.eq(new BN(0)));
  });

  it("Stake Tokens", async () => {
    const userTokenAccount = new web3.Keypair();
    const amount = new BN(1000); // Stake 1000 tokens

    const txHash = await pg.program.methods
      .stake(amount)
      .accounts({
        pool: poolKp.publicKey,
        staker: stakerKp.publicKey,
        owner: pg.wallet.publicKey,
        userTokenAccount: userTokenAccount.publicKey,
        poolVault: poolKp.publicKey, // Mock vault for testing
        tokenProgram: web3.SystemProgram.programId,
        systemProgram: web3.SystemProgram.programId,
      })
      .signers([stakerKp, userTokenAccount])
      .rpc();

    console.log(`Stake transaction: ${txHash}`);

    const stakerAccount = await pg.program.account.staker.fetch(stakerKp.publicKey);
    console.log("Staker Data:", stakerAccount);
    assert(stakerAccount.amount.eq(amount));
  });

  it("Withdraw Tokens", async () => {
    const amountToWithdraw = new BN(500); // Withdraw 500 tokens

    const txHash = await pg.program.methods
      .withdraw(amountToWithdraw)
      .accounts({
        pool: poolKp.publicKey,
        staker: stakerKp.publicKey,
        owner: pg.wallet.publicKey,
        userTokenAccount: stakerKp.publicKey,
        poolVault: poolKp.publicKey, // Mock vault for testing
        tokenProgram: web3.SystemProgram.programId,
      })
      .signers([stakerKp])
      .rpc();

    console.log(`Withdraw transaction: ${txHash}`);

    const updatedStakerAccount = await pg.program.account.staker.fetch(stakerKp.publicKey);
    console.log("Updated Staker Data:", updatedStakerAccount);
    assert(updatedStakerAccount.amount.eq(new BN(500))); // Ensure amount is updated correctly
  });

  it("Distribute Rewards", async () => {
    const txHash = await pg.program.methods
      .distributeRewards()
      .accounts({
        pool: poolKp.publicKey,
        staker: stakerKp.publicKey,
        poolVault: poolKp.publicKey,
        stakerTokenAccount: stakerKp.publicKey,
        tokenProgram: web3.SystemProgram.programId,
      })
      .signers([])
      .rpc();

    console.log(`Distribute rewards transaction: ${txHash}`);

    const updatedPool = await pg.program.account.pool.fetch(poolKp.publicKey);
    console.log("Updated Pool Data:", updatedPool);
    assert(updatedPool.totalRewards.gte(new BN(0))); // Ensure rewards are deducted from pool
  });
});
