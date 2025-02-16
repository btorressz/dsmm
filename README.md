# dsmm

# 🏦 Dynamic Staking for Market-Making (DSMM)

**DSMM** is a **Solana-based staking pool** that provides capital to high-frequency traders (HFTs) for **market-making strategies**. Users stake **USDC, SOL, or SPL tokens**, and their funds are used to **provide deep liquidity** on Solana DEXes. In return, stakers receive **dynamic yield** based on market-maker performance.

---

**devnet:** (https://explorer.solana.com/address/6RqtyVQkmD2ECJ95MAB5QejNhDcWd5b63bkHxedE9BWS?cluster=devnet)

## 🚀 Features

✅ **Optimized for HFT Strategies** – Funds are allocated to professional traders for market-making.  
✅ **Ensures Deep Liquidity** – Increases liquidity depth for Solana's **AMM** & **order book DEXes**.  
✅ **Revenue from Trading Spreads & Maker Rebates** – Stakers earn a share of the profits.  
✅ **Auto-Compounding Rewards** – Stakers can choose to automatically reinvest rewards.  
✅ **Governance & Fee Adjustments** – Admins can adjust fees based on market performance.  
✅ **Impermanent Loss Protection** – A fund is maintained to protect LPs.  
✅ **Multi-Signature Governance** – Ensures security for major protocol updates.  

---

## 📜 Smart Contract Overview

### **Main Instructions**
| Function | Description |
|----------|------------|
| `initialize_pool` | Creates the staking pool with admin-defined parameters. |
| `stake` | Users deposit funds into the pool. |
| `withdraw` | Users withdraw staked funds (after minimum staking duration). |
| `record_trade_profit` | Market-makers submit their trading profits to the pool. |
| `distribute_rewards` | Distributes rewards based on a weighted staking model. |
| `auto_compound_rewards` | Allows stakers to automatically reinvest their rewards. |
| `update_fee_structure` | Adjusts maker/taker fee rates via governance. |
| `adjust_fee_based_on_performance` | Dynamically updates fees based on market conditions. |
| `compensate_lp_losses` | Uses an impermanent loss protection fund to compensate LPs. |
| `emergency_withdraw` | Enables users to withdraw in case of emergencies. |
| `prevent_flash_loans` | Prevents flash loan exploits by enforcing a minimum staking duration. |
| `execute_governance_action` | Executes multi-signature governance proposals. |

---

