# Run integration tests
test: reset
    ./run.sh cargo test

# Run in debug mode
run:
    ./run.sh cargo run

# Stop the run server
kill:
    docker compose kill oxidicom

# Delete all PACSFiles from CUBE
reset:
    ./reset.sh

# Start Orthanc
orthanc:
    docker compose up -d

# Start an observability stack for distributed tracing
observe:
    docker compose --profile observe up -d

# Remove all data and containers
down:
    docker compose --profile observe down -v
