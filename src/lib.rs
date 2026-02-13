#![no_std]
use soroban_sdk::{contract, contracttype, contractimpl, Address, Env, token};

#[contracttype]
#[derive(Clone)]
pub struct Meter {
    pub user: Address,
    pub provider: Address,
    pub rate_per_second: i128,
    pub balance: i128,
    pub last_update: u64,
    pub is_active: bool,
    pub token: Address,
}

#[contracttype]
pub enum DataKey {
    Meter(u64),
    Count,
}

#[contract]
pub struct UtilityContract;

#[contractimpl]
impl UtilityContract {
    pub fn register_meter(
        env: Env,
        user: Address,
        provider: Address,
        rate: i128,
        token: Address,
    ) -> u64 {
        user.require_auth();
        let mut count: u64 = env.storage().instance().get(&DataKey::Count).unwrap_or(0);
        count += 1;

        let meter = Meter {
            user,
            provider,
            rate_per_second: rate,
            balance: 0,
            last_update: env.ledger().timestamp(),
            is_active: false,
            token,
        };

        env.storage().instance().set(&DataKey::Meter(count), &meter);
        env.storage().instance().set(&DataKey::Count, &count);
        count
    }

    pub fn top_up(env: Env, meter_id: u64, amount: i128) {
        let mut meter: Meter = env.storage().instance().get(&DataKey::Meter(meter_id)).unwrap();
        meter.user.require_auth();

        let client = token::Client::new(&env, &meter.token);
        client.transfer(&meter.user, &env.current_contract_address(), &amount);

        meter.balance += amount;
        meter.is_active = true;
        meter.last_update = env.ledger().timestamp();
        
        env.storage().instance().set(&DataKey::Meter(meter_id), &meter);
    }
}
