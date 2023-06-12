use std::{collections::HashMap, fmt};

use bitcoin::OutPoint;

use super::{mint::Brc20MintTx, Brc20TransferTx};

#[derive(Debug, Clone)]
pub struct UserBalance {
  overall_balance: f64,
  active_transfer_inscriptions: HashMap<OutPoint, Brc20TransferTx>,
  transfer_sends: Vec<Brc20TransferTx>,
  transfer_receives: Vec<Brc20TransferTx>,
  mints: Vec<Brc20MintTx>,
}

impl UserBalance {}

impl UserBalance {
  pub fn new(overall_balance: f64) -> Self {
    UserBalance {
      overall_balance,
      active_transfer_inscriptions: HashMap::new(),
      transfer_sends: Vec::new(),
      transfer_receives: Vec::new(),
      mints: Vec::new(),
    }
  }

  pub fn get_transferable_balance(&self) -> f64 {
    self
      .active_transfer_inscriptions
      .values()
      .map(|inscription| inscription.get_amount())
      .sum()
  }

  pub fn get_available_balance(&self) -> f64 {
    self.overall_balance - self.get_transferable_balance()
  }

  pub fn get_overall_balance(&self) -> f64 {
    self.overall_balance
  }

  pub fn increase_overall_balance(&mut self, amount: f64) {
    self.overall_balance += amount;
  }

  pub fn decrease_overall_balance(&mut self, amount: f64) -> Result<(), String> {
    if self.overall_balance >= amount {
      self.overall_balance -= amount;
      Ok(())
    } else {
      Err("Decrease amount exceeds overall balance".to_string())
    }
  }
  pub fn add_transfer_inscription(&mut self, transfer_inscription: Brc20TransferTx) {
    self.active_transfer_inscriptions.insert(
      transfer_inscription.get_inscription_outpoint(),
      transfer_inscription.clone(),
    );
  }

  pub fn is_active_inscription(&self, outpoint: &OutPoint) -> bool {
    self.active_transfer_inscriptions.contains_key(&outpoint)
  }

  pub fn remove_inscription(&mut self, outpoint: &OutPoint) -> Option<Brc20TransferTx> {
    self.active_transfer_inscriptions.remove(&outpoint)
  }

  pub fn add_mint_tx(&mut self, mint: Brc20MintTx) {
    self.mints.push(mint);
  }

  pub fn get_mint_txs(&self) -> &Vec<Brc20MintTx> {
    &self.mints
  }

  // get active transfer inscriptions
  pub fn get_active_transfer_inscriptions(&self) -> &HashMap<OutPoint, Brc20TransferTx> {
    &self.active_transfer_inscriptions
  }

  // pub fn get_transfer_sends(&self) -> &Vec<Brc20TransferTx> {
  //   &self.transfer_sends
  // }

  // pub fn get_transfer_receives(&self) -> &Vec<Brc20TransferTx> {
  //   &self.transfer_receives
  // }

  pub fn add_transfer_send(&mut self, transfer_send: Brc20TransferTx) {
    self
      .decrease_overall_balance(transfer_send.get_amount())
      .unwrap();

    self.transfer_sends.push(transfer_send);
  }

  pub fn add_transfer_receive(&mut self, transfer_receive: Brc20TransferTx) {
    self.increase_overall_balance(transfer_receive.get_amount());

    self.transfer_receives.push(transfer_receive);
  }

  // get total amount of transfer sends
  pub fn get_total_amount_from_transfer_sends(&self) -> f64 {
    self
      .transfer_sends
      .iter()
      .map(|transfer_send| transfer_send.get_amount())
      .sum()
  }

  // get total amount of transfer receives
  pub fn get_total_amount_from_transfer_receives(&self) -> f64 {
    self
      .transfer_receives
      .iter()
      .map(|transfer_receive| transfer_receive.get_amount())
      .sum()
  }

  // get total amount of mints
  pub fn get_total_amount_from_mints(&self) -> f64 {
    self.mints.iter().map(|mint| mint.get_amount()).sum::<f64>()
  }

  // get overall balance using transfer sends, transfer receives and mints
  pub fn get_overall_balance_from_txs(&self) -> f64 {
    self.get_total_amount_from_transfer_receives() - self.get_total_amount_from_transfer_sends()
      + self.get_total_amount_from_mints()
  }

  // get available balance using get_overall_balance_from_txs and active transfer inscriptions
  pub fn get_available_balance_from_txs(&self) -> f64 {
    self.get_overall_balance_from_txs() - self.get_transferable_balance()
  }

  // get available balance using get_overall_balance_from_txs and get_transferable_balance
}

impl fmt::Display for UserBalance {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    writeln!(f, "Overall Balance: {}", self.overall_balance)?;
    writeln!(f, "Active Transfer Inscriptions:")?;
    for (outpoint, transfer_tx) in &self.active_transfer_inscriptions {
      writeln!(f, "OutPoint: {}\n{}", outpoint, transfer_tx)?;
    }
    Ok(())
  }
}

// #[derive(Debug, Clone)]
// pub struct Brc20Holder {
//   address: Address,
//   balance: UserBalance,
// }

// impl Brc20Holder {
//   pub fn new(address: Address, initial_balance: u64) -> Self {
//     Self {
//       address,
//       balance: UserBalance::new(initial_balance),
//     }
//   }

//   pub fn get_address(&self) -> &Address {
//     &self.address
//   }

//   pub fn get_user_balance(&self) -> &UserBalance {
//     &self.balance
//   }
// }

// impl fmt::Display for Brc20Holder {
//   fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//     write!(f, "Address: {}, Balance: {}", self.address, self.balance)
//   }
// }
