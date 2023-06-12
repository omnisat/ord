use super::*;
use bitcoincore_rpc::bitcoincore_rpc_json::{GetRawTransactionResult, GetRawTransactionResultVin};
use mongodb::bson::{doc, Document};
use mongodb::{bson, options::ClientOptions, Client};
use std::{fmt, str};

#[derive(Serialize, Deserialize)]
pub struct Output {
  pub inscription: InscriptionId,
  pub location: SatPoint,
  pub explorer: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Brc20Tx {
  pub tx_id: Txid,
  pub vout: u32,
  pub blocktime: u64,
  pub owner: Address,
  pub inputs: Vec<GetRawTransactionResultVin>,
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

    // Perform any additional processing or validation if needed

    Ok(brc20_tx)
  }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Brc20MintTx {
  pub brc20_tx: Brc20Tx,
  pub mint: Brc20MintTransfer,
  pub amount: u64,
  pub is_valid: bool,
}

impl Brc20MintTx {
  pub fn new(brc20_tx: Brc20Tx, mint: Brc20MintTransfer) -> Self {
    let amount = mint.amt.parse::<u64>().unwrap();
    Brc20MintTx {
      brc20_tx,
      mint,
      amount,
      is_valid: false,
    }
  }

  pub fn validate(
    mut self,
    invalid_tx_map: &mut InvalidBrc20TxMap,
    ticker_map: &HashMap<String, Brc20Ticker>,
  ) -> Self {
    let mut reason = String::new();

    // Get the ticker from the ticker map
    if let Some(ticker) = ticker_map.get(&self.mint.tick) {
      // Get the "lim" and "max" fields from the deploy script
      let limit: u64 = ticker.deploy_tx.deploy_script.lim.parse().unwrap_or(0);
      let max: u64 = ticker.deploy_tx.deploy_script.max.parse().unwrap_or(0);

      // Calculate the total minted amount
      let total_minted = ticker.total_minted + self.amount;

      // Check if the mint amount is greater than the deploy script's "lim" field
      if self.amount > limit {
        reason = "Mint amount exceeds limit".to_string();
      }

      // Check if the requsted mint amount + total minted amount exceeds the deploy script's "max" field
      if total_minted + self.amount > max {
        // Adjust the mint amount to mint the remaining tokens
        self.amount = max - ticker.total_minted;
        reason = format!(
          "Total minted amount exceeds maximum. Adjusted mint amount to {}",
          self.amount
        );
      }
    } else {
      reason = "Ticker symbol does not exist".to_string();
    }

    // Update the validity of the Brc20MintTx based on the validation result
    self.is_valid = reason.is_empty();

    // Add the Brc20MintTx to the invalid transaction map if necessary
    if !self.is_valid {
      let invalid_tx = InvalidBrc20Tx::new(self.brc20_tx.clone(), reason);
      invalid_tx_map.add_invalid_tx(invalid_tx);
    }

    self
  }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Brc20TransferTx {
  pub inscription_tx: Brc20Tx,
  pub transfer_tx: Option<Brc20Tx>,
  pub transfer_script: Brc20MintTransfer,
  pub amount: u64,
  pub receiver: Option<Address>,
  pub is_valid: bool,
}

impl Brc20TransferTx {
  pub fn new(inscription_tx: Brc20Tx, transfer_script: Brc20MintTransfer) -> Self {
    let amount = transfer_script.amt.parse::<u64>().unwrap_or(0);
    Brc20TransferTx {
      inscription_tx,
      transfer_tx: None,
      transfer_script,
      amount,
      receiver: None,
      is_valid: false,
    }
  }

