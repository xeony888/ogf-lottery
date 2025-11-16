use anchor_lang::prelude::*;
use anchor_lang::solana_program::native_token::LAMPORTS_PER_SOL;
use anchor_spl::{
    associated_token::spl_associated_token_account::tools::account,
    token::{transfer, Mint, Token, TokenAccount, Transfer},
};

mod utils;
declare_id!("2CHDuw476jJk4oTtrKuA9PSvLsxSHsEQ3sLm3zJvwJsy");
const ADMIN: &str = "6MeJK3erCnwMtsAHLBhRFaXELpzCBfMrrESEJiBWaHTK"; // "oggzGFTgRM61YmhEbgWeivVmQx8bSAdBvsPGqN3ZfxN"; // "6MeJK3erCnwMtsAHLBhRFaXELpzCBfMrrESEJiBWaHTK";
const TEN_DAYS_SECONDS: u64 = 864000;
#[program]
pub mod ogf_lottery {
    use anchor_lang::system_program;

    use super::*;
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        ctx.accounts.global_data_account.release_length = 2; //500;
        ctx.accounts.global_data_account.fee = LAMPORTS_PER_SOL / 1000000;
        ctx.accounts.global_data_account.release_amount = 100000;
        ctx.accounts.global_data_account.max_time_between_bids = 5; // 1000;
        ctx.accounts.global_data_account.total_releases = 0;
        ctx.accounts.global_data_account.claim_expiry_time = TEN_DAYS_SECONDS;
        Ok(())
    }
    pub fn initialize2(ctx: Context<Initialize2>) -> Result<()> {
        ctx.accounts.global_data_account.mint = ctx.accounts.mint.key();
        Ok(())
    }
    pub fn modify_global_data(ctx: Context<ModifyGlobalData>, fee: u64, release_length: u64, max_time_between_bids: u64, release_amount: u64, claim_expiry_time: u64) -> Result<()> {
        if ADMIN.parse::<Pubkey>().unwrap() != ctx.accounts.signer.key() {
            return Err(CustomError::InvalidSigner.into());
        }
        ctx.accounts.global_data_account.fee = fee;
        ctx.accounts.global_data_account.release_length = release_length;
        ctx.accounts.global_data_account.max_time_between_bids = max_time_between_bids;
        ctx.accounts.global_data_account.release_amount = release_amount;
        ctx.accounts.global_data_account.claim_expiry_time = claim_expiry_time;
        Ok(())
    }
    pub fn deposit_token(ctx: Context<DepositToken>, amount: u64) -> Result<()> {
        // if ADMIN.parse::<Pubkey>().unwrap() != ctx.accounts.signer.key() {
        //     return Err(CustomError::InvalidSigner.into());
        // } // anyone can deposit tokens
        transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.signer_token_account.to_account_info(),
                    to: ctx.accounts.program_token_account.to_account_info(),
                    authority: ctx.accounts.signer.to_account_info(),
                },
            ),
            amount,
        )?;
        Ok(())
    }
    pub fn withdraw_token(ctx: Context<WithdrawToken>, amount: u64) -> Result<()> {
        if ADMIN.parse::<Pubkey>().unwrap() != ctx.accounts.signer.key() {
            return Err(CustomError::InvalidSigner.into());
        }
        transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.program_token_account.to_account_info(),
                    to: ctx.accounts.signer_token_account.to_account_info(),
                    authority: ctx.accounts.program_authority.to_account_info(),
                },
                &[&[b"auth", &[ctx.bumps.program_authority]]],
            ),
            amount,
        )?;
        Ok(())
    }
    pub fn withdraw_sol(ctx: Context<WithdrawSol>) -> Result<()> {
        if ADMIN.parse::<Pubkey>().unwrap() != ctx.accounts.signer.key() {
            return Err(CustomError::InvalidSigner.into());
        }
        let min_rent = Rent::get()?.minimum_balance(8) + 20;
        let transfer = ctx.accounts.program_sol_account.get_lamports() - min_rent;
        if transfer <= 0 {
            return Err(CustomError::NoFeesToWithdraw.into());
        }
        **ctx.accounts.program_sol_account.try_borrow_mut_lamports()? -= transfer;
        **ctx.accounts.signer.try_borrow_mut_lamports()? += transfer;
        Ok(())
    }
    pub fn new_pool(ctx: Context<NewPool>, id: u16) -> Result<()> {
        if ctx.accounts.global_data_account.pools + 1 != id {
            return Err(CustomError::InvalidId.into());
        }
        let time: u64 = Clock::get()?.unix_timestamp as u64;
        if time < ctx.accounts.prev_pool.bid_deadline {
            return Err(CustomError::BidDeadlineNotPassed.into());
        }
        ctx.accounts.global_data_account.pools += 1;
        ctx.accounts.pool.id = id;
        let count = time / ctx.accounts.global_data_account.max_time_between_bids;
        ctx.accounts.pool.bid_deadline = (count + 1) * ctx.accounts.global_data_account.max_time_between_bids;
        let steps = time / ctx.accounts.global_data_account.release_length;
        ctx.accounts.pool.release_time = steps * ctx.accounts.global_data_account.release_length; // do in past so that we can immediately release
        Ok(())
    }
    pub fn release(ctx: Context<Release>, id: u16) -> Result<()> {
        let time = Clock::get()?.unix_timestamp as u64;
        if ctx.accounts.global_data_account.pools != id {
            return Err(CustomError::InvalidId.into());
        }
        if time < ctx.accounts.pool.release_time {
            return Err(CustomError::PoolReleaseTimeNotPassed.into());
        }
        let steps = time / ctx.accounts.global_data_account.release_length;
        let delta = ((time - ctx.accounts.pool.release_time) / ctx.accounts.global_data_account.release_length) + 1;
        ctx.accounts.pool.release_time = (steps + 1) * ctx.accounts.global_data_account.release_length;
        let to_release = (utils::calculate_release(ctx.accounts.global_data_account.total_releases + delta) - utils::calculate_release(ctx.accounts.global_data_account.total_releases)) * ctx.accounts.global_data_account.release_amount;
        msg!("Releasing {}", to_release);
        ctx.accounts.global_data_account.total_releases += delta;
        ctx.accounts.pool.balance += to_release;
        Ok(())
    }
    pub fn create_bid(ctx: Context<CreateBid>, id: u16, account_id: u16) -> Result<()> {
        let bid = &mut ctx.accounts.bid;
        bid.pool = id;
        bid.account_id = account_id;
        bid.bidder = ctx.accounts.signer.key();
        bid.bid_ids = vec![];
        Ok(())
    }
    pub fn bid(ctx: Context<Bid>, id: u16, account_id: u16) -> Result<()> {
        // Validate against your existing pool/global rules
        if ctx.accounts.global_data_account.pools != id {
            return Err(CustomError::InvalidId.into());
        }
        let now = Clock::get()?.unix_timestamp as u64;
        if now > ctx.accounts.pool.bid_deadline {
            return Err(CustomError::BidDeadlinePassed.into());
        }
        // Price and transfer
        let price = ctx.accounts.global_data_account.fee * (ctx.accounts.pool.bids.pow(2) as u64);
        system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                system_program::Transfer {
                    from: ctx.accounts.signer.to_account_info(),
                    to: ctx.accounts.program_sol_account.to_account_info(),
                },
            ),
            price,
        )?;

        // One new u16 → need +2 bytes; ensure rent, then realloc
        {
            let bid_ai = &mut ctx.accounts.bid.to_account_info();
            let current = bid_ai.data_len();
            let needed = current + 2;

            if needed > current {
                let rent = Rent::get()?.minimum_balance(needed);
                if bid_ai.lamports() < rent {
                    let top_up = rent - bid_ai.lamports();
                    system_program::transfer(
                        CpiContext::new(
                            ctx.accounts.system_program.to_account_info(),
                            system_program::Transfer {
                                from: ctx.accounts.signer.to_account_info(),
                                to: bid_ai.clone(),
                            },
                        ),
                        top_up,
                    )?;
                }
                // zero = false because we immediately write valid data
                bid_ai.realloc(needed, /*zero*/ false)?;
            }
        }

        // Initialize fixed fields on first use (all zeros when created)
        let bid_acc = &mut ctx.accounts.bid;
        if bid_acc.bid_ids.len() == 0 {
            bid_acc.pool = id;
            bid_acc.account_id = account_id;
            bid_acc.bidder = ctx.accounts.signer.key();
        }

        // Append new bid id
        let next_bid_id = (ctx.accounts.pool.bids + 1) as u16;
        bid_acc.bid_ids.push(next_bid_id);

        // Update pool timing/counter
        let bucket = now / ctx.accounts.global_data_account.max_time_between_bids;
        ctx.accounts.pool.bid_deadline = (bucket + 2) * ctx.accounts.global_data_account.max_time_between_bids;
        ctx.accounts.pool.bids += 1;

        Ok(())
    }
    pub fn claim(ctx: Context<Claim>, id: u16, account_id: u16) -> Result<()> {
        let time = Clock::get()?.unix_timestamp as u64;
        if time < ctx.accounts.pool.bid_deadline {
            return Err(CustomError::BidDeadlineNotPassed.into());
        }
        if ctx.accounts.bid.bidder != ctx.accounts.signer.key() {
            return Err(CustomError::WrongBidAccountOwner.into());
        }
        if ctx.accounts.pool.bid_deadline + ctx.accounts.global_data_account.claim_expiry_time > time && ctx.accounts.bid.bid_ids.len() > 0 {
            let mut reward: u64 = 0;
            for bid in &ctx.accounts.bid.bid_ids {
                reward += utils::calculate_reward(ctx.accounts.pool.bids as u64, *bid as u64, ctx.accounts.pool.balance);
            }
            transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                        from: ctx.accounts.program_token_account.to_account_info(),
                        to: ctx.accounts.signer_token_account.to_account_info(),
                        authority: ctx.accounts.program_authority.to_account_info(),
                    },
                    &[&[b"auth", &[ctx.bumps.program_authority]]],
                ),
                reward,
            )?;
            emit!(ClaimEvent { user: *ctx.accounts.signer.key, amount: reward })
        }
        Ok(())
    }
}
#[error_code]
pub enum CustomError {
    #[msg("Invalid id")]
    InvalidId,
    #[msg("Bid deadline passed")]
    BidDeadlinePassed,
    #[msg("Bid deadline not passed")]
    BidDeadlineNotPassed,
    #[msg("Pool release time not passed")]
    PoolReleaseTimeNotPassed,
    #[msg("Invalid bid id")]
    InvalidBidId,
    #[msg("Wrong bid account owner")]
    WrongBidAccountOwner,
    #[msg("Invalid signer")]
    InvalidSigner,
    #[msg("No fees to withdraw")]
    NoFeesToWithdraw,
}
#[account]
pub struct GlobalData {
    pub pools: u16,
    pub fee: u64,
    pub release_length: u64,
    pub max_time_between_bids: u64,
    pub release_amount: u64,
    pub mint: Pubkey,
    pub total_releases: u64,
    pub claim_expiry_time: u64,
}
#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
    #[account(
        init,
        seeds = [b"global"],
        bump,
        payer = signer,
        space = 8 + 2 + 8 + 8 + 8 + 8 + 8 + 8 + 32,
    )]
    pub global_data_account: Account<'info, GlobalData>,
    #[account(
        init,
        seeds = [b"auth"],
        bump,
        payer = signer,
        space = 8
    )]
    /// CHECK:
    pub program_authority: AccountInfo<'info>,
    #[account(
        init,
        seeds = [b"pool", 0_u16.to_le_bytes().as_ref()],
        bump,
        payer = signer,
        space = 8 + 2 + 8 + 4 + 8 + 8
    )]
    pub zero_pool: Account<'info, Pool>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}
