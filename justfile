# Run integration tests
test: orthanc
    ./run.sh cargo test

# Run in debug mode
run: orthanc
    ./run.sh cargo run

# Stop the run server
kill:
    docker compose kill oxidicom

# Delete all PACSFiles from CUBE
reset:
    ./reset.sh

# Start Orthanc and download sample data, if necessary
orthanc: orthanc-up
    ./get_data.sh

# Start Orthanc
orthanc-up:
    docker compose up -d

# Start an observability stack for distributed tracing
observe:
    docker compose --profile observe up -d

# Remove all data and containers
down:
    docker compose --profile observe down -v
