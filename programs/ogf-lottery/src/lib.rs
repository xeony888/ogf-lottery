use anchor_lang::prelude::*;

declare_id!("49wfpTZKZmpAM1jmTBcL5GfobDU3r9F4CMUpZqb2o3ZQ");

#[program]
pub mod ogf_lottery {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        msg!("Greetings from: {:?}", ctx.program_id);
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize {}
