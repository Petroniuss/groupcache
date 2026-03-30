#!/usr/bin/env bash
set -euo pipefail

CORE_FILE="$1"

if [ -z "$CORE_FILE" ] || [ ! -f "$CORE_FILE" ]; then
    echo "Usage: $0 <path-to-maelstrom-core.clj>"
    exit 1
fi

# Check if already patched
if grep -q "cache-convergence" "$CORE_FILE"; then
    echo "Already patched."
    exit 0
fi

# The require block uses nested form:
#   [maelstrom.workload [broadcast :as broadcast]
#                       [echo :as echo]
#                       ...
#                       [unique-ids :as unique-ids]]
#
# Add our workload after unique-ids, keeping the closing ]]
sed -i.bak 's/\[unique-ids :as unique-ids\]\]/[unique-ids :as unique-ids]\
                                [cache-convergence :as cache-convergence]]/' "$CORE_FILE"

# The workloads map uses keywords:
#   {:broadcast       broadcast/workload
#    ...
#    :unique-ids      unique-ids/workload})
#
# Add our entry before the closing })
sed -i.bak 's/:unique-ids      unique-ids\/workload})/:unique-ids      unique-ids\/workload\
   :cache-convergence cache-convergence\/workload})/' "$CORE_FILE"

# Clean up backup files
rm -f "${CORE_FILE}.bak"

echo "Patched $CORE_FILE with cache-convergence workload."
