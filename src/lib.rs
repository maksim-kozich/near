use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::Vector;
use near_sdk::{env, near_bindgen};

use std::collections::HashMap;

near_sdk::setup_alloc!();

const MAX_SIZE: usize = 5;

const CMC_PRO_API_QUOTES_URI: &str =
    "https://pro-api.coinmarketcap.com/v1/cryptocurrency/quotes/latest";
const CMC_PRO_API_KEY: &str = "b89d1f7b-2ada-4334-9545-c6ce17e88698";
const CMC_SYMBOL: &str = "BTC";
const CMC_CURRENCY: &str = "USD";
const CMC_TIMEOUT_SECS: u64 = 5;

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

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
pub struct RateContract {
    values: Vector<f64>,
}

impl Default for RateContract {
    fn default() -> Self {
        Self {
            values: Vector::new(b"v".to_vec()),
        }
    }
}

#[near_bindgen]
impl RateContract {
    pub fn refresh(&mut self) {
        match self.get_rate() {
            Ok(rate) => {
                self.values.push(&rate);
                if self.values.len() as usize > MAX_SIZE {
                    let new_values: Vec<f64> = self.values.iter().skip(self.values.len() as usize - MAX_SIZE).collect();
                    self.values.clear();
                    self.values.extend(new_values);
                }
            }
            Err(err) => {
                let log_message = format!("Failed to get rate: {}", err);
                env::log(log_message.as_bytes());
            }
        }
    }

    pub fn get_num(&self) -> f64 {
        self.values.iter().sum::<f64>() / self.values.len() as f64
    }

    fn get_rate(&self) -> Result<f64, String> {
        match ureq::get(CMC_PRO_API_QUOTES_URI)
            .set("X-CMC_PRO_API_KEY", CMC_PRO_API_KEY)
            .query("symbol", CMC_SYMBOL)
            .timeout(std::time::Duration::from_secs(CMC_TIMEOUT_SECS))
            .call()
        {
            Ok(response) => {
                let response = response
                    .into_json::<CmcResponse>()
                    .map_err(|err| format!("CMC response deserialize error: {}", err))?;
                let data_item = response
                    .data
                    .get(CMC_SYMBOL)
                    .ok_or_else(|| format!("CMC symbol {} not found in response", CMC_SYMBOL))?;
                let quote = data_item.quote.get(CMC_CURRENCY).ok_or_else(|| {
                    format!("CMC currency {} not found in response", CMC_CURRENCY)
                })?;
                Ok(quote.price)
            }
            Err(ureq::Error::Status(code, _response)) => {
                Err(format!("non-200 response status: {}", code))
            }
            Err(err) => Err(format!("some kind of io/transport error: {}", err)),
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
        let contract = RateContract::default();

        // []
        assert_eq!(true, f64::is_nan(contract.get_num()));
    }
}
