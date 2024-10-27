#!/bin/bash

# Otherwise, you will have to type in the following command to upload the wasm binary to the network:
RES=$(../mantrachaind tx wasm store artifacts/rwa_protocol_contracts.wasm --from wallet --node https://rpc.dukong.mantrachain.io:443 --chain-id mantra-dukong-1 --gas-prices 0.01uom --gas auto --gas-adjustment 2 -y --output json)
# The response contains the Code Id of the uploaded wasm binary.
echo $RES

# Get the Transaction Hash from the response
TX_HASH=$(echo $RES | jq -r .txhash)

# Get the full transaction details with events 
CODE_ID=$(../mantrachaind query tx $TX_HASH --node https://rpc.dukong.mantrachain.io:443 -o json| jq -r '.logs[0].events[] | select(.type == "store_code") | .attributes[] | select(.key == "code_id") | .value')

echo $CODE_ID