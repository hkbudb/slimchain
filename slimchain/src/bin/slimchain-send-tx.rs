#[macro_use]
extern crate tracing;

use once_cell::sync::{Lazy, OnceCell};
use rand::{distributions::Uniform, prelude::*, rngs::StdRng};
use regex::Regex;
use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::{Address, Nonce, ShardId, U256},
    ed25519::Keypair,
    error::{anyhow, bail, Context as _, Result},
    tx_req::{caller_address_from_pk, SignedTxRequest, TxRequest},
};
use slimchain_network::http::{
    client_rpc::{
        get_block_height, get_tx_count, send_record_event, send_record_event_with_data,
        send_tx_requests_with_shard,
    },
    node_rpc::get_leader,
};
use slimchain_utils::{
    contract::{contract_address, Contract, Token},
    init_tracing_subscriber,
};
use std::{
    collections::VecDeque,
    fs::File,
    io::{self, prelude::*},
    path::PathBuf,
    sync::Mutex,
};
use structopt::StructOpt;
use tokio::time::{delay_for, delay_until, Duration, Instant};

static YCSB: OnceCell<Mutex<io::BufReader<File>>> = OnceCell::new();
static YCSB_READ_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^READ usertable (\w+) \[.+\]$").unwrap());
static YCSB_WRITE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^UPDATE usertable (\w+) \[ field\d+=(.+) \]$").unwrap());

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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

    fn gen_tx_input(self, rng: &mut impl Rng) -> Result<Vec<u8>> {
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
                let size_gen = Uniform::new(1, 8);
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
            ContractArg::KVStore => {
                let contract = self.get_contract();

                if let Some(ycsb) = YCSB.get() {
                    let mut ycsb = ycsb.lock().map_err(|_e| anyhow!("Failed to lock YCSB."))?;
                    loop {
                        let mut buf = String::new();
                        let buf_len = ycsb.read_line(&mut buf)?;

                        if buf_len == 0 {
                            bail!("Failed to read ycsb file. Reach EOF.");
                        }

                        let buf = buf.trim();

                        if let Some(cap) = YCSB_READ_RE.captures(&buf) {
                            return contract
                                .encode_tx_input("get", &[Token::String(cap[1].to_string())]);
                        }

                        if let Some(cap) = YCSB_WRITE_RE.captures(&buf) {
                            return contract.encode_tx_input(
                                "set",
                                &[
                                    Token::String(cap[1].to_string()),
                                    Token::String(cap[2].to_string()),
                                ],
                            );
                        }

                        warn!("Skip line in ycsb file: {}", buf);
                    }
                } else {
                    bail!("Failed to access ycsb file.");
                }
            }
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

fn create_deploy_tx(
    rng: &mut (impl Rng + CryptoRng),
    contract: ContractArg,
    shard_id: ShardId,
) -> (Address, SignedTxRequest) {
    info!(
        "Create deploy tx for contract {:?} at {:?}",
        contract, shard_id
    );
    loop {
        let keypair = Keypair::generate(rng);
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

#[derive(Debug, StructOpt, Serialize, Deserialize)]
#[structopt(version = git_version::git_version!(prefix = concat!(env!("CARGO_PKG_VERSION"), " ("), suffix = ")", fallback = "unknown"))]
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

    /// Wait period in seconds to check block committing after sending TX.
    #[structopt(short, long, default_value = "60")]
    wait: u64,

    /// Seed used for RNG.
    #[structopt(long)]
    seed: Option<u64>,

    /// Maximum number of accounts.
    #[structopt(short, long)]
    accounts: Option<usize>,

    /// Exec additional raft related operations.
    #[structopt(long)]
    raft: bool,

    /// List of contracts. Accepted values: cpuheavy, donothing, ioheavy, kvstore, and smallbank.
    #[structopt(parse(try_from_str = parse_contract_arg), required = true)]
    contract: Vec<ContractArg>,

    #[structopt(
        short,
        long,
        parse(from_os_str),
        help = "Path to ycsb.txt. Used for kvstore smart contract.",
        long_help = r#"Path to ycsb.txt. Used for kvstore smart contract.

The file should contain content similar to the below:
    UPDATE usertable <user> [ field="<value>" ]
    READ usertable <user> [ <all fields>]

To generate it:
    /path/to/ycsb.sh run basic -P /path/to/workload.spec
"#
    )]
    ycsb: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_backtrace::install();
    init_tracing_subscriber("info")?;

    let opts = Opts::from_args();
    info!("Opts: {:#?}", opts);

    if let Some(ycsb) = opts.ycsb.as_ref() {
        YCSB.set(Mutex::new(io::BufReader::new(File::open(ycsb)?)))
            .map_err(|_e| anyhow!("Failed to set YCSB."))?;
    }

    if opts.raft {
        let mut i = 0;
        loop {
            match get_leader(&opts.endpoint).await {
                Ok(leader) => {
                    info!("Raft Leader: {}", leader);
                    break;
                }
                Err(_) => {
                    delay_for(ONE_SECOND).await;
                    i += 1;
                    if i % 60 == 0 {
                        info!("Waiting for leader election...");
                    }
                }
            }
        }
    }

    send_record_event_with_data(&opts.endpoint, "send-tx-opts", &opts).await?;

    let mut rng = match opts.seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => StdRng::from_entropy(),
    };

    let mut contracts: Vec<(Address, ShardId, ContractArg)> =
        Vec::with_capacity(opts.contract.len());
    let deploy_txs: Vec<(SignedTxRequest, ShardId)> = opts
        .contract
        .iter()
        .enumerate()
        .map(|(id, &contract)| {
            let id = (id as u64) % opts.shard;
            let shard_id = ShardId::new(id as u64, opts.shard);
            let (address, deploy_tx) = create_deploy_tx(&mut rng, contract, shard_id);
            debug!("tx {} address {}", id, address);
            contracts.push((address, shard_id, contract));
            (deploy_tx, shard_id)
        })
        .collect();

    info!("Deploy txs");
    let tx_count = get_tx_count(&opts.endpoint).await?;
    send_tx_requests_with_shard(&opts.endpoint, deploy_txs.into_iter()).await?;

    loop {
        delay_for(Duration::from_millis(500)).await;
        let tx_count2 = get_tx_count(&opts.endpoint).await?;

        if tx_count2 >= tx_count + opts.contract.len() {
            break;
        }
    }
    info!("Deploy finished");

    if opts.raft {
        info!("Current Raft Leader: {}", get_leader(&opts.endpoint).await?);
    }

    let mut accounts: VecDeque<(Keypair, Nonce)> = {
        std::iter::repeat_with(|| (Keypair::generate(&mut rng), Nonce::zero()))
            .take(opts.accounts.unwrap_or(opts.total))
            .collect()
    };

    send_record_event(&opts.endpoint, "start-send-tx").await?;
    let begin = Instant::now();
    const ONE_SECOND: Duration = Duration::from_secs(1);
    let mut next_epoch = begin + ONE_SECOND;

    let mut reqs = Vec::with_capacity(opts.rate + 1);
    let mut next_epoch_fut = delay_until(next_epoch);
    for i in 0..opts.total {
        let (address, shard_id, contract) = contracts
            .choose(&mut rng)
            .copied()
            .expect("Failed to get contract.");
        let (key, nonce) = accounts.pop_front().context("Failed to get account.")?;
        let tx_req = TxRequest::Call {
            nonce,
            address,
            data: contract.gen_tx_input(&mut rng)?,
        };
        let signed_tx_req = tx_req.sign(&key);
        accounts.push_back((key, (U256::from(nonce) + 1).into()));

        reqs.push((signed_tx_req, shard_id));

        if reqs.len() == opts.rate {
            send_tx_requests_with_shard(&opts.endpoint, reqs.drain(..)).await?;
            next_epoch_fut.await;

            next_epoch += ONE_SECOND;
            next_epoch_fut = delay_until(next_epoch);
        }

        if (i + 1) % 1_000 == 0 {
            info!("Sent #{} txs", i + 1);
        }
    }

    if !reqs.is_empty() {
        send_tx_requests_with_shard(&opts.endpoint, reqs.drain(..)).await?;
    }

    let total_time = Instant::now() - begin;
    let real_rate = (opts.total as f64) / total_time.as_secs_f64();
    send_record_event_with_data(
        &opts.endpoint,
        "end-send-tx",
        serde_json::json! {{
            "total_time_in_us": total_time.as_micros() as u64,
            "real_rate": real_rate,
        }},
    )
    .await?;

    info!("Time: {:?}", total_time);
    info!("Real rate: {:?} tx/s", real_rate);

    let mut cur_block_height = get_block_height(&opts.endpoint).await?;
    let mut block_update_time = Instant::now();

    loop {
        delay_for(Duration::from_millis(500)).await;
        let height = get_block_height(&opts.endpoint).await?;

        if height > cur_block_height {
            block_update_time = Instant::now();
            cur_block_height = height;
            continue;
        } else if Instant::now() - block_update_time > Duration::from_secs(opts.wait) {
            break;
        }
    }

    info!("You can stop the nodes now by: kill -INT <pid>");

    if opts.raft {
        info!("Current Raft Leader: {}", get_leader(&opts.endpoint).await?);
    }

    send_record_event(&opts.endpoint, "quit-send-tx").await?;

    Ok(())
}
