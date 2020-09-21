#[macro_use]
extern crate tracing;

use futures::{future::join_all, join};
use itertools::Itertools;
use once_cell::sync::OnceCell;
use rand::{distributions::Uniform, prelude::*};
use slimchain_common::{
    basic::{Address, ShardId, U256},
    ed25519::Keypair,
    error::{bail, Result},
    tx_req::{caller_address_from_pk, SignedTxRequest, TxRequest},
};
use slimchain_network::http::send_tx_request_with_shard;
use slimchain_utils::{
    contract::{contract_address, Contract, Token},
    init_tracing_subscriber,
};
use std::io::{self, prelude::*};
use structopt::StructOpt;
use tokio::time::{delay_for, Duration, Instant};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum ContractArg {
    CpuHeavy,
    DoNothing,
    IoHeavy,
    KVStore,
    SmallBank,
}

macro_rules! load_contract {
    ($name: literal) => {{
        static CONTRACT: OnceCell<Contract> = OnceCell::new();
        CONTRACT.get_or_init(|| {
            Contract::from_bytes(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../contracts/build/contracts/",
                $name,
                ".json",
            )))
            .expect("Failed to load the contract.")
        })
    }};
}

impl ContractArg {
    fn get_contract(self) -> &'static Contract {
        match self {
            ContractArg::CpuHeavy => load_contract!("Sorter"),
            ContractArg::DoNothing => load_contract!("Nothing"),
            ContractArg::IoHeavy => load_contract!("IO"),
            ContractArg::KVStore => load_contract!("KVstore"),
            ContractArg::SmallBank => load_contract!("SmallBank"),
        }
    }

    fn gen_tx_input(self, rng: &mut ThreadRng) -> Result<Vec<u8>> {
        match self {
            ContractArg::CpuHeavy => {
                static CPU_HEAVY_TX_INPUT: OnceCell<Vec<u8>> = OnceCell::new();
                Ok(CPU_HEAVY_TX_INPUT
                    .get_or_init(|| {
                        ContractArg::CpuHeavy
                            .get_contract()
                            .encode_tx_input("sort", &[Token::Uint(128.into())])
                            .expect("Failed to create CPU_HEAVY_TX_INPUT.")
                    })
                    .clone())
            }
            ContractArg::DoNothing => {
                static DO_NOTHING_TX_INPUT: OnceCell<Vec<u8>> = OnceCell::new();
                Ok(DO_NOTHING_TX_INPUT
                    .get_or_init(|| {
                        ContractArg::DoNothing
                            .get_contract()
                            .encode_tx_input("nothing", &[])
                            .expect("Failed to create DO_NOTHING_TX_INPUT.")
                    })
                    .clone())
            }
            ContractArg::IoHeavy => {
                let op_gen = Uniform::new(1, 4);
                let key_gen = Uniform::new(1, 100_000);
                let size_gen = Uniform::new(1, 32);
                let contract = self.get_contract();
                match op_gen.sample(rng) {
                    1 => contract.encode_tx_input(
                        "scan",
                        &[
                            Token::Uint(key_gen.sample(rng).into()),
                            Token::Uint(size_gen.sample(rng).into()),
                        ],
                    ),
                    2 => contract.encode_tx_input(
                        "revert_scan",
                        &[
                            Token::Uint(key_gen.sample(rng).into()),
                            Token::Uint(size_gen.sample(rng).into()),
                        ],
                    ),
                    3 => contract.encode_tx_input(
                        "write",
                        &[
                            Token::Uint(key_gen.sample(rng).into()),
                            Token::Uint(size_gen.sample(rng).into()),
                        ],
                    ),
                    _ => unreachable!(),
                }
            }
            ContractArg::KVStore => todo!(),
            ContractArg::SmallBank => {
                // https://github.com/ooibc88/blockbench/blob/master/src/macro/smallbank/smallbank.cc
                let op_gen = Uniform::new(1, 7);
                let acc_gen = Uniform::new(1, 100_000);
                let bal_gen = Uniform::new(1, 100);
                let contract = self.get_contract();
                match op_gen.sample(rng) {
                    1 => contract.encode_tx_input(
                        "almagate",
                        &[
                            Token::String(acc_gen.sample(rng).to_string()),
                            Token::String(acc_gen.sample(rng).to_string()),
                        ],
                    ),
                    2 => contract.encode_tx_input(
                        "getBalance",
                        &[Token::String(acc_gen.sample(rng).to_string())],
                    ),
                    3 => contract.encode_tx_input(
                        "updateBalance",
                        &[
                            Token::String(acc_gen.sample(rng).to_string()),
                            Token::Uint(bal_gen.sample(rng).into()),
                        ],
                    ),
                    4 => contract.encode_tx_input(
                        "updateSaving",
                        &[
                            Token::String(acc_gen.sample(rng).to_string()),
                            Token::Uint(bal_gen.sample(rng).into()),
                        ],
                    ),
                    5 => contract.encode_tx_input(
                        "sendPayment",
                        &[
                            Token::String(acc_gen.sample(rng).to_string()),
                            Token::String(acc_gen.sample(rng).to_string()),
                            Token::Uint(0.into()),
                        ],
                    ),
                    6 => contract.encode_tx_input(
                        "writeCheck",
                        &[
                            Token::String(acc_gen.sample(rng).to_string()),
                            Token::Uint(0.into()),
                        ],
                    ),
                    _ => unreachable!(),
                }
            }
        }
    }
}

