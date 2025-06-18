pub fn calculate_reward(bids: u64, id: u64, amount: u64) -> u64 {
    let sum = bids * (bids + 1) / 2;
    let distance = bids - id;
    return distance * amount / sum;
}

// implement function for the sum of squares of first delta natural numbers
pub fn calculate_release(delta: u64) -> u64 {
    return (delta * (delta + 1) * (2 * delta + 1)) / 6;
}
