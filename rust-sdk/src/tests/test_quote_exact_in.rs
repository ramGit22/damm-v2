use std::u64;

use crate::{quote, tests::get_pool_account};

#[test]
fn test_quote_exact_in() {
    let pool = get_pool_account();

    let current_timestamp: u64 = 1_753_751_761;
    let current_slot: u64 = 356410171;

    let a_to_b: bool = false;
    let has_referral: bool = false;

    let actual_amount_in = u64::MAX;

    let swap_result = quote::get_quote(
        &pool,
        current_timestamp,
        current_slot,
        actual_amount_in,
        a_to_b,
        has_referral,
    )
    .unwrap();
    println!("swap_result {} {:?}", actual_amount_in, swap_result);
}

#[test]
fn test_quote_propagates_disable_error() {
    let mut pool = get_pool_account();
    pool.pool_status = 1;

    let current_timestamp: u64 = 1_753_751_761;
    let current_slot: u64 = 356410171;
    let actual_amount_in = 1u64;

    let result = quote::get_quote(
        &pool,
        current_timestamp,
        current_slot,
        actual_amount_in,
        true,
        false,
    );

    assert!(result.is_err());
}
