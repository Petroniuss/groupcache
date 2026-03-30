use async_trait::async_trait;
use groupcache_maelstrom::handler::handle_message;
use groupcache_maelstrom::node::GroupcacheNode;
use maelstrom::protocol::Message;
use maelstrom::{Node, Result, Runtime};
use std::sync::Arc;
use tokio::sync::OnceCell;

static NODE: OnceCell<Arc<GroupcacheNode>> = OnceCell::const_new();

#[derive(Clone, Default)]
struct Handler;

#[async_trait]
impl Node for Handler {
    async fn process(&self, runtime: Runtime, req: Message) -> Result<()> {
        let node = NODE
            .get_or_init(|| async {
                let node_id = runtime.node_id().to_string();
                let node_ids: Vec<String> =
                    runtime.nodes().iter().map(|s| s.to_string()).collect();
                Arc::new(GroupcacheNode::new(node_id, node_ids))
            })
            .await;

        handle_message(node, &runtime, &req).await
    }
}

fn main() -> Result<()> {
    Runtime::init(try_main())
}

async fn try_main() -> Result<()> {
    let handler = Arc::new(Handler);
    Runtime::new().with_handler(handler).run().await
}
