use super::*;
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

/// The `index_brc20` function is responsible for indexing BRC20 tokens into a MongoDB database.
///
/// # Arguments
///
/// * `index` - An `Index` object representing the BRC20 tokens to be indexed.
///
/// # Returns
///
/// This function returns a `Result` which is an enumeration representing either success (`Ok`) or failure (`Err`).
///
/// # Errors
///
/// This function will return an error if any of the following occur:
/// * The runtime for asynchronous operations cannot be created.
/// * The MongoDB client cannot be created.
/// * The inscriptions cannot be retrieved from the `Index` object.
/// * The `Inscription` object cannot be retrieved for a given inscription.
/// * The body content of the `Inscription` cannot be parsed as a string.
/// * The body content cannot be parsed as a `Brc20Deploy` or `Brc20MintTransfer` struct.
/// * The document cannot be inserted into the MongoDB collection.
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

  // Retrieve the inscriptions from the `Index` object.
  let inscriptions = index.get_inscriptions(None)?;

  // Iterate over the inscriptions.
  for (_location, inscription) in inscriptions {
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
              println!("Deploy: {:?}", deploy);
              // Insert the `Brc20Deploy` struct into the MongoDB collection.
              let future = insert_document_into_brcs_collection(&client, deploy);
              rt.block_on(future)?;
            } else {
              // Parse the body content as a `Brc20MintTransfer` struct.
              let mint_transfer: Result<Brc20MintTransfer, _> = serde_json::from_str(parse_inc);
              if let Ok(mint_transfer) = mint_transfer {
                println!("MintTransfer: {:?}", mint_transfer);
                // Insert the `Brc20MintTransfer` struct into the MongoDB collection.
                let future = insert_document_into_brcs_collection(&client, mint_transfer);
                rt.block_on(future)?;
              }
            }
          }
        }
      }
    }
  }

  Ok(())
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
///
/// # Example
///
/// ```
/// let client = MongoClient::new("mongodb://localhost:27017", "omnisat").await?;
/// let deploy = Brc20Deploy { p: "p", op: "op", tick: "tick", max: "max", lim: "lim" };
/// insert_document_into_brcs_collection(&client, deploy).await?;
/// ```
async fn insert_document_into_brcs_collection<T: ToDocument>(
  client: &MongoClient,
  item: T,
) -> Result<(), Box<dyn std::error::Error>> {
  println!("INSERT");
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
