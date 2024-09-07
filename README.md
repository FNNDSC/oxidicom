# oxidicom

[![GitHub tag](https://img.shields.io/github/v/tag/FNNDSC/oxidicom?filter=v*.*.*&label=version)](https://github.com/FNNDSC/oxidicom/pkgs/container/oxidicom)
[![MIT License](https://img.shields.io/github/license/fnndsc/oxidicom)](https://github.com/FNNDSC/oxidicom/blob/master/LICENSE)
[![CI](https://github.com/FNNDSC/oxidicom/actions/workflows/ci.yml/badge.svg)](https://github.com/FNNDSC/oxidicom/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/FNNDSC/oxidicom/graph/badge.svg?token=24J11SWJEA)](https://codecov.io/gh/FNNDSC/oxidicom)

_oxidicom_ is a high-performance DICOM receiver for the
[_ChRIS_ backend](https://github.com/FNNDSC/ChRIS_ultron_backEnd) (CUBE).

Documentation: https://chrisproject.org/docs/oxidicom

## Development

You'll need: [Docker Compose](https://docs.docker.com/compose/) and [Rust](https://rustup.rs/).

Start [RabbitMQ](https://hub.docker.com/_/rabbitmq) and [Orthanc](https://www.orthanc-server.com/)
services for testing, then download test data:

```shell
docker compose run --rm get-data
```

Run all tests:

```shell
cargo test
```

Clean up:

```shell
docker compose down -v
```

## Notes

### Usage of `opentelemetry` v.s. `tracing` in the codebase

`dicom-rs` itself uses the `tracing` crate, though for the spans described above,
I decided to use the `opentelemetry` crate for spans, and `tracing` for logs.

### Sample DICOM files

See https://github.com/FNNDSC/sample_dicom_downloader
