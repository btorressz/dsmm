use anchor_lang::prelude::*;
use anchor_spl::token::{self, TokenAccount, Token, Transfer};

declare_id!("6RqtyVQkmD2ECJ95MAB5QejNhDcWd5b63bkHxedE9BWS");

/// Minimum stake duration in seconds (7 days)
const MIN_STAKE_DURATION: i64 = 604800;

#[program]
pub mod dsmm {
    use super::*;

    /// Initializes the pool state with configuration parameters and new fields.
    pub fn initialize_pool(
        ctx: Context<InitializePool>, 
        bump: u8, 
        token_mint: Pubkey, 
        maker_fee_rate: u16, 
        taker_fee_rate: u16
    ) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        pool.bump = bump;
        pool.total_staked = 0;
        pool.total_rewards = 0;
        pool.total_weighted_stake = 0;
        pool.token_mint = token_mint;
        pool.maker_fee_rate = maker_fee_rate;
        pool.taker_fee_rate = taker_fee_rate;
        pool.admin = *ctx.accounts.admin.key;
        pool.impermanent_loss_protection_fund = 0;
        pool.is_emergency = false;
        Ok(())
    }

    /// Stake tokens into the pool.
    pub fn stake(ctx: Context<Stake>, amount: u64) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        let staker = &mut ctx.accounts.staker;
        let clock = Clock::get()?;

        // Ensure the token mint of the user's account matches the pool's token mint.
        require!(
            ctx.accounts.user_token_account.mint == pool.token_mint,
            CustomError::InvalidTokenMint
        );

        // Transfer tokens from the user to the pool vault.
        let cpi_accounts = Transfer {
            from: ctx.accounts.user_token_account.to_account_info().clone(),
            to: ctx.accounts.pool_vault.to_account_info().clone(),
            authority: ctx.accounts.owner.to_account_info().clone(),
        };
        let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts);
        token::transfer(cpi_ctx, amount)?;

        // Set deposit timestamp on first stake.
        if staker.amount == 0 {
            staker.deposit_timestamp = clock.unix_timestamp;
        }

        // Compute and update weighted stake.
        {
            let previous_weight = calculate_staker_weight(staker, clock.unix_timestamp);
            staker.amount = staker.amount.checked_add(amount).unwrap();
            let new_weight = calculate_staker_weight(staker, clock.unix_timestamp);
            ctx.accounts.pool.total_weighted_stake = ctx.accounts.pool
                .total_weighted_stake
                .checked_add(new_weight.checked_sub(previous_weight).unwrap())
                .unwrap();
        }
        ctx.accounts.pool.total_staked = ctx.accounts.pool.total_staked.checked_add(amount).unwrap();

        Ok(())
    }

    /// Withdraw staked tokens from the pool.
    pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        let staker = &mut ctx.accounts.staker;
        let clock = Clock::get()?;

        // Enforce the minimum staking duration.
        require!(
            clock.unix_timestamp >= staker.deposit_timestamp + MIN_STAKE_DURATION,
            CustomError::StakeTimeNotReached
        );
        require!(staker.amount >= amount, CustomError::InsufficientStake);

        // Compute and update weighted stake.
        {
            let previous_weight = calculate_staker_weight(staker, clock.unix_timestamp);
            staker.amount = staker.amount.checked_sub(amount).unwrap();
            let new_weight = calculate_staker_weight(staker, clock.unix_timestamp);
            ctx.accounts.pool.total_weighted_stake = ctx.accounts.pool
                .total_weighted_stake
                .checked_sub(previous_weight.checked_sub(new_weight).unwrap())
                .unwrap();
        }
        ctx.accounts.pool.total_staked = ctx.accounts.pool.total_staked.checked_sub(amount).unwrap();

        // Bind pool key to a variable for longer lifetime.
        let pool_key = ctx.accounts.pool.key();
        let seeds = &[pool_key.as_ref(), &[ctx.accounts.pool.bump]];
        let signer = &[&seeds[..]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.pool_vault.to_account_info().clone(),
            to: ctx.accounts.user_token_account.to_account_info().clone(),
            authority: ctx.accounts.pool.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer,
        );
        token::transfer(cpi_ctx, amount)?;

        Ok(())
    }

    /// Record profits from HFT market-making strategies.
    pub fn record_trade_profit(ctx: Context<RecordProfit>, profit: u64) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        pool.total_rewards = pool.total_rewards.checked_add(profit).unwrap();
        Ok(())
    }

    /// Distribute rewards to a staker based on their weighted stake.
    pub fn distribute_rewards(ctx: Context<DistributeRewards>) -> Result<()> {
        let clock = Clock::get()?;

        // Isolate mutable borrows to compute reward_amount.
        let reward_amount: u64 = {
            let pool = &mut ctx.accounts.pool;
            let staker = &mut ctx.accounts.staker;
            require!(pool.total_rewards > 0, CustomError::NoRewardsAvailable);
            require!(pool.total_weighted_stake > 0, CustomError::NoStakedFunds);

            let weighted_stake = calculate_staker_weight(staker, clock.unix_timestamp);
            let reward_share = (weighted_stake as u128)
                .checked_mul(pool.total_rewards as u128)
                .unwrap()
                .checked_div(pool.total_weighted_stake as u128)
                .unwrap();
            reward_share as u64
        }; // Mutable borrows end here.

        // Bind pool key to a variable for longer lifetime.
        let pool_key = ctx.accounts.pool.key();
        let seeds = &[pool_key.as_ref(), &[ctx.accounts.pool.bump]];
        let signer = &[&seeds[..]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.pool_vault.to_account_info(),
            to: ctx.accounts.staker_token_account.to_account_info(),
            authority: ctx.accounts.pool.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer,
        );
        token::transfer(cpi_ctx, reward_amount)?;

        // Reborrow mutable pool to update rewards.
        {
            let pool = &mut ctx.accounts.pool;
            pool.total_rewards = pool.total_rewards.checked_sub(reward_amount).unwrap();
        }

        Ok(())
    }

    /// Auto-compound rewards by adding them to the staker's principal.
    pub fn auto_compound_rewards(ctx: Context<AutoCompoundRewards>) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        let staker = &mut ctx.accounts.staker;
        require!(pool.total_rewards > 0, CustomError::NoRewardsAvailable);
        require!(pool.total_staked > 0, CustomError::NoStakedFunds);

        let reward_share = (staker.amount as u128)
            .checked_mul(pool.total_rewards as u128)
            .unwrap()
            .checked_div(pool.total_staked as u128)
            .unwrap();
        let reward_amount = reward_share as u64;

        staker.amount = staker.amount.checked_add(reward_amount).unwrap();
        pool.total_rewards = pool.total_rewards.checked_sub(reward_amount).unwrap();
        pool.total_staked = pool.total_staked.checked_add(reward_amount).unwrap();

        Ok(())
    }

    /// Governance function to update the fee structure.
    pub fn update_fee_structure(
        ctx: Context<UpdateFeeStructure>, 
        new_maker_fee: u16, 
        new_taker_fee: u16
    ) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        require!(ctx.accounts.admin.key() == pool.admin, CustomError::Unauthorized);

        pool.maker_fee_rate = new_maker_fee;
        pool.taker_fee_rate = new_taker_fee;
        Ok(())
    }

    /// Adjust fee structure based on performance metrics.
    pub fn adjust_fee_based_on_performance(
        ctx: Context<AdjustFeePerformance>, 
        new_maker_fee: u16, 
        new_taker_fee: u16
    ) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        require!(ctx.accounts.admin.key() == pool.admin, CustomError::Unauthorized);
        require!(new_maker_fee <= 1000, CustomError::InvalidFeeRate);
        require!(new_taker_fee <= 1000, CustomError::InvalidFeeRate);

        pool.maker_fee_rate = new_maker_fee;
        pool.taker_fee_rate = new_taker_fee;

        Ok(())
    }

    /// Compensate LPs for impermanent loss using the protection fund.
    pub fn compensate_lp_losses(ctx: Context<CompensateLP>, loss_amount: u64) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        require!(pool.impermanent_loss_protection_fund >= loss_amount, CustomError::NotEnoughFunds);

        pool.impermanent_loss_protection_fund = pool
            .impermanent_loss_protection_fund
            .checked_sub(loss_amount)
            .unwrap();

        Ok(())
    }

    /// Emergency withdrawal function callable only when emergency mode is activated.
    pub fn emergency_withdraw(ctx: Context<EmergencyWithdraw>) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        require!(pool.is_emergency, CustomError::EmergencyNotActivated);
        let staker = &mut ctx.accounts.staker;
        let amount = staker.amount;
        staker.amount = 0;
        pool.total_staked = pool.total_staked.checked_sub(amount).unwrap();

        Ok(())
    }

    /// Prevent flash loan exploits by requiring a minimum stake duration (e.g., 10 minutes).
    pub fn prevent_flash_loans(ctx: Context<PreventFlashLoan>) -> Result<()> {
        let staker = &ctx.accounts.staker;
        let clock = Clock::get()?;
        require!(
            clock.unix_timestamp >= staker.deposit_timestamp + 600, // 10 minutes
            CustomError::FlashLoanDetected
        );
        Ok(())
    }

    /// Multi-signature governance action.
    /// This function expects at least 2 admin signatures (passed as remaining accounts).
    pub fn execute_governance_action(ctx: Context<GovernanceAction>, proposal_id: u64) -> Result<()> {
        let _governance = &ctx.accounts.governance;
        let signatures = &ctx.remaining_accounts;
        require!(signatures.len() >= 2, CustomError::NotEnoughSignatures);

        // Insert logic to execute the governance-approved proposal identified by proposal_id.
        Ok(())
    }

    /// Allocate funds from the treasury for protocol spending.
    pub fn allocate_treasury_funds(ctx: Context<AllocateFunds>, amount: u64) -> Result<()> {
        let treasury = &mut ctx.accounts.treasury;
        require!(treasury.collected_fees >= amount, CustomError::NotEnoughFunds);

        treasury.collected_fees = treasury.collected_fees.checked_sub(amount).unwrap();
        Ok(())
    }
}

