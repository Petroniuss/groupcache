//! gRPC [groupcache_pb::GroupcacheServer] implementation

use crate::groupcache::ValueBounds;
use crate::metrics::METRIC_GET_SERVER_REQUESTS_TOTAL;
use crate::GroupcacheInner;
use async_trait::async_trait;
use groupcache_pb::{
    GetRequest, GetResponse, Groupcache, InvalidationEvent, RemoveRequest, RemoveResponse,
    WatchRequest,
};
use metrics::counter;
use std::pin::Pin;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};

#[async_trait]
impl<Value: ValueBounds> Groupcache for GroupcacheInner<Value> {
    async fn get(&self, request: Request<GetRequest>) -> Result<Response<GetResponse>, Status> {
        counter!(METRIC_GET_SERVER_REQUESTS_TOTAL).increment(1);

        let payload = request.into_inner();
        match self.get(&payload.key).await {
            Ok(value) => {
                match crate::codec::serialize(&value) {
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

    type WatchInvalidationsStream =
        Pin<Box<dyn Stream<Item = Result<InvalidationEvent, Status>> + Send>>;

    async fn watch_invalidations(
        &self,
        _request: Request<WatchRequest>,
    ) -> Result<Response<Self::WatchInvalidationsStream>, Status> {
        let stream = self.invalidation.subscribe_stream();
        Ok(Response::new(Box::pin(stream)))
    }
}
