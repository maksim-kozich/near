use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use eyre::Result;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{near_bindgen, env};

near_sdk::setup_alloc!();

const MAX_SIZE: usize = 5;

mod cmc {
    const CMC_PRO_API_QUOTES_URI: &str = "https://pro-api.coinmarketcap.com/v1/cryptocurrency/quotes/latest";
    const CMC_PRO_API_KEY: &str = "b89d1f7b-2ada-4334-9545-c6ce17e88698";
    const CMC_SYMBOL: &str = "BTC";
    const CMC_CURRENCY: &str = "USD";
    const CMC_TIMEOUT_SECS: u64 = 5;

    use ureq::Error;
    use std::collections::HashMap;

    #[derive(serde::Serialize)]
    pub struct CmcRateProvider;

    impl super::RateProvider for CmcRateProvider {
        fn get_rate(&mut self) -> super::Result<f64> {
            match ureq::get(CMC_PRO_API_QUOTES_URI)
                .set("X-CMC_PRO_API_KEY", CMC_PRO_API_KEY)
                .query("symbol", CMC_SYMBOL)
                .timeout(super::Duration::from_secs(CMC_TIMEOUT_SECS))
                .call() {
                Ok(response) => {
                    let response = response.into_json::<CmcResponse>()
                        .map_err(eyre::Report::from)?;
                    let data_item = response.data.get(CMC_SYMBOL)
                        .ok_or_else(|| eyre::eyre!("CMC symbol {} not found in response", CMC_SYMBOL))?;
                    let quote = data_item.quote.get(CMC_CURRENCY)
                        .ok_or_else(|| eyre::eyre!("CMC currency {} not found in response", CMC_CURRENCY))?;
                    Ok(quote.price)
                },
                Err(Error::Status(code, _response)) => {
                    Err(eyre::eyre!("non-200 response status: {}", code))
                }
                Err(err) => {
                    Err(eyre::eyre!("some kind of io/transport error: {}", err))
                }
            }
        }
    }

    #[derive(serde::Deserialize)]
    pub struct CmcResponse {
        pub data: HashMap<String, CmcDataItem>,
    }

    #[derive(serde::Deserialize)]
    pub struct CmcDataItem {
        pub quote: HashMap<String, CmcQuote>,
    }

    #[derive(serde::Deserialize)]
    pub struct CmcQuote {
        pub price: f64,
    }
}

pub trait RateProvider {
    fn get_rate(&mut self) -> Result<f64>;
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
pub struct RateContract {
    #[borsh_skip]
    values: Arc<RwLock<VecDeque<f64>>>,
    #[borsh_skip]
    stop_update: Arc<AtomicBool>,
}

impl Default for RateContract {
    fn default() -> Self {
        let v = VecDeque::new();
        Self {
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

        // let mut rate_provider = MockRateProvider {
        //     rates: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0].into()
        // };
        let mut rate_provider = cmc::CmcRateProvider;
        let values = Arc::clone(&res.values);
        let stop_update = Arc::clone(&res.stop_update);
        let _handle = std::thread::spawn(move || {
            while !stop_update.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(refresh_ms));

                match rate_provider.get_rate() {
                    Ok(rate) => {
                        let mut guard = values.write().unwrap();
                        guard.push_back(rate);
                        while guard.len() > MAX_SIZE {
                            guard.pop_front();
                        }
                    },
                    Err(err) => {
                        let log_message = format!("Failed to get rate: {}", err);
                        env::log(log_message.as_bytes());
                    }
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

#[derive(serde::Serialize)]
struct MockRateProvider {
    pub rates: VecDeque<f64>
}

impl RateProvider for MockRateProvider {
    fn get_rate(&mut self) -> Result<f64> {
        if let Some(x) = self.rates.pop_front() {
            self.rates.push_back(x);
            Ok(x)
        } else {
            Err(eyre::eyre!("provider is empty"))
        }
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
    fn get_rate() {
        let context = get_context(vec![], false);
        testing_env!(context);
        let refresh_interval_ms = 10;
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