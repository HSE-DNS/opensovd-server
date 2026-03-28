#!/bin/bash

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"

CDA_DIR="../classic-diagnostic-adapter"

# in das testcontainer Verzeichnis des CDA wechseln 
cd $SCRIPT_DIR/$CDA_DIR
cd testcontainer

# Testcontainer starten
docker-compose up -d
sleep 20 

# Token holen
export ACCESS_TOKEN=$(curl -s -X POST -H "Content-Type: application/json" "http://localhost:20002/vehicle/v15/authorize" --data '{"client_id":"test", "client_secret":"secret"}' | jq -r .access_token)

# SOVD Server mit diesem Token starten
cd $SCRIPT_DIR
CDA_TOKEN="$ACCESS_TOKEN" cargo run -p opensovd-gateway -- --url http://0.0.0.0:7690/sovd --cda-host localhost --cda-port 20002 --cda-base-path "/vehicle/v15"