#[derive(Accounts)]
pub struct Initialize2<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
    #[account(
        seeds = [b"auth"],
        bump,
    )]
    /// CHECK:
    pub program_authority: AccountInfo<'info>,
    #[account(
        mut,
        seeds = [b"global"],
        bump,
    )]
    pub global_data_account: Account<'info, GlobalData>,
    pub mint: Account<'info, Mint>,
    #[account(
        init,
        seeds = [b"token"],
        bump,
        payer = signer,
        token::mint = mint,
        token::authority = program_authority
    )]
    pub program_token_account: Account<'info, TokenAccount>,
    #[account(
        init,
        seeds = [b"sol"],
        bump,
        payer = signer,
        space = 8
    )]
    /// CHECK:
    pub program_sol_account: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}
#[derive(Accounts)]
pub struct ModifyGlobalData<'info> {
    pub signer: Signer<'info>,
    #[account(
        mut,
        seeds = [b"global"],
        bump,
    )]
    pub global_data_account: Account<'info, GlobalData>,
}
#[derive(Accounts)]
pub struct DepositToken<'info> {
    pub signer: Signer<'info>,
    #[account(mut)]
    pub signer_token_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        seeds = [b"token"],
        bump,
    )]
    pub program_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}
#[derive(Accounts)]
pub struct WithdrawToken<'info> {
    pub signer: Signer<'info>,
    #[account(mut)]
    pub signer_token_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        seeds = [b"token"],
        bump,
    )]
    pub program_token_account: Account<'info, TokenAccount>,
    #[account(
        seeds = [b"auth"],
        bump,
    )]
    /// CHECK:
    pub program_authority: AccountInfo<'info>,
    pub token_program: Program<'info, Token>,
}
#[derive(Accounts)]
pub struct WithdrawSol<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
    #[account(
        mut,
        seeds = [b"sol"],
        bump,
    )]
    /// CHECK:
    pub program_sol_account: AccountInfo<'info>,
}
#[account]
pub struct Pool {
    pub id: u16,
    pub bid_deadline: u64,
    pub bids: u32,
    pub release_time: u64,
    pub balance: u64,
}
#[derive(Accounts)]
#[instruction(id: u16)]
pub struct NewPool<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
    #[account(
        init,
        seeds = [b"pool", id.to_le_bytes().as_ref()],
        bump,
        space = 8 + 2 + 8 + 4 + 8 + 8,
        payer = signer,
    )]
    pub pool: Account<'info, Pool>,
    #[account(
        mut,
        seeds = [b"pool", (id - 1).to_le_bytes().as_ref()],
        bump,
    )]
    pub prev_pool: Account<'info, Pool>,
    #[account(
        mut,
        seeds = [b"global"],
        bump,
    )]
    pub global_data_account: Account<'info, GlobalData>,
    pub system_program: Program<'info, System>,
}
#[derive(Accounts)]
#[instruction(id: u16)]
pub struct Release<'info> {
    pub signer: Signer<'info>,
    #[account(
        mut,
        seeds = [b"pool", id.to_le_bytes().as_ref()],
        bump,
    )]
    pub pool: Account<'info, Pool>,
    #[account(
        mut,
        seeds = [b"global"],
        bump,
    )]
    pub global_data_account: Account<'info, GlobalData>,
}
#[account]
pub struct BidAccount {
    pub pool: u16,
    pub account_id: u16,
    pub bidder: Pubkey,
    pub bid_ids: Vec<u16>,
}
impl BidAccount {
    pub const BASE: usize = 8 + 2 + 2 + 4 + 32;
    #[inline]
    pub fn space_for(len: usize) -> usize {
        Self::BASE + 2 * len
    }
}
#[derive(Accounts)]
#[instruction(id: u16, account_id: u16)]
pub struct CreateBid<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(
        init,
        payer = signer,
        space = BidAccount::BASE,           // 48 bytes (empty Vec)
        seeds = [b"bid", id.to_le_bytes().as_ref(), account_id.to_le_bytes().as_ref(), signer.key().as_ref()],
        bump
    )]
    pub bid: Account<'info, BidAccount>,

    pub system_program: Program<'info, System>,
}
#[derive(Accounts)]
#[instruction(id: u16, account_id: u16)]
pub struct Bid<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
    #[account(
        mut,
        seeds = [b"pool", id.to_le_bytes().as_ref()],
        bump,
    )]
    pub pool: Account<'info, Pool>,
    #[account(
        mut,
        seeds = [b"bid", id.to_le_bytes().as_ref(), account_id.to_le_bytes().as_ref(), signer.key().as_ref()],
        bump
    )]
    pub bid: Account<'info, BidAccount>,
    #[account(
        seeds = [b"global"],
        bump,
    )]
    pub global_data_account: Account<'info, GlobalData>,
    #[account(
        mut,
        seeds = [b"sol"],
        bump,
    )]
    /// CHECK:
    pub program_sol_account: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(id: u16, account_id: u16)]
