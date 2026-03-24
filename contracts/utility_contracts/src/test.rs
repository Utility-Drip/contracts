#![cfg(test)]

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{token, Address, Env};

#[test]
fn test_prepaid_meter_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.set_oracle(&oracle);

    let token_admin = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token = token::Client::new(&env, &token_address);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    token_admin_client.mint(&user, &1000);

    let meter_id = client.register_meter(&user, &provider, &10, &token_address);
    assert_eq!(meter_id, 1);

    let meter = client.get_meter(&meter_id).unwrap();
    assert_eq!(meter.billing_type, BillingType::PrePaid);
    assert_eq!(meter.off_peak_rate, 10);
    assert_eq!(meter.balance, 0);
    assert_eq!(meter.debt, 0);
    assert_eq!(meter.collateral_limit, 0);
    assert!(!meter.is_active);
    assert_eq!(meter.max_flow_rate_per_hour, 36000);

    client.top_up(&meter_id, &500);
    let meter = client.get_meter(&meter_id).unwrap();
    assert_eq!(meter.balance, 500);
    assert!(meter.is_active);
    assert_eq!(token.balance(&user), 500);
    assert_eq!(token.balance(&contract_id), 500);

    env.ledger().set_timestamp(env.ledger().timestamp() + 5);
    client.claim(&meter_id);

    let meter = client.get_meter(&meter_id).unwrap();
    assert_eq!(meter.balance, 450);
    assert_eq!(token.balance(&provider), 50);
    assert_eq!(token.balance(&contract_id), 450);

    client.deduct_units(&meter_id, &15);
    let meter = client.get_meter(&meter_id).unwrap();
    assert_eq!(meter.balance, 300);
    assert_eq!(token.balance(&provider), 200);
    assert_eq!(token.balance(&contract_id), 300);

    client.deduct_units(&meter_id, &50);
    let meter = client.get_meter(&meter_id).unwrap();
    assert_eq!(meter.balance, 0);
    assert!(!meter.is_active);
    assert_eq!(token.balance(&provider), 500);
    assert_eq!(token.balance(&contract_id), 0);

    client.update_usage(&meter_id, &1500);
    let usage_data = client.get_usage_data(&meter_id).unwrap();
    assert_eq!(usage_data.total_watt_hours, 1_500_000);
    assert_eq!(usage_data.current_cycle_watt_hours, 1_500_000);
    assert_eq!(usage_data.peak_usage_watt_hours, 1_500_000);

    client.reset_cycle_usage(&meter_id);
    let usage_data = client.get_usage_data(&meter_id).unwrap();
    assert_eq!(usage_data.total_watt_hours, 1_500_000);
    assert_eq!(usage_data.current_cycle_watt_hours, 0);
    assert_eq!(usage_data.peak_usage_watt_hours, 1_500_000);

    client.update_usage(&meter_id, &2000);
    let usage_data = client.get_usage_data(&meter_id).unwrap();
    assert_eq!(usage_data.total_watt_hours, 3_500_000);
    assert_eq!(usage_data.current_cycle_watt_hours, 2_000_000);
    assert_eq!(usage_data.peak_usage_watt_hours, 2_000_000);

    let display_total =
        UtilityContract::get_watt_hours_display(usage_data.total_watt_hours, usage_data.precision_factor);
    assert_eq!(display_total, 3500);
}

#[test]
fn test_max_flow_rate_cap() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.set_oracle(&oracle);

    let token_admin = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    token_admin_client.mint(&user, &10_000);

    let meter_id = client.register_meter(&user, &provider, &100, &token_address);
    client.set_max_flow_rate(&meter_id, &5000);
    client.top_up(&meter_id, &10_000);
    client.deduct_units(&meter_id, &120);

    let meter = client.get_meter(&meter_id).unwrap();
    assert_eq!(meter.claimed_this_hour, 5000);
    assert_eq!(meter.balance, 5000);
}

#[test]
fn test_calculate_expected_depletion() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    token_admin_client.mint(&user, &1000);

    let meter_id = client.register_meter(&user, &provider, &10, &token_address);
    client.top_up(&meter_id, &500);

    let depletion_time = client.calculate_expected_depletion(&meter_id).unwrap();
    let current_time = env.ledger().timestamp();
    assert_eq!(depletion_time, current_time + 50);
}

#[test]
fn test_emergency_shutdown() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    token_admin_client.mint(&user, &1000);

    let meter_id = client.register_meter(&user, &provider, &10, &token_address);
    client.top_up(&meter_id, &500);

    let meter = client.get_meter(&meter_id).unwrap();
    assert!(meter.is_active);

    client.emergency_shutdown(&meter_id);

    let meter = client.get_meter(&meter_id).unwrap();
    assert!(!meter.is_active);
}