  pub fn handle_inscribe_transfer_amount(
    self,
    ticker_map: &mut HashMap<String, Brc20Ticker>,
    invalid_tx_map: &mut InvalidBrc20TxMap,
  ) -> Self {
    let mut reason = String::new();
    let mut transfer_tx = self.clone();

    // Check if the ticker symbol exists
    if let Some(ticker) = ticker_map.get_mut(&self.transfer_script.tick) {
      // Get the transfer amount
      let transfer_amount = self.transfer_script.amt.parse::<u64>().unwrap_or(0);

      // Check if the user balance exists
      if let Some(user_balance) = ticker.balances.get_mut(&self.inscription_tx.owner) {
        let available_balance = user_balance.get_available_balance();

        if available_balance >= transfer_amount {
          // Set the validity of the transfer
          transfer_tx = self.set_validity(true);
          println!(
            "VALID: Transfer inscription added. Owner: {:#?}",
            transfer_tx.inscription_tx.owner
          );

          // Increase the transferable balance of the sender
          user_balance.add_transfer_inscription(transfer_tx.clone());
        } else {
          reason = "Transfer amount exceeds available balance".to_string();
        }
      } else {
        reason = "User balance not found".to_string();
      }
    } else {
      reason = "Ticker not found".to_string();
    }

    // Add the invalid transaction to the map if necessary
    if !transfer_tx.is_valid {
      let invalid_tx = InvalidBrc20Tx::new(transfer_tx.clone().inscription_tx, reason);
      invalid_tx_map.add_invalid_tx(invalid_tx);
    }

    transfer_tx
  }

  /// Sets the validity of the transfer.
  ///
  /// # Arguments
  ///
  /// * `is_valid` - A bool indicating the validity of the transfer.
  pub fn set_validity(mut self, is_valid: bool) -> Self {
    self.is_valid = is_valid;
    self
  }

  /// Sets the transfer transaction.
  ///
  /// # Arguments
  ///
  /// * `transfer_tx` - An optional `Brc20Tx` representing the transfer (second) transaction.
  pub fn set_transfer_tx(mut self, transfer_tx: Brc20Tx) -> Self {
    self.transfer_tx = Some(transfer_tx);
    self
  }

