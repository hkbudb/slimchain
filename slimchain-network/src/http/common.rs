use bytes::{Buf, Bytes};
use futures::io::Cursor;
use serde::{Deserialize, Serialize};
use slimchain_common::error::{ensure, Error, Result};
use warp::{
    http::{self, HeaderValue, Response, StatusCode},
    hyper,
    reject::Reject,
    Filter, Rejection,
};

macro_rules! check_resp {
    ($resp:ident) => {
        ensure!(
            $resp.status().is_success(),
            "Failed to send http req. Status code: {}. Msg: {}.",
            $resp.status(),
            $resp
                .body_string()
                .await
                .unwrap_or_else(|e| format!("(Failed to decode http response: {})", e)),
        );
    };
}

pub async fn send_get_request_using_json<Resp: for<'de> Deserialize<'de>>(
    uri: &str,
) -> Result<Resp> {
    let mut resp = surf::get(uri).await.map_err(Error::msg)?;
    check_resp!(resp);
    resp.body_json().await.map_err(Error::msg)
}

pub async fn send_post_request_using_json<Req: Serialize, Resp: for<'de> Deserialize<'de>>(
    uri: &str,
    req: &Req,
) -> Result<Resp> {
    let mut resp = surf::post(uri)
        .body(surf::Body::from_json(&req).map_err(Error::msg)?)
        .await
        .map_err(Error::msg)?;
    check_resp!(resp);
    resp.body_json().await.map_err(Error::msg)
}

pub async fn send_get_request_using_postcard<Resp: for<'de> Deserialize<'de>>(
    uri: &str,
) -> Result<Resp> {
    let mut resp = surf::get(uri).await.map_err(Error::msg)?;
    check_resp!(resp);
    let resp_bytes = resp.body_bytes().await.map_err(Error::msg)?;
    postcard::from_bytes(&resp_bytes[..]).map_err(Error::msg)
}

pub async fn send_post_request_using_postcard<Req: Serialize, Resp: for<'de> Deserialize<'de>>(
    uri: &str,
    req: &Req,
) -> Result<Resp> {
    let mut resp = surf::post(uri)
        .body(surf::Body::from_bytes(postcard::to_allocvec(req)?))
        .await
        .map_err(Error::msg)?;
    check_resp!(resp);
    let resp_bytes = resp.body_bytes().await.map_err(Error::msg)?;
    postcard::from_bytes(&resp_bytes[..]).map_err(Error::msg)
}

pub async fn send_post_request_using_postcard_bytes<Resp: for<'de> Deserialize<'de>>(
    uri: &str,
    req: Bytes,
) -> Result<Resp> {
    let req_len = req.len();
    let mut resp = surf::post(uri)
        .body(surf::Body::from_reader(Cursor::new(req), Some(req_len)))
        .await
        .map_err(Error::msg)?;
    check_resp!(resp);
    let resp_bytes = resp.body_bytes().await.map_err(Error::msg)?;
    postcard::from_bytes(&resp_bytes[..]).map_err(Error::msg)
}

#[derive(Debug)]
struct PostcardDecodeError(postcard::Error);

impl Reject for PostcardDecodeError {}

pub fn warp_body_postcard<T: for<'de> Deserialize<'de> + Send>(
) -> impl Filter<Extract = (T,), Error = Rejection> + Copy {
    warp::filters::body::bytes().and_then(|buf: Bytes| async move {
        postcard::from_bytes(buf.bytes()).map_err(|err| {
            debug!("request postcard body error: {}", err);
            warp::reject::custom(PostcardDecodeError(err))
        })
    })
}

pub fn warp_reply_postcard<T: Serialize>(val: &T) -> impl warp::Reply {
    match postcard::to_allocvec(val) {
        Ok(buf) => {
            let mut resp = Response::new(hyper::Body::from(buf));
            resp.headers_mut().insert(
                http::header::CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
            resp
        }
        Err(e) => {
            error!("warp_reply_postcard error: {}", e);
            let mut resp = Response::new(hyper::Body::empty());
            *resp.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            resp
        }
    }
}
