use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
// use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::Vector;
use near_sdk::near_bindgen;

use std::thread;
use std::time::Duration;

near_sdk::setup_alloc!();

// const CMC_PRO_API_KEY: &str = "b89d1f7b-2ada-4334-9545-c6ce17e88698";
// const CMC_SYMBOL: &str = "BTC";
// const CMC_PRO_API_QUOTES_URI: &str = "https://pro-api.coinmarketcap.com/v1/cryptocurrency/quotes/latest";
const MAX_SIZE: usize = 5;

#[near_bindgen]
// #[derive(BorshDeserialize, BorshSerialize)]
pub struct RateContract {
    // #[borsh_skip]
    // handle: Option<JoinHandle<()>>,
    // values: Arc<RwLock<Vector<f64>>>,
    values: Arc<RwLock<VecDeque<f64>>>,
    stop_update: Arc<AtomicBool>,
}

impl Default for RateContract {
    fn default() -> Self {
        // let v = Vector::new(b"v".to_vec());
        let v = VecDeque::new();
        Self {
            // handle: None,
            values: Arc::new(RwLock::new(v)),
            stop_update: Arc::new(AtomicBool::new(false))
        }
    }
}

#[near_bindgen]
impl RateContract {
    #[init]
    pub fn new(refresh_ms: u64) -> Self {
        let res = Self::default();

        let _handle = thread::spawn({
            let values = Arc::clone(&res.values);
            let stop_update = Arc::clone(&res.stop_update);
            move || {
                let mut x: f64 = 1.0;
                while !stop_update.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(refresh_ms));
                    let mut guard = values.write().unwrap();
                    guard.push_back(x);
                    while guard.len() > MAX_SIZE {
                        guard.pop_front();
                    }
                    x += 1.0;
                }
            }
        });

        res
    }

    pub fn get_average_rate(&self) -> f64 {
        let guard = self.values.read().unwrap();
        guard.iter().sum::<f64>() / guard.len() as f64
    }
}

impl Drop for RateContract {
    fn drop(&mut self) {
        self.stop_update.store(true, Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::MockedBlockchain;
    use near_sdk::{testing_env, VMContext};

    fn get_context(input: Vec<u8>, is_view: bool) -> VMContext {
        VMContext {
            current_account_id: "alice.testnet".to_string(),
            signer_account_id: "robert.testnet".to_string(),
            signer_account_pk: vec![0, 1, 2],
            predecessor_account_id: "jane.testnet".to_string(),
            input,
            block_index: 0,
            block_timestamp: 0,
            account_balance: 0,
            account_locked_balance: 0,
            storage_usage: 0,
            attached_deposit: 0,
            prepaid_gas: 10u64.pow(18),
            random_seed: vec![0, 1, 2],
            is_view,
            output_data_receivers: vec![],
            epoch_height: 19,
        }
    }

    #[test]
    fn get_rate_nan() {
        let context = get_context(vec![], false);
        testing_env!(context);
        let refresh_interval_ms = 100;
        let contract = RateContract::new(refresh_interval_ms);

        // []
        thread::sleep(Duration::from_millis(refresh_interval_ms / 2));
        assert_eq!(true, f64::is_nan(contract.get_average_rate()));

        // [1.0]
        thread::sleep(Duration::from_millis(refresh_interval_ms));
        assert_eq!(1.0, contract.get_average_rate());

        // [1.0, 2.0]
        thread::sleep(Duration::from_millis(refresh_interval_ms));
        assert_eq!(1.5, contract.get_average_rate());

        // [1.0, 2.0, 3.0]
        thread::sleep(Duration::from_millis(refresh_interval_ms));
        assert_eq!(2.0, contract.get_average_rate());

        // [1.0, 2.0, 3.0, 4.0]
        thread::sleep(Duration::from_millis(refresh_interval_ms));
        assert_eq!(2.5, contract.get_average_rate());

        // [1.0, 2.0, 3.0, 4.0, 5.0]
        thread::sleep(Duration::from_millis(refresh_interval_ms));
        assert_eq!(3.0, contract.get_average_rate());

        // [2.0, 3.0, 4.0, 5.0, 6.0]
        thread::sleep(Duration::from_millis(refresh_interval_ms));
        assert_eq!(4.0, contract.get_average_rate());

        // [3.0, 4.0, 5.0, 6.0, 7.0]
        thread::sleep(Duration::from_millis(refresh_interval_ms));
        assert_eq!(5.0, contract.get_average_rate());
    }
}