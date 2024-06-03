#!/bin/bash
# Purpose: run `cargo test` in a container, where the container has access to
#          the CUBE container's network and volumes.

HERE="$(dirname "$(readlink -f "$0")")"
cd "$HERE"

chmod g+r Cargo.lock Cargo.toml

if [ "$CI" = "true" ]; then
  notty='-T'
fi

export GID=$(id -g)
exec docker compose --profile oxidicom run --rm --use-aliases $notty oxidicom "$@"
