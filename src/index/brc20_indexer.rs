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

impl Brc20Deploy {
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Brc20MintTransfer {
  pub p: String,
  pub op: String,
  pub tick: String,
  pub amt: String,
}

impl Brc20MintTransfer {
  fn to_document(&self) -> Document {
    doc! {
        "p": &self.p,
        "op": &self.op,
        "tick": &self.tick,
        "amt": &self.amt,
    }
  }
}

/// Indexes BRC20 tokens into a MongoDB database.
///
/// This function takes an `Index` object as a parameter and performs the indexing process. It retrieves the inscriptions
/// from the `Index` object and iterates over them. For each inscription, it retrieves the corresponding `Inscription`
/// object and checks the content type. If the content type is either "application/json" or "text/plain;charset=utf-8",
/// the body content is extracted and parsed as a string.
///
/// If the body content can be successfully parsed as a `Brc20Deploy` struct, it is inserted into the MongoDB collection
/// named "brcs". Similarly, if it can be parsed as a `Brc20MintTransfer` struct, it is also inserted into the same
/// collection.
///
/// # Arguments
///
/// * `index` - An `Index` object representing the BRC20 tokens to be indexed.
///
/// # Example
///
/// ```
/// let index = Index::new();
/// index_brc20(&index);
/// ```
pub(crate) async fn index_brc20(index: &Index) {
  // Initialize the runtime for asynchronous operations.
  let rt = Runtime::new().unwrap();

  // Create a future that establishes a connection to the MongoDB server.
  let future = async {
    let connection_string = "mongodb://localhost:27017";
    let db_name = "omnisat";

    // Create a new MongoDB client with the provided connection string and database name.
    let client = MongoClient::new(connection_string, db_name)
      .await
      .expect("Failed to initialize MongoDB client");
    client // Return the MongoDB client.
  };

  // Establish a connection to the MongoDB server.
  let client = rt.block_on(future);

  // Retrieve the inscriptions from the `Index` object.
  let inscriptions = index.get_inscriptions(None).unwrap();

  // Iterate over the inscriptions.
  for (_location, inscription) in inscriptions {
    // Retrieve the corresponding `Inscription` object.
    let i = index.get_inscription_by_id(inscription).unwrap();

    // TODO: clean/rustify

    // Check the content type of the `Inscription`.
    if let Some(ct) = i.clone().unwrap().content_type() {
      if ct == "application/json" || ct == "text/plain;charset=utf-8" {
        // Extract the body content of the `Inscription`.
        if let Some(inc) = i.clone().unwrap().body() {
          match str::from_utf8(inc) {
            Ok(parse_inc) => {
              // Attempt to parse the body content as a `Brc20Deploy` struct.
              let deploy: Result<Brc20Deploy, _> = serde_json::from_str(parse_inc);
              match deploy {
                Ok(deploy) => {
                  // Body content successfully parsed as `Brc20Deploy`. Insert into the MongoDB collection.
                  println!("Deploy: {:?}", deploy);
                  let document = deploy.to_document();
                  // insert_document_into_brcs_collection(&client, document).await;
                  let future = async {
                    client
                      .insert_document("brcs", document)
                      .await
                      .expect("Failed to enter into MongoDB");
                  };
                  rt.block_on(future);
                  // client.insert_document("brcs", document);
                }
                Err(_) => {
                  // Attempt to parse the body content as a `Brc20MintTransfer` struct.
                  let mint_transfer: Result<Brc20MintTransfer, _> = serde_json::from_str(parse_inc);
                  match mint_transfer {
                    Ok(mint_transfer) => {
                      // Body content successfully parsed as `Brc20MintTransfer`. Insert into the MongoDB collection.
                      println!("MintTransfer: {:?}", mint_transfer);
                      let document = mint_transfer.to_document();
                      // insert_document_into_brcs_collection(&client, document).await;
                      let future = async {
                        client
                          .insert_document("brcs", document)
                          .await
                          .expect("Failed to enter into MongoDB");
                      };
                      rt.block_on(future);
                      // client.insert_document("brcs", document);
                    }
                    Err(_) => {
                      // eprintln!("Failed to deserialize JSON: {}", &str::from_utf8(inc).unwrap());
                      // Body content failed to parse as either `Brc20Deploy` or `Brc20MintTransfer`.
                    }
                  }
                }
              }
            }
            Err(_) => {}
          };
        }
      }
    }
  }
}

async fn insert_document_into_brcs_collection(client: &MongoClient, document: bson::Document) {
  println!("INSERT");
  client
    .insert_document("brcs", document)
    .await
    .expect("Failed to enter into MongoDB");
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
