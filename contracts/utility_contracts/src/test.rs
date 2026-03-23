#![cfg(test)]

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

#[test]
fn test_utility_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let oracle = Address::generate(&env);
    
    // Setup Oracle
    client.set_oracle(&oracle);

    // Setup a token
    let token_admin = Address::generate(&env);
    let token_address = env.register_stellar_asset_contract(token_admin.clone());
    let token = token::Client::new(&env, &token_address);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    // Initial funding
    token_admin_client.mint(&user, &1000);

    // 1. Register Meter
    let rate = 10; // 10 tokens per unit (kWh)
    let meter_id = client.register_meter(&user, &provider, &rate, &token_address);
    assert_eq!(meter_id, 1);

    let meter = client.get_meter(&meter_id).unwrap();
    assert_eq!(meter.rate_per_unit, 10);
    assert_eq!(meter.balance, 0);
    assert_eq!(meter.is_active, false);

    // 2. Top up
    client.top_up(&meter_id, &500);
    let meter = client.get_meter(&meter_id).unwrap();
    assert_eq!(meter.balance, 500);
    assert_eq!(meter.is_active, true);
    assert_eq!(token.balance(&user), 500);
    assert_eq!(token.balance(&contract_id), 500);

    // 3. Report usage (billing by units)
    let units_consumed = 15; // 15 kWh
    client.deduct_units(&meter_id, &units_consumed);
    
    let meter = client.get_meter(&meter_id).unwrap();
    // 15 units * 10 tokens/unit = 150 tokens claimed
    assert_eq!(meter.balance, 350);
    assert_eq!(token.balance(&provider), 150);
    assert_eq!(token.balance(&contract_id), 350);

    // 4. Report usage that exceeds balance
    let more_units = 50; // 50 units * 10 = 500 cost, but only 350 left
    client.deduct_units(&meter_id, &more_units);

    let meter = client.get_meter(&meter_id).unwrap();
    assert_eq!(meter.balance, 0);
    assert_eq!(meter.is_active, false);
    assert_eq!(token.balance(&provider), 500);
    assert_eq!(token.balance(&contract_id), 0);
}
