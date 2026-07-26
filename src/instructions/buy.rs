use borsh::BorshDeserialize;
use solana_program::{
    account_info::{AccountInfo, next_account_info},
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvar::{Sysvar, clock::Clock},
};

use super::lib::{
    close_ata, get_ata, get_pda, get_token_balance, transfer, transfer_from_pda,
};
use crate::state::Auction;

pub fn buy(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    max_price: u64,
    // Auction PDA bump
    bump: u8,
) -> Result<(), ProgramError> {
    let account_iter = &mut accounts.iter();

    let buyer = next_account_info(account_iter)?;
    let seller = next_account_info(account_iter)?;
    let mint_sell = next_account_info(account_iter)?;
    let mint_buy = next_account_info(account_iter)?;
    let auction_pda = next_account_info(account_iter)?;
    let auction_sell_ata = next_account_info(account_iter)?;
    let buyer_sell_ata = next_account_info(account_iter)?;
    let buyer_buy_ata = next_account_info(account_iter)?;
    let seller_buy_ata = next_account_info(account_iter)?;
    let token_program = next_account_info(account_iter)?;
    let sys_program = next_account_info(account_iter)?;

    // Check buyer signed
    if !buyer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    // Check that auction_pda matches expected PDA
    if get_pda(program_id, seller.key, mint_sell.key, mint_buy.key, bump)? != *auction_pda.key {
        return Err(ProgramError::InvalidArgument);
    }
    // Check that auction_sell_ata matches calculated account
    if get_ata(auction_pda.key, mint_sell.key) != *auction_sell_ata.key {
        return Err(ProgramError::InvalidArgument);
    }
    // Check that buyer_sell_ata matches calculated account
    if get_ata(buyer.key, mint_sell.key) != *buyer_sell_ata.key {
        return Err(ProgramError::InvalidArgument);
    }
    // Check that buyer_buy_ata matches calculated account
    if get_ata(buyer.key, mint_buy.key) != *buyer_buy_ata.key {
        return Err(ProgramError::InvalidArgument);
    }
    // Check that seller_buy_ata matches calculated account
    if get_ata(seller.key, mint_buy.key) != *seller_buy_ata.key {
        return Err(ProgramError::InvalidArgument);
    }

    let clock = Clock::get()?;
    let now: u64 = clock.unix_timestamp.try_into().unwrap();

    
    let auction = {
        let data = auction_pda.data.borrow();
        Auction::try_from_slice(&data)?
    };
    

    // Check auction has started
    if now < auction.start_time {
        return Err(ProgramError::InvalidArgument);
    }
    // Check auction has not ended
    if now > auction.end_time {
        return Err(ProgramError::InvalidArgument);
    }

    // Calculate price
    let current_price = auction.start_price - ((auction.start_price - auction.end_price) * (now - auction.start_time) / (auction.end_time - auction.start_time));
    // Check current price is greater than or equal to end_price
    if current_price < auction.end_price {
        return Err(ProgramError::InvalidArgument);
    }
    // Check current price is less than or equal to max_price
    if current_price > max_price {
        return Err(ProgramError::InvalidArgument);
    }

    // Calculate amount of buy token to send to seller
    let sell_amt = get_token_balance(auction_sell_ata)?;
    let buy_amt = sell_amt * current_price / (1e6 as u64);

    // Send buy token to seller
    transfer(token_program, buyer_buy_ata, seller_buy_ata, buyer, buy_amt)?;

    // Send sell token to buyer
    let seeds = &[
        Auction::SEED_PREFIX,
        seller.key.as_ref(),
        mint_sell.key.as_ref(),
        mint_buy.key.as_ref(),
        &[bump],
    ];
    transfer_from_pda(
        token_program,
        auction_sell_ata,
        buyer_sell_ata,
        auction_pda,
        sell_amt,
        seeds,
    )?;

    // Close auction_sell_ata
    close_ata(
        token_program,
        auction_sell_ata,
        seller,
        auction_pda,
        seeds,
    )?;

    // Close auction_pda
    let pda_lamports = auction_pda.lamports();
    **auction_pda.try_borrow_mut_lamports()? = 0;
    **seller.try_borrow_mut_lamports()? = seller
        .lamports()
        .checked_add(pda_lamports)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    // Clear out data
    auction_pda.resize(0)?;
    // Assign the account to the System Program
    auction_pda.assign(sys_program.key);

    Ok(())
}
