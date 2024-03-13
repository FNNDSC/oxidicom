#!/bin/bash

HERE="$(dirname "$(readlink -f "$0")")"

set -ex
cd "$HERE"
cargo build
exec docker run --rm -it --name chris-scp -u 1001:0 \
  --net=minichris-docker_local \
  -v minichris-docker_chris_files:/data:rw \
  -v "$HERE/target:/target:ro" \
  -e CHRIS_URL=http://chris:8000/api/v1/ \
  -e CHRIS_USERNAME=chris \
  -e CHRIS_PASSWORD=chris1234 \
  -e CHRIS_FILES_ROOT=/data \
  -p 11111:11111 \
  docker.io/library/archlinux:latest \
  /target/debug/chris-scp "$@"