#[test]
fn test_heartbeat_functionality() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    token_admin_client.mint(&user, &1000);

    let meter_id = client.register_meter(&user, &provider, &10, &token_address);

    assert!(!client.is_meter_offline(&meter_id));

    env.ledger().set_timestamp(env.ledger().timestamp() + 3700);
    assert!(client.is_meter_offline(&meter_id));

    client.update_heartbeat(&meter_id);
    assert!(!client.is_meter_offline(&meter_id));
}

#[test]
fn test_claim_within_daily_limit_tracks_withdrawn() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token = token::Client::new(&env, &token_address);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    token_admin_client.mint(&user, &1000);

    let meter_id = client.register_meter(&user, &provider, &10, &token_address);
    client.top_up(&meter_id, &500);

    env.ledger().set_timestamp(env.ledger().timestamp() + 5);
    client.claim(&meter_id);

    let meter = client.get_meter(&meter_id).unwrap();
    let provider_window = client.get_provider_window(&provider).unwrap();

    assert_eq!(meter.balance, 450);
    assert_eq!(token.balance(&provider), 50);
    assert_eq!(token.balance(&contract_id), 450);
    assert_eq!(provider_window.daily_withdrawn, 50);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_claim_reverts_when_daily_limit_is_exceeded() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    token_admin_client.mint(&user, &1000);

    let meter_id = client.register_meter(&user, &provider, &10, &token_address);
    client.top_up(&meter_id, &500);

    env.ledger().set_timestamp(env.ledger().timestamp() + 10);
    client.claim(&meter_id);
}

#[test]
fn test_daily_limit_resets_after_24_hours() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token = token::Client::new(&env, &token_address);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    token_admin_client.mint(&user, &1_000_000);

    let meter_id = client.register_meter(&user, &provider, &1, &token_address);
    client.set_max_flow_rate(&meter_id, &1_000_000);
    client.top_up(&meter_id, &1_000_000);

    env.ledger().set_timestamp(env.ledger().timestamp() + 10_000);
    client.claim(&meter_id);

    let provider_window = client.get_provider_window(&provider).unwrap();
    assert_eq!(provider_window.daily_withdrawn, 10_000);

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + (24 * 60 * 60) + 5_000);
    client.claim(&meter_id);

    let provider_window = client.get_provider_window(&provider).unwrap();
    assert_eq!(provider_window.daily_withdrawn, 91_400);
    assert_eq!(token.balance(&provider), 101_400);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_daily_limit_is_shared_across_provider_meters() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user_one = Address::generate(&env);
    let user_two = Address::generate(&env);
    let provider = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    token_admin_client.mint(&user_one, &500);
    token_admin_client.mint(&user_two, &500);

    let meter_one = client.register_meter(&user_one, &provider, &10, &token_address);
    let meter_two = client.register_meter(&user_two, &provider, &10, &token_address);

    client.top_up(&meter_one, &500);
    client.top_up(&meter_two, &500);

    env.ledger().set_timestamp(env.ledger().timestamp() + 5);
    client.claim(&meter_one);
    client.claim(&meter_two);

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.claim(&meter_one);
}

#[test]
fn test_postpaid_claims_against_collateral_limit() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.set_oracle(&oracle);

    let token_admin = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token = token::Client::new(&env, &token_address);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    token_admin_client.mint(&user, &500);

    let meter_id = client.register_meter_with_mode(
        &user,
        &provider,
        &10,
        &token_address,
        &BillingType::PostPaid,
    );

    client.top_up(&meter_id, &300);

    let meter = client.get_meter(&meter_id).unwrap();
    assert_eq!(meter.billing_type, BillingType::PostPaid);
    assert_eq!(meter.balance, 0);
    assert_eq!(meter.debt, 0);
    assert_eq!(meter.collateral_limit, 300);
    assert!(meter.is_active);
    assert_eq!(token.balance(&contract_id), 300);

    env.ledger().set_timestamp(env.ledger().timestamp() + 3);
    client.claim(&meter_id);

    let meter = client.get_meter(&meter_id).unwrap();
    assert_eq!(meter.debt, 30);
    assert_eq!(meter.collateral_limit, 300);
    assert!(meter.is_active);
    assert_eq!(token.balance(&provider), 30);
    assert_eq!(token.balance(&contract_id), 270);

    client.deduct_units(&meter_id, &27);

    let meter = client.get_meter(&meter_id).unwrap();
    assert_eq!(meter.debt, 300);
    assert_eq!(meter.collateral_limit, 300);
    assert!(!meter.is_active);
    assert_eq!(token.balance(&provider), 300);
    assert_eq!(token.balance(&contract_id), 0);
}

