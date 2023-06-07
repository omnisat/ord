use super::*;
use bitcoincore_rpc::bitcoincore_rpc_json::{GetRawTransactionResult, GetRawTransactionResultVin};
use mongodb::bson::{doc, Document};
use mongodb::{bson, options::ClientOptions, Client};
use std::str;

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
    raw_tx_result: &GetRawTransactionResult,
    owner: &Address,
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
      owner: owner.clone(),
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
  pub fn new(brc20_tx: Brc20Tx, mint: Brc20MintTransfer, amount: u64, is_valid: bool) -> Self {
    Brc20MintTx {
      brc20_tx,
      mint,
      amount,
      is_valid,
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Brc20TransferTx {
  pub brc20_tx: Brc20Tx,
  pub transfer: Brc20MintTransfer,
  pub amount: u64,
  pub owner: Address,
  pub is_inscription: bool,
  pub is_valid: bool,
}

impl Brc20TransferTx {
  pub fn new(
    brc20_tx: Brc20Tx,
    transfer: Brc20MintTransfer,
    owner: Address,
    is_inscription: bool,
  ) -> Self {
    let amount = transfer.amt.parse::<u64>().unwrap_or(0);
    Brc20TransferTx {
      brc20_tx,
      transfer,
      amount,
      owner,
      is_inscription,
      is_valid: false,
    }
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
  transfer_inscriptions: Vec<Brc20TransferTx>,
}

impl UserBalance {
  pub fn new(overall_balance: u64) -> Self {
    UserBalance {
      overall_balance,
      transfer_inscriptions: Vec::new(),
    }
  }

  pub fn get_overall_balance(&self) -> u64 {
    self.overall_balance
  }

  pub fn get_transferable_balance(&self) -> u64 {
    self
      .transfer_inscriptions
      .iter()
      .map(|inscription| inscription.amount)
      .sum()
  }

  pub fn get_available_balance(&self) -> u64 {
    self.overall_balance - self.get_transferable_balance()
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
    self.transfer_inscriptions.push(transfer_inscription);
  }

  pub fn is_active_inscription(&self, outpoint: &OutPoint) -> bool {
    self.transfer_inscriptions.iter().any(|inscription| {
      inscription.brc20_tx.tx_id == outpoint.txid && inscription.brc20_tx.vout == outpoint.vout
    })
  }

  pub fn remove_inscription(&mut self, outpoint: &OutPoint) -> Option<Brc20TransferTx> {
    if let Some(index) = self.transfer_inscriptions.iter().position(|inscription| {
      inscription.brc20_tx.tx_id == outpoint.txid && inscription.brc20_tx.vout == outpoint.vout
    }) {
      Some(self.transfer_inscriptions.remove(index))
    } else {
      None
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Brc20Ticker {
  deploy: Brc20Deploy,
  mints: Vec<Brc20MintTx>,
  transfers: Vec<Brc20TransferTx>,
  total_minted: u64,
  balances: HashMap<Address, UserBalance>,
}

impl Brc20Ticker {
  pub fn new(deploy: Brc20Deploy) -> Self {
    Brc20Ticker {
      deploy,
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

  pub fn add_transfer(&mut self, transfer: Brc20TransferTx) {
    self.transfers.push(transfer);
  }

  pub fn validate_mint(&self, mint: Brc20MintTransfer, brc20_tx: Brc20Tx) -> Brc20MintTx {
    let minted_amount: u64 = mint.amt.parse().unwrap_or(0);
    let limit: u64 = self.deploy.lim.parse().unwrap_or(0);
    let max: u64 = self.deploy.max.parse().unwrap_or(0);

    let is_valid = self.total_minted + minted_amount <= max && minted_amount <= limit;

    Brc20MintTx::new(brc20_tx, mint, minted_amount, is_valid)
  }

  pub fn increase_user_overall_balance(&mut self, address: Address, amount: u64) {
    if let Some(balance) = self.balances.get_mut(&address) {
      balance.increase_overall_balance(amount);
    } else {
      let user_balance = UserBalance::new(amount);
      self.balances.insert(address, user_balance);
    }
  }

  /// Handles the transfer operation.
  ///
  /// # Arguments
  ///
  /// * `brc20_tx` - The Brc20Tx struct representing the transaction information.
  /// * `mint_transfer` - The Brc20MintTransfer struct representing the transfer information.
  /// * `owner` - The address of the owner performing the transfer.
  ///
  /// # Returns
  ///
  /// * `Brc20TransferTx` - The Brc20TransferTx struct representing the transfer operation.
  pub fn handle_transfer(
    &mut self,
    brc20_tx: Brc20Tx,
    mint_transfer: Brc20MintTransfer,
    owner: &Address,
    inscription_id: InscriptionId,
  ) -> Brc20TransferTx {
    if brc20_tx.vout == 0 {
      // Inscribe Transfer
      return self.handle_inscribe_transfer(brc20_tx, mint_transfer, owner);
    } else {
      // Send Transfer
      return self.handle_send_transfer(&owner, brc20_tx, mint_transfer);
    }
  }

  /// Handles the transfer amount validation for an inscription transfer.
  /// It checks if the transfer amount does not exceed the available balance of the owner.
  ///
  /// # Arguments
  ///
  /// * `owner_address` - The address of the owner performing the inscription transfer.
  /// * `transfer` - The Brc20MintTransfer struct representing the transfer information.
  /// * `brc_transfer_tx` - Mutable reference to the Brc20TransferTx struct to update its validity.
  ///
  /// # Returns
  ///
  /// * `bool` - Indicates whether the inscription transfer amount is valid or not.
  pub fn handle_inscribe_transfer_amount(
    &mut self,
    brc_transfer_tx: Brc20TransferTx,
  ) -> Brc20TransferTx {
    // Get the transfer amount
    let transfer_amount = brc_transfer_tx.transfer.amt.parse::<u64>().unwrap_or(0);

    // Check if the user balance exists
    if let Some(sender_balance) = self.balances.get_mut(&brc_transfer_tx.owner) {
      let available_balance = sender_balance.get_available_balance();

      if available_balance >= transfer_amount {
        // Increase the transferable balance of the sender
        sender_balance.add_transfer_inscription(brc_transfer_tx);
        println!(
          "VALID: Transfer inscription added. Owner: {:#?}",
          brc_transfer_tx.owner
        );
      } else {
        println!("INVALID: Transfer amount exceeds available balance.");
      }
    } else {
      // User balance not found
      // println!("INVALID: User balance not found.");
    }
    brc_transfer_tx.set_validity(false)
  }

  pub fn handle_inscribe_transfer(
    &mut self,
    brc20_tx: Brc20Tx,
    mint_transfer: Brc20MintTransfer,
    owner_address: &Address,
  ) -> Brc20TransferTx {
    // Instantiate a new Brc20TransferTx struct
    let brc_transfer_tx: Brc20TransferTx =
      Brc20TransferTx::new(brc20_tx, mint_transfer, owner_address.clone(), true);

    // Check if the transfer amount is valid
    self.handle_inscribe_transfer_amount(brc_transfer_tx)
  }

  /// Handles the send transfer operation.
  ///
  /// # Arguments
  ///
  /// * `owner_address` - The address of the owner performing the transfer.
  /// * `sender_addresses` - The list of sender addresses involved in the transfer.
  /// * `transfer` - The Brc20MintTransfer struct representing the transfer information.
  ///
  /// # Returns
  ///
  /// * `Brc20TransferTx` - The Brc20TransferTx struct representing the transfer operation.
  pub fn handle_send_transfer(
    &mut self,
    owner_address: &Address,
    brc20_tx: Brc20Tx,
    transfer: Brc20MintTransfer,
  ) -> Brc20TransferTx {
    let transfer_amount = transfer.amt.parse::<u64>().unwrap_or(0);

    // Instantiate a new Brc20TransferTx struct
    let mut brc_transfer_tx: Brc20TransferTx =
      Brc20TransferTx::new(brc20_tx, transfer, owner_address.clone(), false);

    for sender_address in &brc_transfer_tx.brc20_tx.sender_addresses {
      if let Some(sender_balance) = self.balances.get_mut(&sender_address) {
        let transferable_balance = sender_balance.get_transferable_balance();
        if transferable_balance >= transfer_amount {
          sender_balance.decrease_overall_balance(transfer_amount);
          sender_balance.decrease_transferable_balance(transfer_amount);
          if let Some(owner_balance) = self.balances.get_mut(owner_address) {
            owner_balance.increase_overall_balance(transfer_amount);
          } else {
            let user_balance = UserBalance::new(transfer_amount);
            self.balances.insert(owner_address.clone(), user_balance);
          }
          println!(
            "VALID: Transfer amount is valid. Sender: {:#?}",
            sender_address
          );

          brc_transfer_tx.is_valid = true;
          return brc_transfer_tx; // Return as soon as a successful transfer is found
        }
      }
    }

    brc_transfer_tx // Return invalid struct if no successful transfer is found
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

/// Indexes BRC20 tokens by processing inscriptions from the provided `Index` object.
/// This function retrieves inscriptions, parses the content, and performs operations
/// based on the content type and operation type (deploy, mint, or transfer). The relevant
/// information is inserted into a MongoDB collection and stored in a hashmap for further
/// processing and validation.
///
/// # Arguments
///
/// * `index` - The `Index` object containing the inscriptions to process.
///
/// # Returns
///
/// * `Result<(), Box<dyn std::error::Error>>` - Represents the result of the indexing operation.
///   An `Ok` value indicates successful indexing, while an `Err` value indicates an error occurred.
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

  // Create the hashmap to store ticker information.
  let mut ticker_map: HashMap<String, Brc20Ticker> = HashMap::new();

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

            // Retrieve the owner address
            let owner = get_owner_of_outpoint(&location.outpoint, &raw_tx_info)?;

            // instantiate a new Brc20Tx struct
            let brc20_tx = Brc20Tx::new(&raw_tx_info, &owner)?;

            // Parse the body content as a `Brc20Deploy` struct.
            let deploy: Result<Brc20Deploy, _> = serde_json::from_str(parse_inc);
            if let Ok(deploy) = deploy {
              if deploy.op == "deploy" && !ticker_map.contains_key(&deploy.tick.to_lowercase()) {
                println!("=========================");
                println!("Deploy: {:?}", deploy);

                // Handle the transaction information.
                // handle_transaction(index, &location.outpoint)?;

                // Insert the `Brc20Deploy` struct into the MongoDB collection.
                let future = insert_document_into_brcs_collection(&client, deploy.clone());
                rt.block_on(future)?;

                // Instantiate a new `Brc20Ticker` struct and update the hashmap with the deploy information.
                let ticker = Brc20Ticker::new(deploy.clone());
                ticker_map.insert(deploy.tick.to_lowercase(), ticker);
              }
            } else {
              // Parse the body content as a `Brc20MintTransfer` struct.
              let mint_transfer: Result<Brc20MintTransfer, _> = serde_json::from_str(parse_inc);
              if let Ok(mint_transfer) = mint_transfer {
                // Check if the ticker struct exists for the ticker.Otherwise ignore the mint transfer.
                if let Some(ticker) = ticker_map.get_mut(&mint_transfer.tick) {
                  // if mint, then validate the mint operation
                  if mint_transfer.op == "mint" {
                    let brc20_mint_tx = ticker.validate_mint(mint_transfer, brc20_tx);

                    if brc20_mint_tx.is_valid {
                      println!("=========================");
                      println!("Mint: {:?}", mint_transfer);
                      println!("Owner Address: {:?}", owner);
                      // Update the ticker struct with the mint operation.
                      ticker.add_mint(brc20_mint_tx);

                      // Insert the `Brc20MintTransfer` struct into the MongoDB collection.
                      let future = insert_document_into_brcs_collection(&client, mint_transfer);
                      rt.block_on(future)?;
                    } else {
                      // println!("Invalid mint operation. Skipping...");
                    }
                  } else if mint_transfer.op == "transfer" {
                    // Check if the transfer is a send or inscription transfer.

                    let is_send = ticker
                      .get_user_balance(&owner)
                      .map(|balance| {
                        let matching_inputs = brc20_tx
                          .inputs
                          .iter()
                          .any(|input| balance.has_matching_inscription(input.txid, input.vout));
                        matching_inputs
                      })
                      .unwrap_or(false);

                    let brc20_transfer_tx: Brc20TransferTx;

                    if is_send {
                      // This is a send transfer
                      brc20_transfer_tx =
                        ticker.handle_send_transfer(&owner, brc20_tx, mint_transfer);
                    } else {
                      // This is an inscription transfer
                      brc20_transfer_tx =
                        ticker.handle_transfer(brc20_tx, mint_transfer, &owner, inscription_id);
                      ticker.add_transfer_inscription(brc20_transfer_tx.clone());
                    }

                    // Check if the transfer is valid.
                    if brc20_transfer_tx.is_valid {
                      println!("=========================");
                      println!("Transfer: {:?}", brc20_transfer_tx.transfer);
                      println!("Owner Address: {:?}", owner);

                      // Update the ticker struct with the transfer operation.
                      ticker.add_transfer(brc20_transfer_tx);

                      // Insert the `Brc20MintTransfer` struct into the MongoDB collection.
                      let future =
                        insert_document_into_brcs_collection(&client, brc20_transfer_tx.transfer);
                      rt.block_on(future)?;
                    } else {
                      // println!("Invalid transfer operation. Skipping...");
                    }
                  }
                } else {
                  // println!("INVALID: Ticker not found.");
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
  println!("Ticker Map: {:?}", ticker_map);
  println!("=========================");

  Ok(())
}

pub(crate) fn get_owner_of_outpoint(
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

// fn handle_transfer(index: &Index, raw_tx_info: &GetRawTransactionResult) {
//   println!("Inputs:");

//   for (i, input) in raw_tx_info.vin.iter().enumerate() {
//     let prev_tx_info = match &input.txid {
//       Some(txid) => index.client.get_raw_transaction_info(&txid, None),
//       None => continue,
//     };

//     if let Ok(prev_tx_info) = prev_tx_info {
//       let output_index = input.vout.unwrap();
//       if let Some(output) = prev_tx_info.vout.get(output_index as usize) {
//         let value = output.value;
//         let script_pubkey = output.script_pub_key.clone();
//         let address =
//           Address::from_script(&script_pubkey.script().unwrap(), Network::Testnet).unwrap();

//         println!("Input {}: ", i + 1);
//         println!("  Value: {}", value);
//         println!("  Address: {:?}", address);
//         println!("  Script Pub Key: {:?}", script_pubkey.asm);
//       }
//     }
//   }
// }

// pub(crate) fn handle_transaction(
//   index: &Index,
//   outpoint: &OutPoint,
// ) -> Result<(), Box<dyn std::error::Error>> {
//   // Get the raw transaction info.
//   let raw_tx_info = index
//     .client
//     .get_raw_transaction_info(&outpoint.txid, None)?;

//   // Display the raw transaction info.
//   // display_raw_transaction_info(&raw_tx_info);

//   // Get the transaction Inputs
//   let inputs = &raw_tx_info.transaction()?.input;

//   // Get the addresses and values of the inputs.
//   let input_addresses_values = transaction_inputs_to_addresses_values(index, inputs)?;
//   for (index, (address, value)) in input_addresses_values.iter().enumerate() {
//     println!("Input Address {}: {}, Value: {}", index + 1, address, value);
//   }

//   // display_input_info(&raw_tx_info);

//   println!("=====");
//   // Get the transaction Outputs
//   // let outputs = &raw_tx_info.transaction()?.output;

//   // Get the addresses and values of the outputs.
//   // let output_addresses_values = transaction_outputs_to_addresses_values(outputs)?;
//   // for (index, (address, value)) in output_addresses_values.iter().enumerate() {
//   //   println!(
//   //     "Output Address {}: {}, Value: {}",
//   //     index + 1,
//   //     address,
//   //     value
//   //   );
//   // }

//   Ok(())
// }

// fn transaction_inputs_to_addresses_values(
//   index: &Index,
//   inputs: &Vec<TxIn>,
// ) -> Result<Vec<(Address, u64)>, Box<dyn std::error::Error>> {
//   let mut addresses_values: Vec<(Address, u64)> = vec![];

//   for input in inputs {
//     let prev_output = input.previous_output;
//     println!(
//       "Input from transaction: {:?}, index: {:?}",
//       prev_output.txid, prev_output.vout
//     );

//     let prev_tx_info = index
//       .client
//       .get_raw_transaction_info(&prev_output.txid, None)?;

//     // display_output_info(&prev_tx_info, prev_output.vout.try_into()?);

//     let prev_tx = prev_tx_info.transaction()?;

//     let output = &prev_tx.output[usize::try_from(prev_output.vout).unwrap()];
//     let script_pub_key = &output.script_pubkey;

//     let address = Address::from_script(&script_pub_key, Network::Testnet).map_err(|_| {
//       println!("Couldn't derive address from scriptPubKey");
//       "Couldn't derive address from scriptPubKey"
//     })?;

//     // Add both the address and the value of the output to the list
//     addresses_values.push((address, output.value));

//     println!("=====");
//   }

//   if addresses_values.is_empty() {
//     Err("Couldn't derive any addresses or values from scriptPubKeys".into())
//   } else {
//     Ok(addresses_values)
//   }
// }

// fn transaction_outputs_to_addresses_values(
//   outputs: &Vec<TxOut>,
// ) -> Result<Vec<(Address, u64)>, Box<dyn std::error::Error>> {
//   let mut addresses_values: Vec<(Address, u64)> = vec![];

//   for output in outputs {
//     let script_pub_key = &output.script_pubkey;

//     if let Ok(address) = Address::from_script(&script_pub_key, Network::Testnet) {
//       // Add both the address and the value of the output to the list
//       addresses_values.push((address, output.value));
//     } else {
//       println!("Couldn't derive address from scriptPubKey");
//     }
//   }

//   if addresses_values.is_empty() {
//     Err("Couldn't derive any addresses or values from scriptPubKeys".into())
//   } else {
//     Ok(addresses_values)
//   }
// }

// fn display_raw_transaction_info(raw_transaction_info: &GetRawTransactionResult) {
//   println!("Raw Transaction Information:");
//   println!("----------------");
//   println!("Txid: {:?}", raw_transaction_info.txid);
//   println!("Hash: {:?}", raw_transaction_info.hash);
//   // println!("Size: {:?}", raw_transaction_info.size);
//   // println!("Vsize: {:?}", raw_transaction_info.vsize);
//   // println!("Version: {:?}", raw_transaction_info.version);
//   // println!("Locktime: {:?}", raw_transaction_info.locktime);
//   println!("Blockhash: {:?}", raw_transaction_info.blockhash);
//   // println!("Confirmations: {:?}", raw_transaction_info.confirmations);
//   println!("Time: {:?}", raw_transaction_info.time);
//   println!("Blocktime: {:?}", raw_transaction_info.blocktime);
//   println!();
// }

// fn display_input_info(raw_transaction_info: &GetRawTransactionResult) {
//   println!("Inputs (Vin):");
//   println!("-------------");
//   for (i, vin) in raw_transaction_info.vin.iter().enumerate() {
//     println!("Vin {}: {:?}", i + 1, vin);
//     if let Some(txid) = &vin.txid {
//       println!("  txid: {:?}", txid);
//     }
//     if let Some(vout) = vin.vout {
//       println!("  vout: {:?}", vout);
//     }
//     if let Some(script_sig) = &vin.script_sig {
//       println!("  script_sig: {:?}", script_sig);
//     }
//     if let Some(txinwitness) = &vin.txinwitness {
//       println!("  txinwitness: {:?}", txinwitness);
//     }
//     if let Some(coinbase) = &vin.coinbase {
//       println!("  coinbase: {:?}", coinbase);
//     }
//     println!("  sequence: {:?}", vin.sequence);
//   }
//   println!();
// }

// fn display_output_info(raw_transaction_info: &GetRawTransactionResult, vout_index: usize) {
//   if let Some(vout) = raw_transaction_info.vout.get(vout_index) {
//     println!("----------------------------------------------");
//     println!("Vout {}", vout_index);
//     println!("- Value: {:?}", vout.value);
//     println!("- N: {:?}", vout.n);

//     let script_pub_key = &vout.script_pub_key;
//     println!("- Script Pub Key:");
//     println!("    - ASM: {:?}", script_pub_key.asm);
//     println!("    - Hex: {:?}", script_pub_key.hex);
//     println!("    - Required Signatures: {:?}", script_pub_key.req_sigs);
//     println!("    - Type: {:?}", script_pub_key.type_);
//     println!("    - Addresses: {:?}", script_pub_key.addresses);
//     println!("    - Address: {:?}", script_pub_key.address);

//     println!();
//   } else {
//     println!("Invalid vout index: {}", vout_index);
//   }
//   println!();
// }

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
