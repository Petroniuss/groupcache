export RUST_BACKTRACE=full
export RUST_LOG=info

cargo build || {
  exit 1;
}

# run three groupcache instances
PORT=3000 ./target/debug/groupcache-app &
groupcache_zero=$!

PORT=3001 ./target/debug/groupcache-app &
groupcache_one=$!

PORT=3002 ./target/debug/groupcache-app &
groupcache_two=$!

# wait for them to start listening
sleep 2

# notify every instance about every other instance
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

# query instances for cached/to be computed values
curl --request GET -sL \
    --url 'http://localhost:3000/key/key-1' | jq

curl --request GET -sL \
    --url 'http://localhost:3000/key/key-1' | jq

curl --request GET -sL \
    --url 'http://localhost:3000/key/key-1' | jq

curl --request GET -sL \
    --url 'http://localhost:3000/key/error-1' | jq

# kill all instances
kill -9 "${groupcache_zero}"
kill -9 "${groupcache_one}"
kill -9 "${groupcache_two}"
