use std::collections::HashMap;

use bitcoin::Address;

use super::{
  deploy::Brc20DeployTx, mint::Brc20MintTx, transfer::Brc20TransferTx, user_balance::UserBalance,
};

#[derive(Debug, Clone)]
pub struct Brc20Ticker {
  ticker: String,
  limit: f64,
  max_supply: f64,
  total_minted: f64,
  decimals: u8,
  deploy_tx: Brc20DeployTx,
  mints: Vec<Brc20MintTx>,
  transfers: Vec<Brc20TransferTx>,
  balances: HashMap<Address, UserBalance>,
}

impl Brc20Ticker {
  pub fn new(deploy_tx: Brc20DeployTx) -> Brc20Ticker {
    let ticker = deploy_tx.get_deploy_script().tick.clone();
    let limit = deploy_tx.get_limit();
    let max_supply = deploy_tx.get_max_supply();
    let decimals = deploy_tx.get_decimals();

    Brc20Ticker {
      ticker,
      limit,
      max_supply,
      total_minted: 0.0,
      decimals,
      deploy_tx,
      mints: Vec::new(),
      transfers: Vec::new(),
      balances: HashMap::new(),
    }
  }

  pub fn add_mint(&mut self, mint: Brc20MintTx) {
    let owner = mint.get_brc20_tx().get_owner();
    // add mint to UserBalance
    if let Some(balance) = self.balances.get_mut(&owner) {
      balance.add_mint_tx(mint.clone());
    } else {
      let mut user_balance = UserBalance::new();
      user_balance.add_mint_tx(mint.clone());
      self.balances.insert(owner.clone(), user_balance);
    }
    self.mints.push(mint);
  }

  // pub fn add_mint(&mut self, mint: Brc20MintTx) {
  //   let amount = mint.get_amount();
  //   self.total_minted += amount;
  //   self.increase_user_overall_balance(mint.get_brc20_tx().get_owner(), amount);
  //   self.mints.push(mint);
  // }

  pub fn add_transfer(&mut self, transfer: Brc20TransferTx) {
    self.transfers.push(transfer);
  }

  // pub fn increase_user_overall_balance(&mut self, address: &Address, amount: f64) {
  //   if let Some(balance) = self.balances.get_mut(address) {
  //     balance.increase_overall_balance(amount);
  //   } else {
  //     let user_balance = UserBalance::new(amount);
  //     self.balances.insert(address.clone(), user_balance);
  //   }
  // }

  pub fn get_user_balance(&self, address: &Address) -> Option<UserBalance> {
    self.balances.get(address).cloned()
  }

  pub fn get_total_supply(&self) -> f64 {
    self.total_minted
  }

  // get total_minted from mints
  pub fn get_total_minted_from_mint_txs(&self) -> f64 {
    self.mints.iter().map(|mint| mint.get_amount()).sum()
  }

  pub fn get_mint_txs(&self) -> &[Brc20MintTx] {
    &self.mints
  }

  pub fn get_transfer_txs(&self) -> &[Brc20TransferTx] {
    &self.transfers
  }

  pub fn get_ticker(&self) -> String {
    self.ticker.to_lowercase()
  }

  pub fn get_decimals(&self) -> u8 {
    self.decimals
  }

  pub fn get_limit(&self) -> f64 {
    self.limit
  }

  pub fn get_max_supply(&self) -> f64 {
    self.max_supply
  }

  pub fn get_deploy_tx(&self) -> &Brc20DeployTx {
    &self.deploy_tx
  }

  // pub fn get_all_holders_with_balances(&self) -> Vec<(&Address, f64)> {
  //   self
  //     .balances
  //     .iter()
  //     .map(|(address, balance)| (address, balance.get_overall_balance()))
  //     .collect()
  // }

  pub fn display_brc20_ticker(&self) {
    println!("Deploy Transaction:\n{}", self.deploy_tx);

    println!("Mints:");
    for mint in &self.mints {
      println!("{}", mint);
    }

    println!("Transfers:");
    for transfer in &self.transfers {
      println!("{}", transfer);
    }

    println!("Total Minted: {}", self.total_minted);

    println!("Balances:");
    for (address, balance) in &self.balances {
      println!("Address: {}", address);
      println!("Overall Balance: {}", balance.get_overall_balance());

      println!("Active Transfer Inscriptions:");
      for (outpoint, transfer) in balance.get_active_transfer_inscriptions() {
        println!("OutPoint: {:?}", outpoint);
        println!("{}", transfer);
      }

      println!("=========================");
    }
  }
}
