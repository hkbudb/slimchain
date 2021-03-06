use async_trait::async_trait;
use core::marker::PhantomData;
use futures::prelude::*;
use libp2p::{
    core::upgrade::{read_varint, write_varint},
    request_response::RequestResponseCodec,
};
use serde::{Deserialize, Serialize};
use slimchain_utils::serde::{binary_decode, binary_encode};
use std::io;

/// Encode/decode the request and response to/from the network.
#[derive(Copy)]
pub struct RpcCodec<Req, Resp>
where
    Req: Serialize + for<'de> Deserialize<'de> + Send + 'static,
    Resp: Serialize + for<'de> Deserialize<'de> + Send + 'static,
{
    _marker: PhantomData<(Req, Resp)>,
}

impl<Req, Resp> Default for RpcCodec<Req, Resp>
where
    Req: Serialize + for<'de> Deserialize<'de> + Send + 'static,
    Resp: Serialize + for<'de> Deserialize<'de> + Send + 'static,
{
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<Req, Resp> Clone for RpcCodec<Req, Resp>
where
    Req: Serialize + for<'de> Deserialize<'de> + Send + 'static,
    Resp: Serialize + for<'de> Deserialize<'de> + Send + 'static,
{
    fn clone(&self) -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

#[async_trait]
impl<Req, Resp> RequestResponseCodec for RpcCodec<Req, Resp>
where
    Req: Serialize + for<'de> Deserialize<'de> + Send + 'static,
    Resp: Serialize + for<'de> Deserialize<'de> + Send + 'static,
{
    type Protocol = crate::p2p::rpc::RpcProtocol;
    type Request = Req;
    type Response = Resp;

    async fn read_request<Socket>(
        &mut self,
        _: &Self::Protocol,
        socket: &mut Socket,
    ) -> io::Result<Self::Request>
    where
        Socket: AsyncRead + Unpin + Send,
    {
        let len = read_varint(socket).await?;
        let mut buf = vec![0; len];
        socket.read_exact(&mut buf).await?;
        binary_decode(buf.as_ref()).map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    async fn read_response<Socket>(
        &mut self,
        _: &Self::Protocol,
        socket: &mut Socket,
    ) -> io::Result<Self::Response>
    where
        Socket: AsyncRead + Unpin + Send,
    {
        let len = read_varint(socket).await?;
        let mut buf = vec![0; len];
        socket.read_exact(&mut buf).await?;
        binary_decode(buf.as_ref()).map_err(|e| io::Error::new(io::ErrorKind::Other, e))
    }

    async fn write_request<Socket>(
        &mut self,
        _: &Self::Protocol,
        socket: &mut Socket,
        request: Self::Request,
    ) -> io::Result<()>
    where
        Socket: AsyncWrite + Unpin + Send,
    {
        let bin = binary_encode(&request).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        write_varint(socket, bin.len()).await?;
        socket.write_all(bin.as_ref()).await?;
        socket.close().await?;
        Ok(())
    }

    async fn write_response<Socket>(
        &mut self,
        _: &Self::Protocol,
        socket: &mut Socket,
        response: Self::Response,
    ) -> io::Result<()>
    where
        Socket: AsyncWrite + Unpin + Send,
    {
        let bin = binary_encode(&response).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        write_varint(socket, bin.len()).await?;
        socket.write_all(bin.as_ref()).await?;
        socket.close().await?;
        Ok(())
    }
}
