use crate::{Groupcache, GroupcacheWrapper, ValueBounds};
use anyhow::Result;
use async_trait::async_trait;
use groupcache_pb::groupcache_pb::{groupcache_server, GetRequest, GetResponse};
use tonic::{Request, Response, Status};
use tracing::info;

// todo: make it possible to run groupcache in axum.
// or run groupcache separately.
// todo: address here should be different from Peer.
pub async fn start_grpc_server<Value: ValueBounds>(
    groupcache: GroupcacheWrapper<Value>,
) -> Result<()> {
    let addr = groupcache.0.me.socket;
    info!("Groupcache server listening on {}", addr);

    tonic::transport::Server::builder()
        .add_service(groupcache.server())
        .serve(addr)
        .await?;

    Ok(())
}

#[async_trait]
impl<Value: ValueBounds> groupcache_server::Groupcache for Groupcache<Value> {
    async fn get(
        &self,
        request: Request<GetRequest>,
    ) -> std::result::Result<Response<GetResponse>, Status> {
        let payload = request.into_inner();

        match self.get(&payload.key).await {
            Ok(value) => {
                let result = rmp_serde::to_vec(&value);
                match result {
                    Ok(bytes) => Ok(Response::new(GetResponse { value: Some(bytes) })),
                    Err(err) => Err(Status::internal(err.to_string())),
                }
            }
            Err(err) => Err(Status::internal(err.to_string())),
        }
    }
}