fn parse_contract_arg(input: &str) -> Result<ContractArg> {
    Ok(match input {
        "cpuheavy" => ContractArg::CpuHeavy,
        "donothing" => ContractArg::DoNothing,
        "ioheavy" => ContractArg::IoHeavy,
        "kvstore" => ContractArg::KVStore,
        "smallbank" => ContractArg::SmallBank,
        _ => {
            bail!("Accepted values: cpuheavy, donothing, ioheavy, kvstore, and smallbank.");
        }
    })
}

fn create_deploy_tx(contract: ContractArg, shard_id: ShardId) -> (Address, SignedTxRequest) {
    info!(
        "Create deploy tx for contract {:?} at {:?}",
        contract, shard_id
    );
    let mut rng = thread_rng();
    loop {
        let keypair = Keypair::generate(&mut rng);
        let caller_address = caller_address_from_pk(&keypair.public);
        let contract_address = contract_address(caller_address, U256::from(0).into());
        if shard_id.contains(contract_address) {
            let tx_req = TxRequest::Create {
                nonce: U256::from(0).into(),
                code: contract.get_contract().code().clone(),
            };
            return (contract_address, tx_req.sign(&keypair));
        }
    }
}

#[derive(Debug, StructOpt)]
struct Opts {
    /// Endpoint to http tx server.
    #[structopt(long, default_value = "127.0.0.1:8000")]
    endpoint: String,

    /// Total number of shards.
    #[structopt(short, long, default_value = "1")]
    shard: u64,

    /// Total number of TX.
    #[structopt(short, long)]
    total: usize,

    /// Number of TX per seconds.
    #[structopt(short, long)]
    rate: usize,

    /// List of contracts. Accepted values: cpuheavy, donothing, ioheavy, kvstore, and smallbank.
    #[structopt(parse(try_from_str = parse_contract_arg))]
    contract: Vec<ContractArg>,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_backtrace::install();
    init_tracing_subscriber("info")?;

    let opts = Opts::from_args();
    info!("Opts: {:#?}", opts);

    let mut contracts: Vec<(Address, ShardId, ContractArg)> =
        Vec::with_capacity(opts.contract.len());
    let deploy_txs: Vec<(ShardId, SignedTxRequest)> = opts
        .contract
        .iter()
        .enumerate()
        .map(|(id, &contract)| {
            let id = (id as u64) % opts.shard;
            let shard_id = ShardId::new(id as u64, opts.shard);
            let (address, deploy_tx) = create_deploy_tx(contract, shard_id);
            contracts.push((address, shard_id, contract));
            (shard_id, deploy_tx)
        })
        .collect();
    info!("Deploy txs");
    for (shard_id, tx_req) in deploy_txs {
        send_tx_request_with_shard(&opts.endpoint, tx_req, shard_id).await?;
    }
    info!("Deploy finished");

    let keys: Vec<Keypair> = {
        let mut keygen_rng = thread_rng();
        std::iter::repeat_with(|| Keypair::generate(&mut keygen_rng))
            .take(opts.total)
            .collect()
    };

    {
        print!("Press Enter to continue");
        io::stdout().flush().ok();
        let mut buf = String::new();
        io::stdin().read_line(&mut buf)?;
    }

    let begin = Instant::now();

    let mut rng = thread_rng();
    let mut req_futs = Vec::with_capacity(opts.rate + 1);
    let mut next_epoch = delay_for(Duration::from_secs(1));
    for (i, key) in keys.iter().enumerate() {
        let (address, shard_id, contract) = contracts.choose(&mut rng).copied().unwrap();
        let tx_req = TxRequest::Call {
            nonce: U256::from(0).into(),
            address,
            data: contract.gen_tx_input(&mut rng)?,
        };
        let signed_tx_req = tx_req.sign(key);
        let req_fut = send_tx_request_with_shard(&opts.endpoint, signed_tx_req, shard_id);
        req_futs.push(req_fut);

        if req_futs.len() == opts.rate {
            let (resps, _) = join!(join_all(req_futs.drain(..)), next_epoch);
            resps.into_iter().try_collect()?;

            next_epoch = delay_for(Duration::from_secs(1));
        }

        if (i + 1) % 1_000 == 0 {
            info!("Sent #{} txs", i + 1);
        }
    }

    if !req_futs.is_empty() {
        join_all(req_futs).await.into_iter().try_collect()?;
    }

    let total_time = Instant::now() - begin;
    info!("Time: {:?}", total_time);
    info!(
        "Real rate: {:?} tx/s",
        (opts.total as f64) / total_time.as_secs_f64()
    );

    Ok(())
}
