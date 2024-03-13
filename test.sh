#!/bin/bash
# Purpose: run `cargo test` in a container, where the container has access to
#          the CUBE container's network and volumes.

HERE="$(dirname "$(readlink -f "$0")")"

if [ -z "$CI" ]; then
  TTY=-it
fi

set -ex
cd "$HERE"

# create a volume called "cargo-oxidicom-target" that is read-writable for group
docker run -v cargo-oxidicom-target:/target docker.io/library/rust:1.76-bookworm \
    chmod g+rwx /target

# run container as the same container user as CUBE container, but also with host user
# group for permission to read files in $HERE
exec docker run --rm $TTY --name cargo-chris-scp -u 1001:0 --group-add "$(id -g)" \
  --net=minichris-docker_local \
  -v minichris-docker_chris_files:/data:rw \
  -v "$HERE:/src:ro" \
  -v cargo-oxidicom-target:/target:rw \
  -w /src \
  -e CARGO_TARGET_DIR=/target \
  -e CHRIS_URL=http://chris:8000/api/v1/ \
  -e CHRIS_USERNAME=chris \
  -e CHRIS_PASSWORD=chris1234 \
  -e CHRIS_FILES_ROOT=/data \
  -p 11112:11112 \
  docker.io/library/rust:1.76-bookworm \
  cargo test -- "$@"
