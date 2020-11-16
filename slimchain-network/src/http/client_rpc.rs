use futures::prelude::*;
use serde::{Deserialize, Serialize};
use slimchain_common::{
    basic::{BlockHeight, ShardId},
    error::{ensure, Error, Result},
    tx_req::SignedTxRequest,
};
use slimchain_utils::record_event;
use std::{iter, sync::Arc};
use warp::Filter;

const CLIENT_RPC_ROUTE_PATH: &str = "client_rpc";
const TX_REQ_ROUTE_PATH: &str = "tx_req";
const RECORD_EVENT_ROUTE_PATH: &str = "record_event";
const TX_COUNT_ROUTE_PATH: &str = "tx_count";
const BLOCK_HEIGHT_ROUTE_PATH: &str = "block_height";

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TxHttpRequest {
    pub req: SignedTxRequest,
    #[serde(default)]
    pub shard_id: ShardId,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecordEventHttpRequest {
    pub info: String,
    pub data: Option<serde_json::Value>,
}

impl RecordEventHttpRequest {
    fn emit_record_event(&self) {
        match self.data.as_ref() {
            Some(data) => record_event!("client_event", "info": self.info, "data": data),
            None => record_event!("client_event", "info": self.info),
        }
    }
}

pub async fn send_tx_request(endpoint: &str, req: SignedTxRequest) -> Result<()> {
    send_tx_requests(endpoint, iter::once(req)).await
}

pub async fn send_tx_requests(
    endpoint: &str,
    reqs: impl Iterator<Item = SignedTxRequest>,
) -> Result<()> {
    let reqs = reqs.into_iter().map(|req| (req, ShardId::default()));
    send_tx_requests_with_shard(endpoint, reqs).await
}

pub async fn send_tx_requests_with_shard(
    endpoint: &str,
    reqs: impl Iterator<Item = (SignedTxRequest, ShardId)>,
) -> Result<()> {
    let reqs: Vec<_> = reqs
        .into_iter()
        .map(|(req, shard_id)| TxHttpRequest { req, shard_id })
        .collect();

    let mut resp = surf::post(&format!(
        "http://{}/{}/{}",
        endpoint, CLIENT_RPC_ROUTE_PATH, TX_REQ_ROUTE_PATH
    ))
    .body(surf::Body::from_json(&reqs).map_err(Error::msg)?)
    .await
    .map_err(Error::msg)?;
    ensure!(
        resp.status().is_success(),
        "Failed to send Tx http req (status code: {})",
        resp.status()
    );
    resp.body_json().await.map_err(Error::msg)
}

pub async fn send_record_event(endpoint: &str, info: &str) -> Result<()> {
    send_record_event_inner(
        endpoint,
        RecordEventHttpRequest {
            info: info.to_string(),
            data: None,
        },
    )
    .await
}

pub async fn send_record_event_with_data(
    endpoint: &str,
    info: &str,
    data: impl Serialize,
) -> Result<()> {
    send_record_event_inner(
        endpoint,
        RecordEventHttpRequest {
            info: info.to_string(),
            data: Some(serde_json::to_value(data).map_err(Error::msg)?),
        },
    )
    .await
}

async fn send_record_event_inner(endpoint: &str, req: RecordEventHttpRequest) -> Result<()> {
    let mut resp = surf::post(&format!(
        "http://{}/{}/{}",
        endpoint, CLIENT_RPC_ROUTE_PATH, RECORD_EVENT_ROUTE_PATH
    ))
    .body(surf::Body::from_json(&req).map_err(Error::msg)?)
    .await
    .map_err(Error::msg)?;
    ensure!(
        resp.status().is_success(),
        "Failed to send record event http req (status code: {})",
        resp.status()
    );
    resp.body_json().await.map_err(Error::msg)
}

pub async fn get_tx_count(endpoint: &str) -> Result<usize> {
    let mut resp = surf::get(&format!(
        "http://{}/{}/{}",
        endpoint, CLIENT_RPC_ROUTE_PATH, TX_COUNT_ROUTE_PATH
    ))
    .await
    .map_err(Error::msg)?;
    ensure!(
        resp.status().is_success(),
        "Failed to get tx count (status code: {})",
        resp.status()
    );
    resp.body_json().await.map_err(Error::msg)
}

pub async fn get_block_height(endpoint: &str) -> Result<BlockHeight> {
    let mut resp = surf::get(&format!(
        "http://{}/{}/{}",
        endpoint, CLIENT_RPC_ROUTE_PATH, BLOCK_HEIGHT_ROUTE_PATH
    ))
    .await
    .map_err(Error::msg)?;
    ensure!(
        resp.status().is_success(),
        "Failed to get block height count (status code: {})",
        resp.status()
    );
    resp.body_json().await.map_err(Error::msg)
}

#[derive(Debug)]
struct ClientRpcServerError(Error);

impl warp::reject::Reject for ClientRpcServerError {}

pub fn client_rpc_server<TxReqOutput>(
    tx_req_fn: impl Fn(Vec<TxHttpRequest>) -> TxReqOutput + Send + Sync + 'static,
    tx_count_fn: impl Fn() -> usize + Send + Sync + 'static,
    block_height_fn: impl Fn() -> BlockHeight + Send + Sync + 'static,
) -> warp::filters::BoxedFilter<(impl warp::Reply,)>
where
    TxReqOutput: TryFuture<Ok = (), Error = Error> + Send + Sync + 'static,
{
    let tx_req_fn = Arc::new(tx_req_fn);
    let tx_req_route = warp::post()
        .and(warp::path(TX_REQ_ROUTE_PATH))
        .and(warp::body::json())
        .and_then(move |reqs: Vec<TxHttpRequest>| {
            tx_req_fn(reqs)
                .map_ok(|_| warp::reply::json(&()))
                .map_err(|e| warp::reject::custom(ClientRpcServerError(e)))
        });
    let record_event_route = warp::post()
        .and(warp::path(RECORD_EVENT_ROUTE_PATH))
        .and(warp::body::json())
        .map(move |req: RecordEventHttpRequest| {
            req.emit_record_event();
            warp::reply::json(&())
        });
    let tx_count_fn = Arc::new(tx_count_fn);
    let tx_count_route = warp::get()
        .and(warp::path(TX_COUNT_ROUTE_PATH))
        .map(move || {
            let tx_count = tx_count_fn();
            warp::reply::json(&tx_count)
        });
    let block_height_fn = Arc::new(block_height_fn);
    let block_height_route = warp::get()
        .and(warp::path(BLOCK_HEIGHT_ROUTE_PATH))
        .map(move || {
            let block_height = block_height_fn();
            warp::reply::json(&block_height)
        });
    warp::path(CLIENT_RPC_ROUTE_PATH)
        .and(
            tx_req_route
                .or(record_event_route)
                .or(tx_count_route)
                .or(block_height_route),
        )
        .boxed()
}
