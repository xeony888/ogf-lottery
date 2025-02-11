pub fn calculate_reward(bids: u64, id: u64, amount: u64) -> u64 {
    let sum = bids * (bids + 1) / 2;
    return sum * amount / id;
}