pub struct Claim<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
    #[account(mut)]
    pub signer_token_account: Account<'info, TokenAccount>,
    #[account(
        seeds = [b"pool", id.to_le_bytes().as_ref()],
        bump
    )]
    pub pool: Account<'info, Pool>,
    #[account(
        mut,
        seeds = [b"bid", id.to_le_bytes().as_ref(), account_id.to_le_bytes().as_ref(), signer.key().as_ref()],
        bump,
        close = signer,
    )]
    pub bid: Account<'info, BidAccount>,
    #[account(
        mut,
        seeds = [b"token"],
        bump,
    )]
    pub program_token_account: Account<'info, TokenAccount>,
    #[account(
        seeds = [b"auth"],
        bump,
    )]
    /// CHECK:
    pub program_authority: AccountInfo<'info>,
    #[account(
        seeds = [b"global"],
        bump,
    )]
    pub global_data_account: Account<'info, GlobalData>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}
#[event]
pub struct ClaimEvent {
    pub user: Pubkey,
    pub amount: u64,
}

/*
solana program deploy --skip-fee-check ./program.so --with-compute-unit-price 100 --use-rpc --max-sign-attempts 1000
solana program deploy --skip-fee-check ./target/deploy/ogf_lottery.so  --with-compute-unit-price 100 --use-rpc --max-sign-attempts 1000 --keypair ~/.config/solana/id.json
*/

// RPC URL: https://devnet.helius-rpc.com/?api-key=e7b6dcae-bb88-4740-bc6b-908683b4725d
