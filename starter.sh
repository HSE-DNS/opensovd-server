#!/bin/bash

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"

CDA_DIR="../classic-diagnostic-adapter"
PID_FILE="$SCRIPT_DIR/.sovd-gateway.pid"


start() {
    # in das testcontainer Verzeichnis des CDA wechseln 
    cd $SCRIPT_DIR/$CDA_DIR/testcontainer

    # Testcontainer starten
    echo "starting CDA testcontainer..."
    docker-compose up -d
    sleep 20 

    # Token holen
    echo "getting CDA access token..."
    export ACCESS_TOKEN=$(curl -s -X POST -H "Content-Type: application/json" "http://localhost:20002/vehicle/v15/authorize" --data '{"client_id":"test", "client_secret":"secret"}' | jq -r .access_token)

    # SOVD Server mit diesem Token starten
    echo "starting SOVD server..."
    cd $SCRIPT_DIR
    cargo build -p opensovd-gateway
    CDA_TOKEN="$ACCESS_TOKEN" ./target/debug/opensovd-gateway --url http://0.0.0.0:7690/sovd --cda-host localhost --cda-port 20002 --cda-base-path "/vehicle/v15" & echo $! > $PID_FILE
    echo "SOVD server started successfully, PID $(cat $PID_FILE)"
}

stop() {
    echo "stopping SOVD server..."
    if [ -f "$PID_FILE" ]; then
        kill $(cat $PID_FILE) 2>/dev/null || true
        rm $PID_FILE
    fi
    echo "SOVD server shut down successfully"

    echo "stopping CDA testcontainer..."
    cd $SCRIPT_DIR/$CDA_DIR/testcontainer
    docker-compose down
    echo "CDA testcontainer shut down successfully"
}

status() {
    cd $SCRIPT_DIR/$CDA_DIR/testcontainer
    docker-compose ps
}

case "$1" in 
    start)
       start
       ;;
    stop)
       stop
       ;;
    restart)
       stop
       start
       ;;
    status)
       status
       ;;
    *)
       echo "Usage: $0 can be started with the follwing arguments: {start|stop|status|restart}."
esac

exit 0 