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
- Observability: `oxidicom` outputs structured logs. I also plan to add OpenTelemetry metrics.
- Scalability: manual implementation of C-STORE makes `oxidicom` horizontally scalable (opposed to
  relying on dcmtk's `storescp`, which is harder to scale because it spawns subprocesses).

Prior to `oxidicom`, `pfdcm` was the major bottleneck in the _ChRIS_ PACS query/retrieval architecture.
Now, _CUBE_ is the bottleneck. See the section on [Performance Tuning](#performance-tuning) below.

## Environment Variables

| Name                          | Description                                                                                         |
|-------------------------------|-----------------------------------------------------------------------------------------------------|
| `CHRIS_DB_CONNECTION`         | PostgreSQL connection string                                                                        |
| `CHRIS_DB_POOL`               | Database connection pool size                                                                       |
| `CHRIS_FILES_ROOT`            | (required) Path to where _CUBE_'s storage is mounted                                                |
| `CHRIS_SCP_AET`               | DICOM AE title (hospital PACS pushing to `oxidicom` should be configured to push to this name)      |
| `CHRIS_SCP_STRICT`            | Whether receiving PDUs must not surpass the negotiated maximum PDU length.                          |
| `CHRIS_SCP_MAX_PDU_LENGTH`    | Maximum PDU length                                                                                  |
| `CHRIS_SCP_UNCOMPRESSED_ONLY` | Only accept native/uncompressed transfer syntaxes                                                   |                                                      
| `CHRIS_PACS_ADDRESS`          | PACS server addresses (optional, see [PACS address configuration](#pacs-address-configuration))     |
| `CHRIS_LISTENER_THREADS`      | Maximum number of concurrent SCU clients to handle. (see [Performance Tuning](#performance-tuning)) |
| `TOKIO_WORKER_THREADS`        | Number of threads to use for the async runtime                                                      |
| `CHRIS_VERBOSE`               | Set as `yes` to show debugging messages                                                             |
| `PORT`                        | TCP port number to listen on                                                                        |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OpenTelemetry Collector gRPC endpoint                                                               |
| `OTEL_RESOURCE_ATTRIBUTES`    | Resource attributes, e.g. `service.name=oxidicom-test`                                              |

## Performance Tuning

TODO

## Failure Modes

- An error with an individual instance does not terminate the association
  (meaning, subsequent instances will still have the chance to be received).
- Currently, the following tags are required:
  StudyInstanceUID, SeriesInstanceUID, SOPInstanceUID, PatientID, and StudyDate.
  If any of the tags are missing, the DICOM instance will not be stored.
- Files are first written to storage, then registered to CUBE. If CUBE does not
  accept the file registration, the file will still remain in storage.
- If an unknown SOP class UID is encountered, the SCU will (probably) choose to abort
  the association. In this case, `oxidicom` will be aware that the abortion and the
  OpenTelemetry span for this association will have `status=error`. This can maybe
  be resolved, see https://github.com/Enet4/dicom-rs/issues/477
- If _CUBE_'s response times are slow, then `oxidicom` will experience backpressure
  and its memory usage will start to balloon.
- If a PACS retrieve was triggered twice, even though the first one was successful,
  the file will be overwritten in CUBE's storage, but the second registration will fail.
  Assuming the file sent by PACS did not change, the operation is idempotent.

## "Oxidicom Custom Metadata" Spec

The _ChRIS_ API does not provide any mechanism for knowing when a DICOM series has been pulled in completion.
A DICOM series contains 0 or more DICOM instances. _CUBE_ tracks each DICOM instance individually, but _CUBE_
does not track how many instances _should_ there be for a series (`NumberOfSeriesRelatedInstances`).

https://github.com/FNNDSC/ChRIS_ultron_backEnd/issues/544

As a hacky workaround for this shortcoming, `oxidicom` will push dummy files into _CUBE_ as PACSFiles
under the space `SERVICES/PACS/org.fnndsc.oxidicom`. See [CUSTOM_SPEC.md](./CUSTOM_SPEC.md).

## PACS Address Configuration

The environment variable `CHRIS_PACS_ADDRESS` should be a comma-separated list of `key=value` pairs.
Blanks will be ignored (which implies that trailing comma is OK).

The PACS server address for a client AE title is used to lookup the `NumberOfSeriesRelatedInstances`.
For example, suppose `CHRIS_PACS_ADDRESS=BCH=1.2.3.4:4242`. When we receive DICOMs from `BCH`, `oxidicom`
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
just
```

The `just` command, without arguments, will:

1. Run Orthanc
2. Download sample data
3. Push sample data into Orthanc
4. Run integration tests

### Observability

`oxidicom` exports traces to OpenTelemetry collector. There is a span for the association
(TCP connection from PACS server to send us DICOM objects).

### Usage of `opentelemetry` v.s. `tracing` in the codebase

`dicom-rs` itself uses the `tracing` crate, though for the spans described above,
I decided to use the `opentelemetry` crate.

### Sample DICOM files

See https://github.com/FNNDSC/sample_dicom_downloader
