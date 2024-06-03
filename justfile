# Run `cargo check`
check:
    ./run.sh cargo check

# Run `cargo clippy`
clippy:
    ./run.sh cargo clippy

# Run tests.
#
# Examples:
#
#     Run all tests, including integration tests:
#
#         just test
#
#     Run a specific unit test:
#
#         just test chrisdb_client::tests::test_query_for_existing
#
test test_name="": reset
    test_name={{test_name}}; ./run.sh cargo test $test_name

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

# Run the psql shell
psql:
    docker exec -it minichris-docker-db-1 psql postgresql://chris:chris1234@localhost:5432/chris