  /// Sets the receiver address.
  ///
  /// # Arguments
  ///
  /// * `receiver` - An optional `Address` representing the receiver address.
  pub fn set_receiver(mut self, receiver: Address) -> Self {
    self.receiver = Some(receiver);
    self
  }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Brc20DeployTx {
  pub deploy_tx: Brc20Tx,
  pub deploy_script: Brc20Deploy,
  pub is_valid: bool,
}

impl Brc20DeployTx {
  pub fn new(deploy_tx: Brc20Tx, deploy_script: Brc20Deploy) -> Self {
    Brc20DeployTx {
      deploy_tx,
      deploy_script,
      is_valid: false,
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Brc20Deploy {
  pub p: String,
  pub op: String,
  pub tick: String,
  pub max: String,
  pub lim: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Brc20MintTransfer {
  pub p: String,
  pub op: String,
  pub tick: String,
  pub amt: String,
}

trait ToDocument {
  fn to_document(&self) -> Document;
}

impl ToDocument for Brc20Deploy {
  fn to_document(&self) -> Document {
    doc! {
        "p": &self.p,
        "op": &self.op,
        "tick": &self.tick,
        "max": &self.max,
        "lim": &self.lim,
    }
  }
}

impl ToDocument for Brc20MintTransfer {
  fn to_document(&self) -> Document {
    doc! {
        "p": &self.p,
        "op": &self.op,
        "tick": &self.tick,
        "amt": &self.amt,
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserBalance {
  overall_balance: u64,
  active_transfer_inscriptions: HashMap<OutPoint, Brc20TransferTx>,
  transfer_sends: HashMap<OutPoint, Brc20TransferTx>,
  transfer_receives: HashMap<OutPoint, Brc20TransferTx>,
}

impl UserBalance {
  pub fn new(overall_balance: u64) -> Self {
    UserBalance {
      overall_balance,
      active_transfer_inscriptions: HashMap::new(),
      transfer_sends: HashMap::new(),
      transfer_receives: HashMap::new(),
    }
  }

  pub fn get_transferable_balance(&self) -> u64 {
    self
      .active_transfer_inscriptions
      .values()
      .map(|inscription| inscription.amount)
      .sum()
  }

  pub fn get_available_balance(&self) -> u64 {
    self.overall_balance - self.get_transferable_balance()
  }

  pub fn get_overall_balance(&self) -> u64 {
    self.overall_balance
  }

  pub fn increase_overall_balance(&mut self, amount: u64) {
    self.overall_balance += amount;
  }

  pub fn decrease_overall_balance(&mut self, amount: u64) -> Result<(), String> {
    if self.overall_balance >= amount {
      self.overall_balance -= amount;
      Ok(())
    } else {
      Err("Decrease amount exceeds overall balance".to_string())
    }
  }
  pub fn add_transfer_inscription(&mut self, transfer_inscription: Brc20TransferTx) {
    let outpoint = OutPoint {
      txid: transfer_inscription.inscription_tx.tx_id,
      vout: transfer_inscription.inscription_tx.vout,
    };
    self
      .active_transfer_inscriptions
      .insert(outpoint, transfer_inscription);
  }

  pub fn add_transfer_send(&mut self, transfer_send: Brc20TransferTx) {
    let outpoint = OutPoint {
      txid: transfer_send.inscription_tx.tx_id,
      vout: transfer_send.inscription_tx.vout,
    };
    self.transfer_sends.insert(outpoint, transfer_send);
  }

  pub fn add_transfer_receive(&mut self, transfer_receive: Brc20TransferTx) {
    let outpoint = OutPoint {
      txid: transfer_receive.inscription_tx.tx_id,
      vout: transfer_receive.inscription_tx.vout,
    };
    self.transfer_receives.insert(outpoint, transfer_receive);
  }

  pub fn is_active_inscription(&self, outpoint: &OutPoint) -> bool {
    self.active_transfer_inscriptions.contains_key(&outpoint)
  }

  pub fn remove_inscription(&mut self, outpoint: &OutPoint) -> Option<Brc20TransferTx> {
    self.active_transfer_inscriptions.remove(&outpoint)
  }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Brc20Ticker {
  deploy_tx: Brc20DeployTx,
  mints: Vec<Brc20MintTx>,
  transfers: Vec<Brc20TransferTx>,
  total_minted: u64,
  balances: HashMap<Address, UserBalance>,
}

impl Brc20Ticker {
  pub fn new(deploy_tx: Brc20DeployTx) -> Self {
    Brc20Ticker {
      deploy_tx,
      mints: Vec::new(),
      transfers: Vec::new(),
      total_minted: 0,
      balances: HashMap::new(),
    }
  }

  pub fn add_mint(&mut self, mint: Brc20MintTx) {
    self.total_minted += mint.amount;
    self.increase_user_overall_balance(mint.brc20_tx.owner.clone(), mint.amount);
    self.mints.push(mint);
  }

  pub fn add_completed_transfer(&mut self, transfer: Brc20TransferTx) {
    self.transfers.push(transfer);
  }

  pub fn increase_user_overall_balance(&mut self, address: Address, amount: u64) {
    if let Some(balance) = self.balances.get_mut(&address) {
      balance.increase_overall_balance(amount);
    } else {
      let user_balance = UserBalance::new(amount);
      self.balances.insert(address, user_balance);
    }
  }

  pub fn get_user_balance(&self, address: &Address) -> Option<UserBalance> {
    self.balances.get(address).cloned()
  }

  pub fn get_total_minted(&self) -> u64 {
    self.total_minted
  }

  pub fn get_mints(&self) -> &[Brc20MintTx] {
    &self.mints
  }

  pub fn get_transfers(&self) -> &[Brc20TransferTx] {
    &self.transfers
  }
}

pub struct InvalidBrc20Tx {
  pub brc20_tx: Brc20Tx,
  pub reason: String,
}

impl InvalidBrc20Tx {
  pub fn new(brc20_tx: Brc20Tx, reason: String) -> Self {
    InvalidBrc20Tx { brc20_tx, reason }
  }
}

pub struct InvalidBrc20TxMap {
  map: HashMap<Txid, InvalidBrc20Tx>,
}

impl InvalidBrc20TxMap {
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

impl fmt::Display for Brc20Ticker {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    writeln!(f, "Deploy Transaction:\n{}", self.deploy_tx)?;
    writeln!(f, "Mint Transactions:")?;
    for mint in &self.mints {
      writeln!(f, "{}", mint)?;
    }
    writeln!(f, "Transfer Transactions:")?;
    for transfer in &self.transfers {
      writeln!(f, "{}", transfer)?;
    }
    writeln!(f, "Total Minted: {}", self.total_minted)?;
    writeln!(f, "Balances:")?;
    for (address, balance) in &self.balances {
      writeln!(f, "Address: {}\n{}", address, balance)?;
    }
    Ok(())
  }
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

impl fmt::Display for Brc20DeployTx {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    writeln!(f, "Deploy Transaction: {}", self.deploy_tx)?;
    writeln!(f, "Deploy Script: {:#?}", self.deploy_script)?;
    writeln!(f, "Is Valid: {}", self.is_valid)?;
    Ok(())
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

impl fmt::Display for Brc20MintTx {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    writeln!(f, "Brc20 Transaction: {}", self.brc20_tx)?;
    writeln!(f, "Mint: {:#?}", self.mint)?;
    writeln!(f, "Amount: {}", self.amount)?;
    writeln!(f, "Is Valid: {}", self.is_valid)?;
    Ok(())
  }
}

impl fmt::Display for Brc20TransferTx {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    writeln!(f, "Inscription Transaction: {}", self.inscription_tx)?;
    writeln!(f, "Transfer Transaction: {:?}", self.transfer_tx)?;
    writeln!(f, "Transfer Script: {:#?}", self.transfer_script)?;
    writeln!(f, "Amount: {}", self.amount)?;
    writeln!(f, "Receiver: {:?}", self.receiver)?;
    writeln!(f, "Is Valid: {}", self.is_valid)?;
    Ok(())
  }
}

pub(crate) fn index_brc20(index: &Index) -> Result<(), Box<dyn std::error::Error>> {
  // Initialize the runtime for asynchronous operations.
  let rt = Runtime::new()?;

  // Create a future that establishes a connection to the MongoDB server.
  let future = async {
    let connection_string = "mongodb://localhost:27017";
    let db_name = "omnisat";
    MongoClient::new(connection_string, db_name).await
  };

  // Establish a connection to the MongoDB server.
  let client = rt.block_on(future)?;

  // The key is the ticker symbol, and the value is the `Brc20Ticker` struct.
  let mut ticker_map: HashMap<String, Brc20Ticker> = HashMap::new();

  // The hashmap to store invalid transactions.
  let mut invalid_tx_map = InvalidBrc20TxMap::new();

  // Retrieve the inscriptions from the `Index` object.
  let inscriptions = index.get_inscriptions(None)?;

  // Iterate over the inscriptions.
  for (location, inscription_id) in inscriptions {
    // Retrieve the corresponding `Inscription` object.
    let i = index.get_inscription_by_id(inscription_id.clone())?;

    // Check if the `Inscription` object exists.
    if let Some(inscription) = i {
      // Check the content type of the `Inscription`.
      if let Some(ct) = inscription.content_type() {
        // Check if the content type is either JSON or plain text.
        if ct == "application/json" || ct == "text/plain;charset=utf-8" {
          // Parse the body content of the `Inscription` as a string.
          if let Some(inc) = inscription.body() {
            let parse_inc = str::from_utf8(inc)?;

            // Get the raw transaction info.
            let raw_tx_info = index
              .client
              .get_raw_transaction_info(&location.outpoint.txid, None)?;

            // Retrieve the inscription owner address
            let owner = get_owner_of_output(&location.outpoint, &raw_tx_info)?;

            // instantiate a new Brc20Tx struct
            let brc20_tx = Brc20Tx::new(raw_tx_info, owner)?;

            // Parse the body content as a `Brc20Deploy` struct.
            let deploy: Result<Brc20Deploy, _> = serde_json::from_str(parse_inc);
            if let Ok(deploy) = deploy {
              if deploy.op == "deploy" {
                let deploy_tx = Brc20DeployTx::new(brc20_tx, deploy.clone());

                // validate the deploy script
                let validated_deploy_tx =
                  validate_deploy_script(deploy_tx, &mut invalid_tx_map, &ticker_map);

                if validated_deploy_tx.is_valid {
                  println!("=========================");
                  println!("Deploy: {:?}", deploy);
                  // Insert the `Brc20Deploy` struct into the MongoDB collection.
                  let future = insert_document_into_brcs_collection(&client, deploy.clone());
                  rt.block_on(future)?;

                  // Instantiate a new `Brc20Ticker` struct and update the hashmap with the deploy information.
                  let ticker = Brc20Ticker::new(validated_deploy_tx.clone());
                  ticker_map.insert(
                    validated_deploy_tx.deploy_script.tick.to_lowercase(),
                    ticker,
                  );
                }
              }
            } else {
              // Parse the body content as a `Brc20MintTransfer` struct.
              let mint_transfer: Result<Brc20MintTransfer, _> = serde_json::from_str(parse_inc);
              if let Ok(mint_transfer) = mint_transfer {
                // if mint, then validate the mint operation
                if mint_transfer.op == "mint" {
                  // Instantiate a new `Brc20MintTx` struct.
                  let mint_tx = Brc20MintTx::new(brc20_tx, mint_transfer);

                  // Validate the mint operation.
                  let validated_mint_tx = mint_tx.validate(&mut invalid_tx_map, &ticker_map);

                  if validated_mint_tx.is_valid {
                    println!("=========================");
                    println!("Mint: {:?}", validated_mint_tx.mint);
                    println!("Owner Address: {:?}", validated_mint_tx.brc20_tx.owner);

                    // Get the ticker symbol.
                    let ticker_symbol = validated_mint_tx.mint.tick.to_lowercase();

                    // Retrieve the `Brc20Ticker` struct from the hashmap.
                    let ticker = ticker_map.get_mut(&ticker_symbol).unwrap();

                    // Update the ticker struct with the mint operation.
                    ticker.add_mint(validated_mint_tx.clone());

                    // Insert the `Brc20MintTransfer` struct into the MongoDB collection.
                    let future =
                      insert_document_into_brcs_collection(&client, validated_mint_tx.mint);
                    rt.block_on(future)?;
                  }
                } else if mint_transfer.op == "transfer" {
                  // Instantiate a new `BrcTransferTx` struct.
                  let mut brc20_transfer_tx = Brc20TransferTx::new(brc20_tx, mint_transfer.clone());

                  // call handle_inscribe_transfer_amount
                  brc20_transfer_tx = brc20_transfer_tx
                    .handle_inscribe_transfer_amount(&mut ticker_map, &mut invalid_tx_map);

                  // Check if the transfer is valid.
                  if brc20_transfer_tx.is_valid {
                    println!("=========================");
                    println!("Transfer: {:?}", brc20_transfer_tx.transfer_script);
                    println!(
                      "Owner Address: {:?}",
                      brc20_transfer_tx.inscription_tx.owner
                    );

                    // Insert the `Brc20MintTransfer` struct into the MongoDB collection.
                    let future = insert_document_into_brcs_collection(
                      &client,
                      brc20_transfer_tx.transfer_script,
                    );
                    rt.block_on(future)?;
                  } else {
                    // println!("Invalid transfer operation. Skipping...");
                    // process invalid transfers here
                  }
                }
              }
            }
          }
        }
      }
    }
  }
  // print hashmap
  println!("=========================");
  for (ticker_symbol, ticker) in &ticker_map {
    // Access the ticker symbol and ticker value here
    println!("Ticker Symbol: {}", ticker_symbol);
    display_brc20_ticker(ticker);
  }
  println!("=========================");

  Ok(())
}

pub(crate) fn get_owner_of_output(
  outpoint: &OutPoint,
  raw_tx_info: &GetRawTransactionResult,
) -> Result<Address, Box<dyn std::error::Error>> {
  // Get the controlling address of this output
  let script_pubkey = &raw_tx_info.vout[outpoint.vout as usize].script_pub_key;
  let this_address = Address::from_script(&script_pubkey.script().unwrap(), Network::Testnet)
    .map_err(|_| {
      println!("Couldn't derive address from scriptPubKey");
      "Couldn't derive address from scriptPubKey"
    })?;

  // println!("Script Pub Key: {:?}", script_pubkey.asm);

  Ok(this_address)
}

fn display_brc20_ticker(ticker: &Brc20Ticker) {
  println!("Deploy Transaction:\n{}", ticker.deploy_tx);

  println!("Mints:");
  for mint in &ticker.mints {
    println!("{}", mint);
  }

  println!("Transfers:");
  for transfer in &ticker.transfers {
    println!("{}", transfer);
  }

  println!("Total Minted: {}", ticker.total_minted);

  println!("Balances:");
  for (address, balance) in &ticker.balances {
    println!("Address: {}", address);
    println!("Overall Balance: {}", balance.overall_balance);

    println!("Active Transfer Inscriptions:");
    for (outpoint, transfer) in &balance.active_transfer_inscriptions {
      println!("OutPoint: {:?}", outpoint);
      println!("{}", transfer);
    }

    println!("=========================");
  }
}

fn validate_deploy_script(
  mut deploy_tx: Brc20DeployTx,
  invalid_tx_map: &mut InvalidBrc20TxMap,
  ticker_map: &HashMap<String, Brc20Ticker>,
) -> Brc20DeployTx {
  let mut reason = String::new();

  // Check if the ticker symbol already exists in the ticker map
  let ticker_symbol = deploy_tx.deploy_script.tick.to_lowercase();
  if ticker_map.contains_key(&ticker_symbol) {
    // Handle the case when the ticker symbol already exists
    reason = "Ticker symbol already exists".to_string();
  } else if deploy_tx.deploy_script.tick.chars().count() != 4 {
    // Handle the case when the ticker symbol is not 4 characters long
    reason = "Ticker symbol should be 4 characters long".to_string();
  }

  // Update the validity of the Brc20DeployTx based on the reason
  deploy_tx.is_valid = reason.is_empty();

  // Add the Brc20DeployTx to the invalid transaction map if necessary
  if !deploy_tx.is_valid {
    let invalid_tx = InvalidBrc20Tx::new(deploy_tx.deploy_tx.clone(), reason);
    invalid_tx_map.add_invalid_tx(invalid_tx);
  }

  deploy_tx
}

// fn find_sender_addresses(index: &Index, raw_tx_info: &GetRawTransactionResult) -> Vec<Address> {
//   let mut sender_addresses: Vec<Address> = Vec::new();

//   for input in &raw_tx_info.vin {
//     if let Some(prev_tx_info) = input
//       .txid
//       .as_ref()
//       .and_then(|txid| index.client.get_raw_transaction_info(&txid, None).ok())
//     {
//       let output_index = input.vout.unwrap();
//       if let Some(output) = prev_tx_info.vout.get(output_index as usize) {
//         if let Some(sender_address) = &output.script_pub_key.address {
//           sender_addresses.push(sender_address.clone());
//         }
//       }
//     }
//   }

//   sender_addresses
// }

pub(crate) fn handle_transaction(
  index: &Index,
  outpoint: &OutPoint,
) -> Result<(), Box<dyn std::error::Error>> {
  // Get the raw transaction info.
  let raw_tx_info = index
    .client
    .get_raw_transaction_info(&outpoint.txid, None)?;

  // Display the raw transaction info.
  display_raw_transaction_info(&raw_tx_info);

  // Get the transaction Inputs
  let inputs = &raw_tx_info.transaction()?.input;

  // Get the addresses and values of the inputs.
  let input_addresses_values = transaction_inputs_to_addresses_values(index, inputs)?;
  for (index, (address, value)) in input_addresses_values.iter().enumerate() {
    println!("Input Address {}: {}, Value: {}", index + 1, address, value);
  }

  // display_input_info(&raw_tx_info);

  println!("=====");
  // Get the transaction Outputs
  // let outputs = &raw_tx_info.transaction()?.output;

  // Get the addresses and values of the outputs.
  // let output_addresses_values = transaction_outputs_to_addresses_values(outputs)?;
  // for (index, (address, value)) in output_addresses_values.iter().enumerate() {
  //   println!(
  //     "Output Address {}: {}, Value: {}",
  //     index + 1,
  //     address,
  //     value
  //   );
  // }

  Ok(())
}

fn transaction_inputs_to_addresses_values(
  index: &Index,
  inputs: &Vec<TxIn>,
) -> Result<Vec<(Address, u64)>, Box<dyn std::error::Error>> {
  let mut addresses_values: Vec<(Address, u64)> = vec![];

  for input in inputs {
    let prev_output = input.previous_output;
    println!(
      "Input from transaction: {:?}, index: {:?}",
      prev_output.txid, prev_output.vout
    );

    let prev_tx_info = index
      .client
      .get_raw_transaction_info(&prev_output.txid, None)?;

    let prev_tx = prev_tx_info.transaction()?;

    let output = &prev_tx.output[usize::try_from(prev_output.vout).unwrap()];
    let script_pub_key = &output.script_pubkey;

    let address = Address::from_script(&script_pub_key, Network::Testnet).map_err(|_| {
      println!("Couldn't derive address from scriptPubKey");
      "Couldn't derive address from scriptPubKey"
    })?;

    // Add both the address and the value of the output to the list
    addresses_values.push((address, output.value));

    println!("=====");
  }

  if addresses_values.is_empty() {
    Err("Couldn't derive any addresses or values from scriptPubKeys".into())
  } else {
    Ok(addresses_values)
  }
}

fn transaction_outputs_to_addresses_values(
  outputs: &Vec<TxOut>,
) -> Result<Vec<(Address, u64)>, Box<dyn std::error::Error>> {
  let mut addresses_values: Vec<(Address, u64)> = vec![];

  for output in outputs {
    let script_pub_key = &output.script_pubkey;

    if let Ok(address) = Address::from_script(&script_pub_key, Network::Testnet) {
      // Add both the address and the value of the output to the list
      addresses_values.push((address, output.value));
    } else {
      println!("Couldn't derive address from scriptPubKey");
    }
  }

  if addresses_values.is_empty() {
    Err("Couldn't derive any addresses or values from scriptPubKeys".into())
  } else {
    Ok(addresses_values)
  }
}

fn display_raw_transaction_info(raw_transaction_info: &GetRawTransactionResult) {
  println!("Raw Transaction Information:");
  println!("----------------");
  println!("Txid: {:?}", raw_transaction_info.txid);
  println!("Hash: {:?}", raw_transaction_info.hash);
  println!("Size: {:?}", raw_transaction_info.size);
  println!("Vsize: {:?}", raw_transaction_info.vsize);
  println!("Version: {:?}", raw_transaction_info.version);
  println!("Locktime: {:?}", raw_transaction_info.locktime);
  println!("Blockhash: {:?}", raw_transaction_info.blockhash);
  println!("Confirmations: {:?}", raw_transaction_info.confirmations);
  println!("Time: {:?}", raw_transaction_info.time);
  println!("Blocktime: {:?}", raw_transaction_info.blocktime);
  println!();
}

fn display_input_info(raw_transaction_info: &GetRawTransactionResult) {
  println!("Inputs (Vin):");
  println!("-------------");
  for (i, vin) in raw_transaction_info.vin.iter().enumerate() {
    println!("Vin {}: {:?}", i + 1, vin);
    if let Some(txid) = &vin.txid {
      println!("  txid: {:?}", txid);
    }
    if let Some(vout) = vin.vout {
      println!("  vout: {:?}", vout);
    }
    if let Some(script_sig) = &vin.script_sig {
      println!("  script_sig: {:?}", script_sig);
    }
    if let Some(txinwitness) = &vin.txinwitness {
      println!("  txinwitness: {:?}", txinwitness);
    }
    if let Some(coinbase) = &vin.coinbase {
      println!("  coinbase: {:?}", coinbase);
    }
    println!("  sequence: {:?}", vin.sequence);
  }
  println!();
}

fn display_output_info(raw_transaction_info: &GetRawTransactionResult, vout_index: usize) {
  if let Some(vout) = raw_transaction_info.vout.get(vout_index) {
    println!("----------------------------------------------");
    println!("Vout {}", vout_index);
    println!("- Value: {:?}", vout.value);
    println!("- N: {:?}", vout.n);

    let script_pub_key = &vout.script_pub_key;
    println!("- Script Pub Key:");
    println!("    - ASM: {:?}", script_pub_key.asm);
    println!("    - Hex: {:?}", script_pub_key.hex);
    println!("    - Required Signatures: {:?}", script_pub_key.req_sigs);
    println!("    - Type: {:?}", script_pub_key.type_);
    println!("    - Addresses: {:?}", script_pub_key.addresses);
    println!("    - Address: {:?}", script_pub_key.address);

    println!();
  } else {
    println!("Invalid vout index: {}", vout_index);
  }
  println!();
}

/// The `insert_document_into_brcs_collection` function is responsible for inserting a document into the "brcs" collection in MongoDB.
///
/// # Arguments
///
/// * `client` - A `MongoClient` object representing the MongoDB client.
/// * `item` - An item that implements the `ToDocument` trait, which will be converted into a MongoDB document and inserted into the collection.
///
/// # Returns
///
/// This function returns a `Result` which is an enumeration representing either success (`Ok`) or failure (`Err`).
///
/// # Errors
///
/// This function will return an error if the document cannot be inserted into the MongoDB collection.
async fn insert_document_into_brcs_collection<T: ToDocument>(
  client: &MongoClient,
  item: T,
) -> Result<(), Box<dyn std::error::Error>> {
  // Convert the item into a MongoDB document.
  let document = item.to_document();

  // Insert the document into the "brcs" collection.
  client.insert_document("brcs", document).await?;

  // Return success.
  Ok(())
}

struct MongoClient {
  client: Client,
  db_name: String,
}

impl MongoClient {
  async fn new(connection_string: &str, db_name: &str) -> Result<Self, mongodb::error::Error> {
    let mut client_options = ClientOptions::parse(connection_string).await?;
    client_options.direct_connection = Some(true);
    let client = Client::with_options(client_options)?;

    Ok(Self {
      client,
      db_name: db_name.to_string(),
    })
  }

  async fn insert_document(
    &self,
    collection_name: &str,
    document: bson::Document,
  ) -> Result<(), mongodb::error::Error> {
    let db = self.client.database(&self.db_name);
    let collection = db.collection::<bson::Document>(collection_name);

    collection
      .insert_one(document, None)
      .await
      .expect("Could not insert document");

    Ok(())
  }
}
