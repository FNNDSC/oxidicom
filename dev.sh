#!/bin/bash

HERE="$(dirname "$(readlink -f "$0")")"

set -ex
cd "$HERE"
cargo build
exec docker run --rm -it --name chris-scp -u 1001:0 \
  --net=minichris-docker_local \
  -v minichris-docker_chris_files:/data:rw \
  -v "$HERE/target:/target:ro" \
  docker.io/library/archlinux:latest \
  /target/debug/chris-scp "$@"
