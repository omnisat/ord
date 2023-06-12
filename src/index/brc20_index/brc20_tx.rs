use std::{collections::HashMap, fmt};

use bitcoin::{Address, OutPoint, Txid};
use bitcoincore_rpc::bitcoincore_rpc_json::{GetRawTransactionResult, GetRawTransactionResultVin};

#[derive(Debug, Clone)]
pub struct Brc20Tx {
  tx_id: Txid,
  vout: u32,
  blocktime: u64,
  owner: Address,
  inputs: Vec<GetRawTransactionResultVin>,
}

impl Brc20Tx {
  pub fn new(
    raw_tx_result: GetRawTransactionResult,
    owner: Address,
  ) -> Result<Self, Box<dyn std::error::Error>> {
    let tx_id = raw_tx_result.txid;
    let vout = raw_tx_result.vout[0].n;

    // Get the blocktime from the raw transaction result
    let blocktime = raw_tx_result
      .blocktime
      .ok_or_else(|| "Blocktime not found in raw transaction result")?;

    // Create the Brc20Tx instance
    let brc20_tx = Brc20Tx {
      tx_id,
      vout,
      blocktime: blocktime as u64,
      owner,
      inputs: raw_tx_result.vin,
    };

    Ok(brc20_tx)
  }

  // Getters
  pub fn get_tx_id(&self) -> Txid {
    self.tx_id
  }

  pub fn get_vout(&self) -> u32 {
    self.vout
  }

  //get OutPoint
  pub fn get_outpoint(&self) -> OutPoint {
    OutPoint {
      txid: self.tx_id,
      vout: self.vout,
    }
  }

  pub fn get_blocktime(&self) -> u64 {
    self.blocktime
  }

  // get address as reference
  pub fn get_owner(&self) -> &Address {
    &self.owner
  }

  pub fn get_inputs(&self) -> Vec<GetRawTransactionResultVin> {
    self.inputs.clone()
  }

  pub fn get_inputs_len(&self) -> usize {
    self.inputs.len()
  }

  pub fn get_inputs_is_empty(&self) -> bool {
    self.inputs.is_empty()
  }
}

impl fmt::Display for Brc20Tx {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    writeln!(f, "Transaction ID: {}", self.tx_id)?;
    writeln!(f, "Vout: {}", self.vout)?;
    writeln!(f, "Blocktime: {}", self.blocktime)?;
    writeln!(f, "Owner: {}", self.owner)?;
    writeln!(f, "Inputs: {:?}", self.inputs)?;
    Ok(())
  }
}

#[derive(Debug, Clone)]
pub struct InvalidBrc20Tx {
  pub brc20_tx: Brc20Tx,
  pub reason: String,
}

impl<'a> InvalidBrc20Tx {
  pub fn new(brc20_tx: Brc20Tx, reason: String) -> Self {
    InvalidBrc20Tx { brc20_tx, reason }
  }
}

impl fmt::Display for InvalidBrc20Tx {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    writeln!(f, "Brc20 Transaction: {}", self.brc20_tx)?;
    writeln!(f, "Reason: {}", self.reason)?;
    Ok(())
  }
}

#[derive(Debug)]
pub struct InvalidBrc20TxMap {
  map: HashMap<Txid, InvalidBrc20Tx>,
}

impl<'a> InvalidBrc20TxMap {
  pub fn new() -> Self {
    InvalidBrc20TxMap {
      map: HashMap::new(),
    }
  }

  pub fn add_invalid_tx(&mut self, invalid_tx: InvalidBrc20Tx) {
    let tx_id = invalid_tx.brc20_tx.tx_id;
    self.map.insert(tx_id, invalid_tx);
  }
}