/// Calculates the weighted stake based on deposit duration.
/// Longer staked amounts receive higher weight.
fn calculate_staker_weight(staker: &Staker, current_time: i64) -> u64 {
    let duration = current_time - staker.deposit_timestamp;
    if duration > 31536000 {
        // More than 1 year: 2x weight.
        staker.amount * 2
    } else if duration > 15768000 {
        // More than 6 months: 1.5x weight.
        staker.amount * 15 / 10
    } else {
        staker.amount
    }
}

#[derive(Accounts)]
pub struct InitializePool<'info> {
    #[account(init, payer = admin, space = 8 + Pool::LEN)]
    pub pool: Account<'info, Pool>,
    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Stake<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,
    /// The staker account is derived from the owner's key and pool address.
    #[account(init_if_needed, payer = owner, space = 8 + Staker::LEN, seeds = [owner.key.as_ref(), pool.key().as_ref()], bump)]
    pub staker: Account<'info, Staker>,
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub pool_vault: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,
    #[account(mut, seeds = [owner.key.as_ref(), pool.key().as_ref()], bump)]
    pub staker: Account<'info, Staker>,
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub pool_vault: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct RecordProfit<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,
}

#[derive(Accounts)]
pub struct DistributeRewards<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,
    /// The staker account is derived from the staker's public key and the pool.
    #[account(mut, seeds = [staker.owner.as_ref(), pool.key().as_ref()], bump)]
    pub staker: Account<'info, Staker>,
    #[account(mut)]
    pub pool_vault: Account<'info, TokenAccount>,
    #[account(mut)]
    pub staker_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct AutoCompoundRewards<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,
    /// The staker account is derived from the staker's public key and the pool.
    #[account(mut, seeds = [staker.owner.as_ref(), pool.key().as_ref()], bump)]
    pub staker: Account<'info, Staker>,
}

