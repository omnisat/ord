use crate::index::brc20_index::brc20_ticker::Brc20Ticker;
use crate::index::brc20_index::brc20_tx::{Brc20Tx, InvalidBrc20TxMap};
use crate::index::brc20_index::deploy::Brc20DeployTx;
use crate::index::brc20_index::transfer::Brc20TransferTx;

use self::deploy::Brc20Deploy;
use self::mint::Brc20Mint;
use self::transfer::Brc20Transfer;

use super::*;
use bitcoincore_rpc::bitcoincore_rpc_json::GetRawTransactionResult;
use mongodb::bson::{doc, Document};
use mongodb::{bson, options::ClientOptions, Client};
use std::str;

mod brc20_ticker;
mod brc20_tx;
mod deploy;
mod mint;
mod transfer;
mod user_balance;
mod utils;

// Brc20Index is a struct that represents the BRC-20 index.
#[derive(Debug)]
pub struct Brc20Index {
  // The BRC-20 tickers.
  tickers: HashMap<String, Brc20Ticker>,
  // The invalid BRC-20 transactions.
  invalid_tx_map: InvalidBrc20TxMap,
}

// implement new() function for Brc20Index
impl Brc20Index {
  pub fn new() -> Self {
    Brc20Index {
      tickers: HashMap::new(),
      invalid_tx_map: InvalidBrc20TxMap::new(),
    }
  }
}

// Create a new Brc20Index.
// let brc20_index = Brc20Index {
//   client,
//   db,
//   collection,
//   tickers: HashMap::new(),
//   invalid_tx_map: InvalidBrc20TxMap::new(),
// };

#[derive(Serialize, Deserialize)]
pub struct Output {
  pub inscription: InscriptionId,
  pub location: SatPoint,
  pub explorer: String,
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
        "dec": &self.dec,
    }
  }
}

impl ToDocument for Brc20Mint {
  fn to_document(&self) -> Document {
    doc! {
        "p": &self.p,
        "op": &self.op,
        "tick": &self.tick,
        "amt": &self.amt,
    }
  }
}

impl ToDocument for Brc20Transfer {
  fn to_document(&self) -> Document {
    doc! {
        "p": &self.p,
        "op": &self.op,
        "tick": &self.tick,
        "amt": &self.amt,
    }
  }
}

pub(crate) fn index_brc20(index: &Index) -> Result<(), Box<dyn std::error::Error>> {
  // Instantiate a new `Brc20Index` struct.
  let mut brc20_index = Brc20Index::new();

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
            let brc20_tx = Brc20Tx::new(raw_tx_info, owner.clone())?;

            // Parse the body content as a `Brc20Deploy` struct.
            let deploy: Result<Brc20Deploy, _> = serde_json::from_str(parse_inc);
            if let Ok(deploy) = deploy {
              if deploy.op == "deploy" {
                // validate the deploy script
                let validated_deploy_tx = Brc20DeployTx::new(brc20_tx.clone(), deploy)
                  .validate_deploy_script(&mut brc20_index.invalid_tx_map, &brc20_index.tickers);

                if validated_deploy_tx.is_valid() {
                  println!("=========================");
                  println!("Deploy: {:?}", validated_deploy_tx.get_deploy_script());
                  // Insert the `Brc20Deploy` struct into the MongoDB collection.
                  let future = insert_document_into_brcs_collection(
                    &client,
                    validated_deploy_tx.get_deploy_script().clone(),
                  );
                  rt.block_on(future)?;

                  // Instantiate a new `Brc20Ticker` struct and update the hashmap with the deploy information.
                  let ticker = Brc20Ticker::new(validated_deploy_tx);
                  brc20_index.tickers.insert(ticker.get_ticker(), ticker);
                }
              }
            }
            // Parse the body content as a `Brc20Mint` struct.
            let mint_transfer: Result<Brc20Mint, _> = serde_json::from_str(parse_inc);
            if let Ok(mint_transfer) = mint_transfer {
              // if mint, then validate the mint operation
              if mint_transfer.op == "mint" {
                // Validate and instantiate a new `Brc20MintTx` struct.
                // let mint_tx = Brc20MintTx::new(brc20_tx, mint_transfer);
                let mint_tx = mint_transfer.validate(
                  &brc20_tx,
                  &mut brc20_index.tickers,
                  &mut brc20_index.invalid_tx_map,
                );

                // Check if the mint operation is valid.
                if mint_tx.is_valid() {
                  println!("=========================");
                  println!("Mint: {:?}", mint_tx.get_mint());
                  println!("Owner Address: {:?}", owner);

                  // Insert the `Brc20MintTransfer` struct into the MongoDB collection.
                  let future =
                    insert_document_into_brcs_collection(&client, mint_tx.get_mint().clone());
                  rt.block_on(future)?;
                }
              }
            }
            // Parse the body content as a `Brc20Transfer` struct.
            let mint_transfer: Result<Brc20Transfer, _> = serde_json::from_str(parse_inc);
            if let Ok(mint_transfer) = mint_transfer {
              // if mint, then validate the mint operation
              if mint_transfer.op == "transfer" {
                // Instantiate a new `BrcTransferTx` struct.
                let mut brc20_transfer_tx = Brc20TransferTx::new(brc20_tx, mint_transfer.clone());

                // call handle_inscribe_transfer_amount
                brc20_transfer_tx = brc20_transfer_tx.handle_inscribe_transfer_amount(
                  &mut brc20_index.tickers,
                  &mut brc20_index.invalid_tx_map,
                );

                // Check if the transfer is valid.
                if brc20_transfer_tx.is_valid() {
                  println!("=========================");
                  println!("Transfer: {:?}", brc20_transfer_tx.get_transfer_script());
                  println!("Owner Address: {:?}", owner);

                  // Insert the `Brc20Transfer` struct into the MongoDB collection.
                  let future = insert_document_into_brcs_collection(
                    &client,
                    brc20_transfer_tx.get_transfer_script().clone(),
                  );
                  rt.block_on(future)?;
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
  for (ticker_symbol, ticker) in &brc20_index.tickers {
    // Access the ticker symbol and ticker value here
    println!("Ticker Symbol: {}", ticker_symbol);
    ticker.display_brc20_ticker();
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
