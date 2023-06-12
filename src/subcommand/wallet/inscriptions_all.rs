use mongodb::bson::{doc, Document};
use mongodb::{bson, options::ClientOptions, Client};
use std::str;
use {super::*, crate::wallet::Wallet};

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

pub(crate) fn run(options: Options) -> Result {
  println!("{:#?}, options", options);
  let index = Index::open(&options)?;
  //   index.update()?;

  let inscriptions = index.get_inscriptions(None)?;
  let unspent_outputs = index.get_unspent_outputs(Wallet::load(&options)?)?;

  let explorer = match options.chain() {
    Chain::Mainnet => "https://ordinals.com/inscription/",
    Chain::Regtest => "http://localhost/inscription/",
    Chain::Signet => "https://signet.ordinals.com/inscription/",
    Chain::Testnet => "https://testnet.ordinals.com/inscription/",
  };

  let mut output = Vec::new();

  let rt = Runtime::new().unwrap();
  let future = async {
    let connection_string = "mongodb://localhost:27017";
    let db_name = "omnisat";
    let client = MongoClient::new(connection_string, db_name)
      .await
      .expect("Failed to initialize MongoDB client");
    client
  };

  let client = rt.block_on(future);

  for (location, inscription) in inscriptions {
    let i = index.get_inscription_by_id(inscription).unwrap();

    // TODO: clean/rustify
    if let Some(ct) = i.clone().unwrap().content_type() {
      if ct == "application/json" || ct == "text/plain;charset=utf-8" {
        if let Some(inc) = i.clone().unwrap().body() {
          match str::from_utf8(inc) {
            Ok(parse_inc) => {
              let deploy: Result<Brc20Deploy, _> = serde_json::from_str(parse_inc);
              match deploy {
                Ok(deploy) => {
                  println!("Deploy: {:?}", deploy);
                  let document = deploy.to_document();
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
                  let mint_transfer: Result<Brc20MintTransfer, _> = serde_json::from_str(parse_inc);
                  match mint_transfer {
                    Ok(mint_transfer) => {
                      println!("MintTransfer: {:?}", mint_transfer);
                      let document = mint_transfer.to_document();
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

    output.push(Output {
      location,
      inscription,
      explorer: format!("{explorer}{inscription}"),
    });
  }

  //   print_json(&output)?;

  Ok(())
}

pub(crate) fn index_brc20(index: &Index) {
  let rt = Runtime::new().unwrap();
  let future = async {
    let connection_string = "mongodb://localhost:27017";
    let db_name = "omnisat";
    let client = MongoClient::new(connection_string, db_name)
      .await
      .expect("Failed to initialize MongoDB client");
    client
  };

  let client = rt.block_on(future);
  let inscriptions = index.get_inscriptions(None).unwrap();
  for (_location, inscription) in inscriptions {
    let i = index.get_inscription_by_id(inscription).unwrap();

    // TODO: clean/rustify
    if let Some(ct) = i.clone().unwrap().content_type() {
      if ct == "application/json" || ct == "text/plain;charset=utf-8" {
        if let Some(inc) = i.clone().unwrap().body() {
          match str::from_utf8(inc) {
            Ok(parse_inc) => {
              let deploy: Result<Brc20Deploy, _> = serde_json::from_str(parse_inc);
              match deploy {
                Ok(deploy) => {
                  println!("Deploy: {:?}", deploy);
                  let document = deploy.to_document();
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
                  let mint_transfer: Result<Brc20MintTransfer, _> = serde_json::from_str(parse_inc);
                  match mint_transfer {
                    Ok(mint_transfer) => {
                      println!("MintTransfer: {:?}", mint_transfer);
                      let document = mint_transfer.to_document();
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
