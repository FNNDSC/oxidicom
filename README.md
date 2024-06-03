# oxidicom

[![GitHub tag](https://img.shields.io/github/v/tag/FNNDSC/oxidicom?filter=v*.*.*&label=version)](https://github.com/FNNDSC/oxidicom/pkgs/container/oxidicom)
[![MIT License](https://img.shields.io/github/license/fnndsc/oxidicom)](https://github.com/FNNDSC/oxidicom/blob/master/LICENSE)
[![CI](https://github.com/FNNDSC/oxidicom/actions/workflows/ci.yml/badge.svg)](https://github.com/FNNDSC/oxidicom/actions/workflows/ci.yml)

`oxidicom` is a high-performance DICOM receiver for the
[_ChRIS_ backend](https://github.com/FNNDSC/ChRIS_ultron_backEnd) (CUBE).
It **partially** replaces [pfdcm](https://github.com/FNNDSC/pfdcm).

More technically, `oxidicom` implements a DICOM C-STORE service class provider (SCP),
a "server," which listens for incoming DICOM files. For every DICOM file received,
it writes it to the storage of _CUBE_ and "registers" the file with _CUBE_.

## Improvements over pfdcm

Rewriting the functionality of `pfdcm` in Rust and with a modern design has led to several advantages:

- Performance: registration of retrieved DICOM files to _CUBE_ happens in real-time instead of being
  done in stages and polled until completion.
- Simplicity: client can simply check for the number of PACS files existing in CUBE (for a given
  SeriesInstanceUID) instead of having to ask pfdcm for intermediate progress information (and having
  to poll pfdcm to completion).
- Observability: `oxidicom` outputs structured logs and also sends traces to OpenTelemetry collector.
- Scalability: manual implementation of C-STORE makes `oxidicom` horizontally scalable (opposed to
  relying on dcmtk's `storescp`, which is harder to scale because it spawns subprocesses).

Prior to `oxidicom`, `pfdcm` was the major bottleneck in the _ChRIS_ PACS query/retrieval architecture.
Prior to `oxidicom` version 2, [CUBE was the bottleneck](https://github.com/FNNDSC/ChRIS_ultron_backEnd/issues/546).
Since `oxidicom` version 2, the _ChRIS_ architecture is fully able to keep up with user requests and
the data being sent to it from PACS, being capable of receiving >1,000s of DICOM files per second (with good hardware).

## Environment Variables

Only `OXIDICOM_DB_CONNECTION` and `OXIDICOM_FILES_ROOT` are required. Those configure how oxidicom connects to CUBE.
The other variables are either for optional features or performance tuning.

| Name                             | Description                                                                                         |
|----------------------------------|-----------------------------------------------------------------------------------------------------|
| `OXIDICOM_DB_CONNECTION`         | (required) PostgreSQL connection string                                                             |
| `OXIDICOM_DB_POOL`               | Database connection pool size                                                                       |
| `OXIDICOM_DB_BATCH_SIZE`         | Maximum number of files to register per request                                                     |
| `OXIDICOM_FILES_ROOT`            | (required) Path to where _CUBE_'s storage is mounted                                                |
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
3. registerer: writes DICOM metadata to CUBE's database

`OXIDICOM_LISTENER_THREADS` controls the parallelism of the listener, whereas
`TOKIO_WORKER_THREADS` controls the async runtime's thread pool which is shared
between the writer and registerer. (The reason why we have two thread pools is
an implementation detail: the Rust ecosystem suffers from a sync/async divide.)

## Failure Modes

`oxidicom` is designed to be fault-tolerant. Furthermore, it makes few assumptions
about whether the PACS is well-behaved. For instance, an error with an individual
DICOM instance does not terminate the association (meaning, subsequent DICOM
instances will still have the chance to be received).

Receiving the same DICOM data is idempotent. The database row will not be overwritten.
The duplicate DICOMs will be indicated in a corresponding OpenTelemetry span attribute.

## "Oxidicom Custom Metadata" Spec

The _ChRIS_ API does not provide any mechanism for knowing when a DICOM series has been pulled in completion.
A DICOM series contains 0 or more DICOM instances. _CUBE_ tracks each DICOM instance individually, but _CUBE_
does not track how many instances _should_ there be for a series (`NumberOfSeriesRelatedInstances`).

https://github.com/FNNDSC/ChRIS_ultron_backEnd/issues/544

As a hacky workaround for this shortcoming, `oxidicom` will push dummy files into _CUBE_ as PACSFiles
under the space `SERVICES/PACS/org.fnndsc.oxidicom`. See [CUSTOM_SPEC.md](./CUSTOM_SPEC.md).

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