#[derive(Accounts)]
pub struct UpdateFeeStructure<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,
    #[account(mut)]
    pub admin: Signer<'info>,
}

#[derive(Accounts)]
pub struct AdjustFeePerformance<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,
    #[account(mut)]
    pub admin: Signer<'info>,
}

#[derive(Accounts)]
pub struct CompensateLP<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,
}

#[derive(Accounts)]
pub struct EmergencyWithdraw<'info> {
    #[account(mut)]
    pub pool: Account<'info, Pool>,
    /// The staker account is derived from the staker's public key and the pool.
    #[account(mut, seeds = [staker.owner.as_ref(), pool.key().as_ref()], bump)]
    pub staker: Account<'info, Staker>,
}

#[derive(Accounts)]
pub struct PreventFlashLoan<'info> {
    pub staker: Account<'info, Staker>,
}

#[derive(Accounts)]
pub struct GovernanceAction<'info> {
    #[account(mut)]
    pub governance: Account<'info, Governance>,
    // Additional admin signature accounts should be passed as remaining accounts.
}

#[derive(Accounts)]
pub struct AllocateFunds<'info> {
    #[account(mut)]
    pub treasury: Account<'info, Treasury>,
}

/// Pool account to track overall staked amounts, rewards, fees, and additional fields.
#[account]
pub struct Pool {
    pub bump: u8,
    pub total_staked: u64,
    pub total_rewards: u64,
    pub total_weighted_stake: u64, // NEW: Total weighted stake of all stakers.
    pub token_mint: Pubkey,
    pub maker_fee_rate: u16,
    pub taker_fee_rate: u16,
    pub admin: Pubkey,
    pub impermanent_loss_protection_fund: u64,
    pub is_emergency: bool,
}

