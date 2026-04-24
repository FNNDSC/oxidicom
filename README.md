# oxidicom

[![GitHub tag](https://img.shields.io/github/v/tag/FNNDSC/oxidicom?filter=v*.*.*&label=version)](https://github.com/FNNDSC/oxidicom/pkgs/container/oxidicom)
[![MIT License](https://img.shields.io/github/license/fnndsc/oxidicom)](https://github.com/FNNDSC/oxidicom/blob/master/LICENSE)
[![CI](https://github.com/FNNDSC/oxidicom/actions/workflows/ci.yml/badge.svg)](https://github.com/FNNDSC/oxidicom/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/FNNDSC/oxidicom/graph/badge.svg?token=24J11SWJEA)](https://codecov.io/gh/FNNDSC/oxidicom)

_oxidicom_ is a high-performance DICOM receiver for the
[_ChRIS_ backend](https://github.com/FNNDSC/ChRIS_ultron_backEnd) (CUBE).

Documentation: https://chrisproject.org/docs/oxidicom

## Development

You'll need: [podman-compose](https://github.com/containers/podman-compose) (or [Docker Compose](https://docs.docker.com/compose/))
and [Rust](https://rustup.rs/) (or use [`nix develop`](https://nix.dev/manual/nix/2.34/command-ref/new-cli/nix3-develop)).

Start [DragonflyDB](https://github.com/dragonflydb/dragonfly/pkgs/container/dragonfly) and [Orthanc](https://www.orthanc-server.com/)
services for testing, then download test data:

```shell
podman-compose up -d
podman-compose --profile tools run --rm get-data
```

Run all tests:

```shell
export RUST_LOG=oxidicom=debug,integration_test=debug  # optional
cargo test
```

Clean up:

```shell
podman-compose down -v
```

## Notes

### Usage of `opentelemetry` v.s. `tracing` in the codebase

`dicom-rs` itself uses the `tracing` crate, though for the spans described above,
I decided to use the `opentelemetry` crate for spans, and `tracing` for logs.

### Sample DICOM files

See https://github.com/FNNDSC/sample_dicom_downloader
