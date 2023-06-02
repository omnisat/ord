use super::*;
use bitcoincore_rpc::bitcoincore_rpc_json::GetRawTransactionResult;
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
pub struct Brc20Ticker {
  deploy: Brc20Deploy,
  mints: Vec<Brc20MintTransfer>,
  transfers: Vec<Brc20MintTransfer>,
  total_minted: u64, // New field: TotalMinted
}

impl Brc20Ticker {
  pub fn new(deploy: Brc20Deploy) -> Self {
    Brc20Ticker {
      deploy,
      mints: Vec::new(),
      transfers: Vec::new(),
      total_minted: 0,
    }
  }

  pub fn add_mint(&mut self, mint: Brc20MintTransfer) {
    self.mints.push(mint.clone());
    let minted_amount: u64 = mint.amt.parse().unwrap_or(0);
    self.total_minted += minted_amount;
  }

  pub fn add_transfer(&mut self, transfer: Brc20MintTransfer) {
    self.transfers.push(transfer);
  }

  pub fn is_mint_valid(&self, mint: &Brc20MintTransfer) -> bool {
    let minted_amount: u64 = mint.amt.parse().unwrap_or(0);
    let limit: u64 = self.deploy.lim.parse().unwrap_or(0);
    let max: u64 = self.deploy.max.parse().unwrap_or(0);

    self.total_minted + minted_amount <= max && minted_amount <= limit
  }

  pub fn is_transfer_valid(&self, transfer: &Brc20MintTransfer) -> bool {
    // TODO: Implement transfer validation logic here.
    true
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
  for (location, inscription) in inscriptions {
    // Retrieve the corresponding `Inscription` object.
    let i = index.get_inscription_by_id(inscription)?;

    // Check if the `Inscription` object exists.
    if let Some(inscription) = i {
      // Check the content type of the `Inscription`.
      if let Some(ct) = inscription.content_type() {
        // Check if the content type is either JSON or plain text.
        if ct == "application/json" || ct == "text/plain;charset=utf-8" {
          // Parse the body content of the `Inscription` as a string.
          if let Some(inc) = inscription.body() {
            let parse_inc = str::from_utf8(inc)?;

            // Parse the body content as a `Brc20Deploy` struct.
            let deploy: Result<Brc20Deploy, _> = serde_json::from_str(parse_inc);
            if let Ok(deploy) = deploy {
              println!("=========================");
              println!("Deploy: {:?}", deploy);
              println!("=========================");

              // Check if the ticker already exists in the hashmap.
              if let Some(ticker) = ticker_map.get(&deploy.tick.to_lowercase()) {
                println!("Duplicate deploy. Skipping...{:#?}", ticker);
                continue;
              }

              // Handle the transaction information.
              handle_transaction(index, &location.outpoint)?;

              // Insert the `Brc20Deploy` struct into the MongoDB collection.
              let future = insert_document_into_brcs_collection(&client, deploy.clone());
              rt.block_on(future)?;

              // Instantiate a new `Brc20Ticker` struct and update the hashmap with the deploy information.
              let ticker = Brc20Ticker::new(deploy.clone());
              ticker_map.insert(deploy.tick.clone(), ticker);
            } else {
              // Parse the body content as a `Brc20MintTransfer` struct.
              let mint_transfer: Result<Brc20MintTransfer, _> = serde_json::from_str(parse_inc);
              if let Ok(mint_transfer) = mint_transfer {
                if mint_transfer.op == "mint" {
                  println!("=========================");
                  println!("Mint: {:?}", mint_transfer);
                  println!("=========================");

                  // Check if the mint is valid.
                  if let Some(ticker) = ticker_map.get_mut(&mint_transfer.tick) {
                    if ticker.is_mint_valid(&mint_transfer) {
                      // Handle the transaction information.
                      handle_transaction(index, &location.outpoint)?;

                      // Insert the `Brc20MintTransfer` struct into the MongoDB collection.
                      let future =
                        insert_document_into_brcs_collection(&client, mint_transfer.clone());
                      rt.block_on(future)?;

                      // Update the ticker struct with the mint operation.
                      ticker.add_mint(mint_transfer.clone());
                    } else {
                      println!("Invalid mint operation. Skipping...");
                    }
                  } else {
                    println!("Ticker not found. Skipping mint operation...");
                  }
                  println!("----------------");
                }

                // CURRENTLY SUPPRESSING TRANSFERS
                // if mint_transfer.op == "transfer" {
                //   println!("=========================");
                //   println!("Transfer: {:?}", mint_transfer);
                //   println!("=========================");
                //   handle_transaction(index, &location.outpoint)?;

                //   Insert the `Brc20MintTransfer` struct into the MongoDB collection.
                //   let future = insert_document_into_brcs_collection(&client, mint_transfer);
                //   rt.block_on(future)?;
                //   println!("----------------");
                // }
              }
            }
          }
        }
      }
    }
  }

  Ok(())
}

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

  // Get the transaction Outputs
  let outputs = &raw_tx_info.transaction()?.output;

  // Get the addresses and values of the inputs.
  let input_addresses_values = transaction_inputs_to_addresses_values(index, inputs)?;
  for (index, (address, value)) in input_addresses_values.iter().enumerate() {
    println!("Input Address {}: {}, Value: {}", index + 1, address, value);
  }

  println!("=====");
  // Get the addresses and values of the outputs.
  let output_addresses_values = transaction_outputs_to_addresses_values(outputs)?;
  for (index, (address, value)) in output_addresses_values.iter().enumerate() {
    println!(
      "Output Address {}: {}, Value: {}",
      index + 1,
      address,
      value
    );
  }

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

    display_output_info(&prev_tx_info, prev_output.vout.try_into()?);

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
    println!("----------------------------------------------");
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
