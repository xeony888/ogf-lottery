pub fn calculate_reward(bids: u64, id: u64, amount: u64) -> u64 {
    if bids == id {
        return amount / 2;
    } else {
        return amount / 2 / (bids - 1);
    }
}

pub fn calculate_release(delta: u64) -> u64 {
    let adj = delta as u128;
    let sum128 = adj.checked_mul(adj + 1).expect("overflow in multiplication") / 2;
    return sum128.try_into().expect("result doesn’t fit in u64");
}
