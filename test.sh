#!/bin/bash
# Purpose: run `cargo test` in a container, where the container has access to
#          the CUBE container's network and volumes.

HERE="$(dirname "$(readlink -f "$0")")"

if [ -z "$CI" ]; then
  TTY=-it
fi

set -ex
cd "$HERE"

# create two named volumes called "cargo-oxidicom-target" and "cargo-oxidicom-home"
docker run \
    -v cargo-oxidicom-target:/target \
    -v cargo-oxidicom-home:/cargo \
    docker.io/library/rust:1.76-bookworm \
    chmod g+rwx /target /cargo

# run container as the same container user as CUBE container, but also with host user
# group for permission to read files in $HERE
exec docker run --rm $TTY --name cargo-chris-scp -u 1001:0 --group-add "$(id -g)" \
  --net=minichris-local \
  -v minichris-files:/data:rw \
  -v cargo-oxidicom-target:/target \
  -v cargo-oxidicom-home:/cargo \
  -e CARGO_TARGET_DIR=/target \
  -e CARGO_HOME=/cargo \
  -v "$HERE:/src:ro" \
  -w /src \
  docker.io/library/rust:1.76-bookworm \
  cargo test -- "$@"
