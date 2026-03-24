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
    
    // Setup a token
    let token_admin = Address::generate(&env);
    let token_address = env.register_stellar_asset_contract(token_admin.clone());
    let token = token::Client::new(&env, &token_address);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    // Initial funding - provide enough for minimum balance tests
    token_admin_client.mint(&user, &10000000); // 10 XLM

    // 1. Register Meter
    let rate = 10; // 10 tokens per second
    let meter_id = client.register_meter(&user, &provider, &rate, &token_address);
    assert_eq!(meter_id, 1);

    let meter = client.get_meter(&meter_id).unwrap();
    assert_eq!(meter.rate_per_second, 10);
    assert_eq!(meter.balance, 0);
    assert_eq!(meter.is_active, false);
    assert_eq!(meter.usage_data.total_watt_hours, 0);
    assert_eq!(meter.usage_data.current_cycle_watt_hours, 0);
    assert_eq!(meter.usage_data.peak_usage_watt_hours, 0);
    assert_eq!(meter.usage_data.precision_factor, 1000);

    // 2. Top up with minimum balance
    client.top_up(&meter_id, &5000000); // 5 XLM - meets minimum
    let meter = client.get_meter(&meter_id).unwrap();
    assert_eq!(meter.balance, 5000000);
    assert_eq!(meter.is_active, true);
    assert_eq!(token.balance(&user), 5000000); // 10,000,000 - 5,000,000 = 5,000,000 remaining
    assert_eq!(token.balance(&contract_id), 5000000);

    // 3. Claim balance (simulate time passing)
    env.ledger().set_timestamp(env.ledger().timestamp() + 10); // 10 seconds pass
    client.claim(&meter_id);
    
    let meter = client.get_meter(&meter_id).unwrap();
    // 10 seconds * 10 tokens/sec = 100 tokens claimed
    assert_eq!(meter.balance, 4999900); // 5,000,000 - 100
    assert_eq!(meter.is_active, false); // Below minimum (4,999,900 < 5,000,000)
    assert_eq!(token.balance(&provider), 100);
    assert_eq!(token.balance(&contract_id), 4999900);

    // 4. Claim more to drop below minimum
    env.ledger().set_timestamp(env.ledger().timestamp() + 499990); // 499,990 seconds pass
    client.claim(&meter_id); // This should claim 4,999,900 tokens (499,990 * 10)

    let meter = client.get_meter(&meter_id).unwrap();
    assert_eq!(meter.balance, 0); // 4,999,900 - 4,999,900 = 0
    assert_eq!(meter.is_active, false); // Below minimum
    assert_eq!(token.balance(&provider), 5000000); // 100 + 4,999,900 = 5,000,000 total
    assert_eq!(token.balance(&contract_id), 0);

    // 5. Test usage tracking
    client.update_usage(&meter_id, &1500); // 1.5 kWh
    let usage_data = client.get_usage_data(&meter_id).unwrap();
    assert_eq!(usage_data.total_watt_hours, 1500000); // 1500 * 1000 precision
    assert_eq!(usage_data.current_cycle_watt_hours, 1500000);
    assert_eq!(usage_data.peak_usage_watt_hours, 1500000);

    // 6. Test cycle reset
    client.reset_cycle_usage(&meter_id);
    let usage_data = client.get_usage_data(&meter_id).unwrap();
    assert_eq!(usage_data.total_watt_hours, 1500000); // Total remains
    assert_eq!(usage_data.current_cycle_watt_hours, 0); // Current cycle reset
    assert_eq!(usage_data.peak_usage_watt_hours, 1500000); // Peak remains

    // 7. Test peak usage update
    client.update_usage(&meter_id, &2000); // 2.0 kWh
    let usage_data = client.get_usage_data(&meter_id).unwrap();
    assert_eq!(usage_data.total_watt_hours, 3500000); // 1500 + 2000
    assert_eq!(usage_data.current_cycle_watt_hours, 2000000); // New cycle
    assert_eq!(usage_data.peak_usage_watt_hours, 2000000); // Updated peak

    // 8. Test display helper function
    let display_total = UtilityContract::get_watt_hours_display(usage_data.total_watt_hours, usage_data.precision_factor);
    assert_eq!(display_total, 3500); // 3500000 / 1000 = 3500 (3.5 kWh)

    // 9. Test minimum balance functionality
    let min_balance = client.get_minimum_balance_to_flow();
    assert_eq!(min_balance, 5000000); // 5 XLM in stroops

    // Test small top-up that doesn't meet minimum
    let meter_id_2 = client.register_meter(&user, &provider, &rate, &token_address);
    client.top_up(&meter_id_2, &1000000); // 1 XLM - below minimum
    let meter_2 = client.get_meter(&meter_id_2).unwrap();
    assert_eq!(meter_2.balance, 1000000);
    assert_eq!(meter_2.is_active, false); // Should not be active

    // Test top-up that meets minimum
    client.top_up(&meter_id_2, &4000000); // Add 4 XLM more = 5 XLM total
    let meter_2 = client.get_meter(&meter_id_2).unwrap();
    assert_eq!(meter_2.balance, 5000000);
    assert_eq!(meter_2.is_active, true); // Should now be active

    // Test claim that drops below minimum
    env.ledger().set_timestamp(env.ledger().timestamp() + 100); // 100 seconds pass
    client.claim(&meter_id_2); // This should claim 1000 tokens (100 * 10)
    let meter_2 = client.get_meter(&meter_id_2).unwrap();
    assert_eq!(meter_2.balance, 4999000); // 5M - 1M = 4.999M (not 4M)
    assert_eq!(meter_2.is_active, false); // Should be deactivated
}