impl Pool {
    // Calculated space: 1 + 8 + 8 + 8 + 32 + 2 + 2 + 32 + 8 + 1 = 102 bytes.
    const LEN: usize = 102;
}

/// Staker account to track an individual staker’s deposit and timestamp.
#[account]
pub struct Staker {
    pub owner: Pubkey,
    pub amount: u64,
    pub deposit_timestamp: i64,
}

impl Staker {
    // Calculated space: 32 + 8 + 8 = 48 bytes.
    const LEN: usize = 48;
}

/// Governance account for multi-signature actions.
#[account]
pub struct Governance {
    pub admin_1: Pubkey,
    pub admin_2: Pubkey,
    pub admin_3: Pubkey,
}

/// Treasury account for managing collected fees.
#[account]
pub struct Treasury {
    pub collected_fees: u64,
}

#[error_code]
pub enum CustomError {
    #[msg("Insufficient stake for withdrawal")]
    InsufficientStake,
    #[msg("No rewards available to distribute")]
    NoRewardsAvailable,
    #[msg("No staked funds available")]
    NoStakedFunds,
    #[msg("Stake duration has not been reached for withdrawal")]
    StakeTimeNotReached,
    #[msg("Unauthorized action")]
    Unauthorized,
    #[msg("Invalid token mint for this pool")]
    InvalidTokenMint,
    #[msg("Invalid fee rate provided")]
    InvalidFeeRate,
    #[msg("Not enough funds available")]
    NotEnoughFunds,
    #[msg("Emergency state not activated")]
    EmergencyNotActivated,
    #[msg("Flash loan detected")]
    FlashLoanDetected,
    #[msg("Not enough signatures provided")]
    NotEnoughSignatures,
}
