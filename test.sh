export RUST_BACKTRACE=full
cargo build

PORT=3000 ./target/debug/groupcache-app &
groupcache_zero=$!

PORT=3001 ./target/debug/groupcache-app &
groupcache_one=$!

PORT=3002 ./target/debug/groupcache-app &
groupcache_two=$!

sleep 1

curl --request PUT -sL \
    --url 'http://localhost:8000/peer/127.0.0.1:3001'

curl --request PUT -sL \
    --url 'http://localhost:8000/peer/127.0.0.1:3002'


curl --request PUT -sL \
    --url 'http://localhost:8001/peer/127.0.0.1:3000'

curl --request PUT -sL \
    --url 'http://localhost:8001/peer/127.0.0.1:3002'


curl --request PUT -sL \
    --url 'http://localhost:8002/peer/127.0.0.1:3001'

curl --request PUT -sL \
    --url 'http://localhost:8002/peer/127.0.0.1:3002'


curl --request GET -sL \
    --url 'http://localhost:8000/key/key-1' | jq

curl --request GET -sL \
    --url 'http://localhost:8000/key/key-1' | jq

time curl --request GET -sL \
    --url 'http://localhost:8000/key/key-1' | jq

time curl --request GET -sL \
    --url 'http://localhost:8000/key/error-1' | jq

curl --request GET -sL \
    --url 'http://localhost:8000/root'



kill -9 "${groupcache_zero}"
kill -9 "${groupcache_one}"
kill -9 "${groupcache_two}"




