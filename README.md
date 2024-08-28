# oxidicom

[![GitHub tag](https://img.shields.io/github/v/tag/FNNDSC/oxidicom?filter=v*.*.*&label=version)](https://github.com/FNNDSC/oxidicom/pkgs/container/oxidicom)
[![MIT License](https://img.shields.io/github/license/fnndsc/oxidicom)](https://github.com/FNNDSC/oxidicom/blob/master/LICENSE)
[![CI](https://github.com/FNNDSC/oxidicom/actions/workflows/ci.yml/badge.svg)](https://github.com/FNNDSC/oxidicom/actions/workflows/ci.yml)

_oxidicom_ is a high-performance DICOM receiver for the
[_ChRIS_ backend](https://github.com/FNNDSC/ChRIS_ultron_backEnd) (CUBE).

More technically, _oxidicom_ implements a DICOM C-STORE service class provider (SCP),
meaning it is a "server" which receives DICOM data over TCP. For every DICOM file received,
_oxidicom_ writes it to the storage of _CUBE_ and "registers" the file with _CUBE_.

## Environment Variables

Only `OXIDICOM_AMQP_ADDRESS` and `OXIDICOM_FILES_ROOT` are required. Those configure how oxidicom connects to _CUBE_.
The other variables are either for optional features or performance tuning.

| Name                             | Description                                                                                         |
|----------------------------------|-----------------------------------------------------------------------------------------------------|
| `OXIDICOM_AMQP_ADDRESS`          | (required) AMQP address of the RabbitMQ used by _CUBE_'s celery workers                             |
| `OXIDICOM_FILES_ROOT`            | (required) Path to where _CUBE_'s storage is mounted                                                |
| `OXIDICOM_PROGRESS_NATS_ADDRESS` | (optional) NATS server where to send progress messages                                              |
| `OXIDICOM_PROGRESS_INTERVAL_MS`  | Minimum delay between progress messages per study                                                   |
| `OXIDICOM_SCP_AET`               | DICOM AE title (hospital PACS pushing to `oxidicom` should be configured to push to this name)      |
| `OXIDICOM_SCP_STRICT`            | Whether receiving PDUs must not surpass the negotiated maximum PDU length.                          |
| `OXIDICOM_SCP_UNCOMPRESSED_ONLY` | Only accept native/uncompressed transfer syntaxes                                                   |                                                      
| `OXIDICOM_SCP_PROMISCUOUS`       | Whether to accept unknown abstract syntaxes.                                                        |
| `OXIDICOM_SCP_MAX_PDU_LENGTH`    | Maximum PDU length                                                                                  |
| `OXIDICOM_PACS_ADDRESS`          | PACS server addresses (recommended, see [PACS address configuration](#pacs-address-configuration))  |
| `OXIDICOM_LISTENER_THREADS`      | Maximum number of concurrent SCU clients to handle. (see [Performance Tuning](#performance-tuning)) |
| `OXIDICOM_LISTENER_PORT`         | TCP port number to listen on                                                                        |
| `OXIDICOM_VERBOSE`               | Set as `yes` to show debugging messages                                                             |
| `TOKIO_WORKER_THREADS`           | Number of threads to use for the async runtime                                                      |
| `OTEL_EXPORTER_OTLP_ENDPOINT`    | OpenTelemetry Collector gRPC endpoint                                                               |
| `OTEL_RESOURCE_ATTRIBUTES`       | Resource attributes, e.g. `service.name=oxidicom-test`                                              |

See [src/settings.rs](src/settings.rs) for the source of truth on the table above and default values of optional settings.

## Performance Tuning

Behind the scenes, _oxidicom_ has three components connected by asynchronous channels:

1. listener: receives DICOM objects over TCP
2. writer: writes DICOM objects to storage
3. sender: emits progress messages to NATS and series registration jobs to celery

`OXIDICOM_LISTENER_THREADS` controls the parallelism of the listener, whereas
`TOKIO_WORKER_THREADS` controls the async runtime's thread pool which is shared
between the writer and registerer. (The reason why we have two thread pools is
an implementation detail: the Rust ecosystem suffers from a sync/async divide.)

## Scaling

Large amounts of incoming data can be handled by horizontally scaling _oxidicom_.
It is easy to increase its number of replicas. However, the task queue for
registering the data to _CUBE_ will fill up. If you try to increase the number of
_CUBE_ celery workers, then the PostgreSQL database will get strained.

## Failure Modes

_oxidicom_ is designed to be fault-tolerant. For instance, an error with an individual
DICOM instance does not terminate the association (meaning, subsequent DICOM
instances will still have the chance to be received).

No assumptions are made about the PACS being well-behaved. _oxidicom_ does not care
if the PACS sends illegal data (e.g. the wrong number of DICOM instances for a series).

Receiving the same DICOM data more than once will overwrite the existing file in storage,
and another task to register the series will be sent to _CUBE_'s celery workers. _CUBE_'s
workers are going to throw an error when this happens. The overall behavior is idempotent.

## PACS Address Configuration

The environment variable `OXIDICOM_PACS_ADDRESS` should be a dictionary of AE titles to their IPv4 sockets
(IP address and port number).

The PACS server address for a client AE title is used to look up the `NumberOfSeriesRelatedInstances`.
For example, suppose `OXIDICOM_PACS_ADDRESS={BCH="1.2.3.4:4242"}`. When we receive DICOMs from `BCH`, `oxidicom`
will do a C-FIND to `1.2.3.4:4242`, asking them what is the `NumberOfSeriesRelatedInstances` for the
received DICOMs. When we receive DICOMs from `MGH`, the PACS address is unknown, so `oxidicom` will set
`NumberOfSeriesRelatedInstances=unknown`.

## Development

The development scripts are hard-coded to work with an instance of _miniChRIS_.
Follow these instructions to spin up the backend: 
https://github.com/FNNDSC/miniChRIS-docker#readme

To speak to _CUBE_, `oxidicom` needs to run in a Docker container in the same network and mounting
the same volume as _CUBE_'s container. This is coded up in `./docker-compose.yml`.

You need to have installed:

- Docker Compose
- https://github.com/casey/just

Simply run

```shell
just test
```

The `just` command, without arguments, will:

1. Run Orthanc
2. Download sample data
3. Push sample data into Orthanc
4. Run unit and integration tests

### Observability

`oxidicom` exports traces to OpenTelemetry collector. There is a span for the association
(TCP connection from PACS server to send us DICOM objects).

### Usage of `opentelemetry` v.s. `tracing` in the codebase

`dicom-rs` itself uses the `tracing` crate, though for the spans described above,
I decided to use the `opentelemetry` crate for spans, and `tracing` for logs.

### Sample DICOM files

See https://github.com/FNNDSC/sample_dicom_downloader