#[test]
fn test_postpaid_top_up_settles_debt_and_resets_when_reactivated() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.set_oracle(&oracle);

    let token_admin = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token = token::Client::new(&env, &token_address);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    token_admin_client.mint(&user, &500);

    let meter_id = client.register_meter_with_mode(
        &user,
        &provider,
        &10,
        &token_address,
        &BillingType::PostPaid,
    );

    client.top_up(&meter_id, &100);
    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.claim(&meter_id);
    client.deduct_units(&meter_id, &9);

    let meter = client.get_meter(&meter_id).unwrap();
    assert_eq!(meter.debt, 100);
    assert!(!meter.is_active);
    assert_eq!(token.balance(&provider), 100);

    env.ledger().set_timestamp(env.ledger().timestamp() + 80);
    client.top_up(&meter_id, &200);

    let meter = client.get_meter(&meter_id).unwrap();
    assert_eq!(meter.debt, 0);
    assert_eq!(meter.collateral_limit, 200);
    assert!(meter.is_active);
    assert_eq!(token.balance(&contract_id), 200);

    env.ledger().set_timestamp(env.ledger().timestamp() + 1);
    client.claim(&meter_id);

    let meter = client.get_meter(&meter_id).unwrap();
    assert_eq!(meter.debt, 10);
    assert_eq!(meter.collateral_limit, 200);
    assert!(meter.is_active);
    assert_eq!(token.balance(&provider), 110);
    assert_eq!(token.balance(&contract_id), 190);
}

#[test]
fn test_variable_rate_tariffs_peak_vs_offpeak() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token = token::Client::new(&env, &token_address);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    token_admin_client.mint(&user, &10_000);

    // Register meter with off-peak rate of 10 tokens/second
    // Peak rate will be automatically set to 15 (10 * 1.5)
    let meter_id = client.register_meter(&user, &provider, &10, &token_address);
    
    let meter = client.get_meter(&meter_id).unwrap();
    assert_eq!(meter.off_peak_rate, 10);
    assert_eq!(meter.peak_rate, 15);

    client.top_up(&meter_id, &1000);

    // Test OFF-PEAK claim: timestamp = 46800 (13:00 UTC)
    // This is before peak hours (18:00-21:00), so use off_peak_rate
    env.ledger().set_timestamp(46800); // 13:00 UTC
    let meter_before = client.get_meter(&meter_id).unwrap();
    let initial_balance = meter_before.balance;

    env.ledger().set_timestamp(46805); // 5 seconds later
    client.claim(&meter_id);

    let meter_after_offpeak = client.get_meter(&meter_id).unwrap();
    let offpeak_deduction = initial_balance - meter_after_offpeak.balance;
    // Off-peak: 5 seconds * 10 tokens/second = 50 tokens
    assert_eq!(offpeak_deduction, 50);
    assert_eq!(token.balance(&provider), 50);

    // Test PEAK claim: timestamp = 68400 (19:00 UTC)
    // This is during peak hours (18:00-21:00), so use peak_rate (1.5x)
    env.ledger().set_timestamp(68400); // 19:00 UTC
    let meter_before_peak = client.get_meter(&meter_id).unwrap();
    let balance_before_peak = meter_before_peak.balance;

    env.ledger().set_timestamp(68405); // 5 seconds later (still at 19:00)
    client.claim(&meter_id);

    let meter_after_peak = client.get_meter(&meter_id).unwrap();
    let peak_deduction = balance_before_peak - meter_after_peak.balance;
    // Peak: 5 seconds * 15 tokens/second = 75 tokens
    assert_eq!(peak_deduction, 75);
    assert_eq!(token.balance(&provider), 125); // 50 + 75

    // Verify the rate multiplier was correctly applied
    // peak_rate should be 1.5x off_peak_rate
    assert_eq!(meter_after_peak.peak_rate, (meter_after_peak.off_peak_rate * 3) / 2);
}

#[test]
fn test_variable_rate_deduct_units_respects_peak_hours() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(UtilityContract, ());
    let client = UtilityContractClient::new(&env, &contract_id);

    let user = Address::generate(&env);
    let provider = Address::generate(&env);
    let oracle = Address::generate(&env);
    client.set_oracle(&oracle);

    let token_admin = Address::generate(&env);
    let token_address = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token = token::Client::new(&env, &token_address);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_address);

    token_admin_client.mint(&user, &5000);

    // Register with off-peak rate of 20 tokens/second
    let meter_id = client.register_meter(&user, &provider, &20, &token_address);
    client.top_up(&meter_id, &2000);

    // OFF-PEAK deduction at 10:00 UTC
    env.ledger().set_timestamp(36000); // 10:00 UTC
    client.deduct_units(&meter_id, &10); // 10 units
    
    let meter = client.get_meter(&meter_id).unwrap();
    // Off-peak: 10 units * 20 tokens/unit = 200 tokens
    assert_eq!(meter.balance, 1800);
    assert_eq!(token.balance(&provider), 200);

    // PEAK deduction at 20:00 UTC
    env.ledger().set_timestamp(72000); // 20:00 UTC
    client.deduct_units(&meter_id, &10); // 10 units
    
    let meter = client.get_meter(&meter_id).unwrap();
    // Peak: 10 units * 30 tokens/unit (20 * 1.5) = 300 tokens
    assert_eq!(meter.balance, 1500);
    assert_eq!(token.balance(&provider), 500); // 200 + 300
}
