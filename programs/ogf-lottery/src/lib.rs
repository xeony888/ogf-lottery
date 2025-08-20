use anchor_lang::prelude::*;
use anchor_lang::solana_program::native_token::LAMPORTS_PER_SOL;
use anchor_spl::token::{transfer, Mint, Token, TokenAccount, Transfer};

mod utils;
declare_id!("BtZ6LuVh4nSDQvTVhb9JMm8aaq1nbj9YBJxN2CXK8MzB");
const ADMIN: &str = "6MeJK3erCnwMtsAHLBhRFaXELpzCBfMrrESEJiBWaHTK"; //"oggzGFTgRM61YmhEbgWeivVmQx8bSAdBvsPGqN3ZfxN";

#[program]
pub mod ogf_lottery {
    use super::*;
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        ctx.accounts.global_data_account.release_length = 500;
        ctx.accounts.global_data_account.fee = LAMPORTS_PER_SOL / 1000000;
        ctx.accounts.global_data_account.release_amount = 100000;
        ctx.accounts.global_data_account.max_time_between_bids = 1000;
        ctx.accounts.global_data_account.total_releases = 0;
        Ok(())
    }
    pub fn initialize2(ctx: Context<Initialize2>) -> Result<()> {
        ctx.accounts.global_data_account.mint = ctx.accounts.mint.key();
        Ok(())
    }
    pub fn modify_global_data(ctx: Context<ModifyGlobalData>, fee: u64, release_length: u64, max_time_between_bids: u64, release_amount: u64) -> Result<()> {
        if ADMIN.parse::<Pubkey>().unwrap() != ctx.accounts.signer.key() {
            return Err(CustomError::InvalidSigner.into());
        }
        ctx.accounts.global_data_account.fee = fee;
        ctx.accounts.global_data_account.release_length = release_length;
        ctx.accounts.global_data_account.max_time_between_bids = max_time_between_bids;
        ctx.accounts.global_data_account.release_amount = release_amount;
        Ok(())
    }
    pub fn deposit_token(ctx: Context<DepositToken>, amount: u64) -> Result<()> {
        if ADMIN.parse::<Pubkey>().unwrap() != ctx.accounts.signer.key() {
            return Err(CustomError::InvalidSigner.into());
        }
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
    pub fn bid(ctx: Context<Bid>, id: u16, bid_id: u16) -> Result<()> {
        if ctx.accounts.global_data_account.pools != id {
            return Err(CustomError::InvalidId.into());
        }
        if ctx.accounts.pool.bids != bid_id as u32 {
            return Err(CustomError::InvalidBidId.into());
        }
        let time = Clock::get()?.unix_timestamp as u64;
        if time > ctx.accounts.pool.bid_deadline {
            return Err(CustomError::BidDeadlinePassed.into());
        }
        let price = ctx.accounts.global_data_account.fee * ctx.accounts.pool.bids.pow(2) as u64;
        anchor_lang::system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                anchor_lang::system_program::Transfer {
                    from: ctx.accounts.signer.to_account_info(),
                    to: ctx.accounts.program_sol_account.to_account_info(),
                },
            ),
            price,
        )?;
        let count = time / ctx.accounts.global_data_account.max_time_between_bids;
        // ctx.accounts.pool.bid_deadline = time + ctx.accounts.global_data_account.max_time_between_bids;
        ctx.accounts.pool.bid_deadline = (count + 2) * ctx.accounts.global_data_account.max_time_between_bids;
        ctx.accounts.pool.bids += 1;
        ctx.accounts.bid.bidder = ctx.accounts.signer.key();
        ctx.accounts.bid.bid_id = bid_id;
        ctx.accounts.bid.pool = id;
        Ok(())
    }
    pub fn claim(ctx: Context<Claim>, id: u16, bid_id: u16) -> Result<()> {
        let time = Clock::get()?.unix_timestamp as u64;
        if time < ctx.accounts.pool.bid_deadline {
            return Err(CustomError::BidDeadlineNotPassed.into());
        }
        if ctx.accounts.bid.bidder != ctx.accounts.signer.key() {
            return Err(CustomError::WrongBidAccountOwner.into());
        }
        let reward = utils::calculate_reward(ctx.accounts.pool.bids as u64, bid_id as u64, ctx.accounts.pool.balance);
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
        space = 8 + 2 + 8 + 8 + 8 + 8 + 8 + 32,
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
    pub bid_id: u16,
    pub bidder: Pubkey,
}
#[derive(Accounts)]
#[instruction(id: u16, bid_id: u16)]
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
        init,
        seeds = [b"bid", id.to_le_bytes().as_ref(), bid_id.to_le_bytes().as_ref()],
        bump,
        payer = signer,
        space = 8 + 2 + 2 + 32
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
#[instruction(id: u16, bid_id: u16)]
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
        seeds = [b"bid", id.to_le_bytes().as_ref(), bid_id.to_le_bytes().as_ref()],
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
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

/*
solana program deploy --skip-fee-check ./program.so --with-compute-unit-price 100 --use-rpc --max-sign-attempts 1000
solana program deploy --skip-fee-check ./target/deploy/ogc_reserve.so  --with-compute-unit-price 100 --use-rpc --max-sign-attempts 1000 --keypair /home/xeony/.config/solana/id.json
*/
