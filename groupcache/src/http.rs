//! gRPC [groupcache_pb::GroupcacheServer] implementation

use crate::groupcache::ValueBounds;
use crate::metrics::METRIC_GET_SERVER_REQUESTS_TOTAL;
use crate::GroupcacheInner;
use async_trait::async_trait;
use groupcache_pb::{GetRequest, GetResponse, Groupcache, RemoveRequest, RemoveResponse};
use metrics::counter;
use tonic::{Request, Response, Status};

#[async_trait]
impl<Value: ValueBounds> Groupcache for GroupcacheInner<Value> {
    async fn get(&self, request: Request<GetRequest>) -> Result<Response<GetResponse>, Status> {
        counter!(METRIC_GET_SERVER_REQUESTS_TOTAL).increment(1);

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

    async fn remove(
        &self,
        request: Request<RemoveRequest>,
    ) -> Result<Response<RemoveResponse>, Status> {
        let payload = request.into_inner();

        match self.remove(&payload.key).await {
            Ok(_) => Ok(Response::new(RemoveResponse {})),
            Err(err) => Err(Status::internal(err.to_string())),
        }
    }
}
