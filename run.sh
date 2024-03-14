#!/bin/bash
# Purpose: run `cargo test` in a container, where the container has access to
#          the CUBE container's network and volumes.

HERE="$(dirname "$(readlink -f "$0")")"

if [ -z "$CI" ]; then
  TTY=-it
fi

cmd="$1"
shift

set -ex
cd "$HERE"

# create two named volumes called "cargo-oxidicom-target" and "cargo-oxidicom-home"
docker run \
    -v cargo-oxidicom-target:/target \
    -v cargo-oxidicom-home:/cargo \
    docker.io/library/rust:1.76-bookworm \
    chmod g+rwx /target /cargo

chmod g+r Cargo.lock Cargo.toml

# run container as the same container user as CUBE container, but also with host user
# group for permission to read files in $HERE
exec docker run --rm $TTY --name cargo-chris-scp -u 1001:0 --group-add "$(id -g)" \
  --net=host \
  -v cargo-oxidicom-target:/target \
  -v cargo-oxidicom-home:/cargo \
  -e CARGO_TARGET_DIR=/target \
  -e CARGO_HOME=/cargo \
  -v "$HERE:/src:ro" \
  -w /src \
  -v minichris-files:/data:rw \
  -e CHRIS_FILES_ROOT=/data \
  -e CHRIS_URL=http://localhost:8000/api/v1/ \
  -e CHRIS_USERNAME=chris \
  -e CHRIS_PASSWORD=chris1234 \
  -e PORT=11112 \
  -e OTEL_EXPORTER_OTLP_ENDPOINT=http://centurion.tch.harvard.edu:4318 \
  -e OTEL_RESOURCE_ATTRIBUTES=service.name=oxidicom-test \
  docker.io/library/rust:1.76-bookworm \
  cargo "$cmd" -- "$@"
