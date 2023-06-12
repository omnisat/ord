use std::{collections::HashMap, fmt};

use super::utils::convert_to_float;
use serde::{Deserialize, Serialize};

use super::{
  brc20_ticker::Brc20Ticker,
  brc20_tx::{Brc20Tx, InvalidBrc20Tx, InvalidBrc20TxMap},
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Brc20Deploy {
  pub p: String,
  pub op: String,
  pub tick: String,
  pub max: String,
  pub lim: Option<String>,
  pub dec: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Brc20DeployTx {
  max_supply: f64,
  limit: f64,
  decimals: u8,
  brc20_tx: Brc20Tx,
  deploy_script: Brc20Deploy,
  is_valid: bool,
}

impl Brc20DeployTx {
  pub fn new(brc20_tx: Brc20Tx, deploy_script: Brc20Deploy) -> Self {
    Brc20DeployTx {
      max_supply: 0.0,
      limit: 0.0,
      decimals: 18,
      brc20_tx,
      deploy_script,
      is_valid: false,
    }
  }

  // getters and setters
  pub fn get_max_supply(&self) -> f64 {
    self.max_supply
  }

  pub fn get_limit(&self) -> f64 {
    self.limit
  }

  pub fn get_decimals(&self) -> u8 {
    self.decimals
  }

  pub fn is_valid(&self) -> bool {
    self.is_valid
  }

  pub fn set_valid(mut self, is_valid: bool) -> Self {
    self.is_valid = is_valid;
    self
  }

  pub fn get_deploy_script(&self) -> &Brc20Deploy {
    &self.deploy_script
  }

  pub fn get_brc20_tx(&self) -> &Brc20Tx {
    &self.brc20_tx
  }

  pub fn validate_deploy_script(
    mut self,
    invalid_tx_map: &mut InvalidBrc20TxMap,
    ticker_map: &HashMap<String, Brc20Ticker>,
  ) -> Self {
    let mut reason = String::new();

    // Check if the ticker symbol already exists in the ticker map
    let ticker_symbol = self.deploy_script.tick.to_lowercase();
    if ticker_map.contains_key(&ticker_symbol) {
      // Handle the case when the ticker symbol already exists
      reason = "Ticker symbol already exists".to_string();
    } else if self.deploy_script.tick.chars().count() != 4 {
      // Handle the case when the ticker symbol is not 4 characters long
      reason = "Ticker symbol must be 4 characters long".to_string();
    }

    // Check if the "dec" field is a valid number 18 or less
    if let Some(decimals) = &self.deploy_script.dec {
      let decimals = decimals.parse::<u8>().unwrap();
      if decimals > 18 {
        reason = "Decimals must be 18 or less".to_string();
      } else {
        self.decimals = decimals;
      }
    }

    // Check if the "max" field is valid
    let max = convert_to_float(&self.deploy_script.max, self.decimals);
    match max {
      Ok(max) => {
        if max == 0.0 {
          reason = "Max supply must be greater than 0".to_string();
        } else {
          self.max_supply = max;
        }
      }
      Err(e) => {
        reason = e.to_string();
      }
    }

    // Check if the "lim" field is valid, if not set, set to max supply
    let limit = convert_to_float(
      &self
        .deploy_script
        .lim
        .clone()
        .unwrap_or_else(|| "0".to_string()),
      self.decimals,
    );
    match limit {
      Ok(limit) => {
        if limit > self.max_supply {
          reason = "Limit must be less than or equal to max supply".to_string();
        } else if limit == 0.0 {
          self.limit = self.max_supply;
        } else {
          self.limit = limit;
        }
      }
      Err(e) => {
        reason = e.to_string();
      }
    }

    // Update the validity of the Brc20DeployTx based on the reason
    let valid_deploy_tx = self.set_valid(reason.is_empty());

    // Add the Brc20DeployTx to the invalid transaction map if necessary
    if !valid_deploy_tx.is_valid() {
      let invalid_tx = InvalidBrc20Tx::new(valid_deploy_tx.get_brc20_tx().clone(), reason);
      invalid_tx_map.add_invalid_tx(invalid_tx);
    }

    valid_deploy_tx
  }
}

impl fmt::Display for Brc20DeployTx {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    writeln!(f, "Deploy Transaction: {}", self.brc20_tx)?;
    writeln!(f, "Deploy Script: {:#?}", self.deploy_script)?;
    writeln!(f, "Is Valid: {}", self.is_valid)?;
    Ok(())
  }
}
