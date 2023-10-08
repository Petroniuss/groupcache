use crate::{Groupcache, ValueBounds};
use async_trait::async_trait;
use groupcache_pb::groupcache_pb::{
    groupcache_server, GetRequest, GetResponse, RemoveRequest, RemoveResponse,
};
use tonic::{Request, Response, Status};

#[async_trait]
impl<Value: ValueBounds> groupcache_server::Groupcache for Groupcache<Value> {
    async fn get(&self, request: Request<GetRequest>) -> Result<Response<GetResponse>, Status> {
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
        todo!()
    }
}
