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

## Environment Variables

| Name                          | Description                                                                                             |
|-------------------------------|---------------------------------------------------------------------------------------------------------|
| `CHRIS_URL`                   | (required) CUBE `v1/api/` URL                                                                           |
| `CHRIS_USERNAME`              | (required) Username of user to do PACSFile registration. Note: CUBE requires the username to be "chris" |
| `CHRIS_PASSWORD`              | (required) User password                                                                                |
| `CHRIS_FILES_ROOT`            | (required) Path to where _CUBE_'s storage is mounted                                                    |
| `CHRIS_HTTP_RETRIES`          | Number of times to retry failed HTTP request to CUBE                                                    |
| `CHRIS_PACS_NAME`             | Name of the PACS server pushing to `oxidicom`. (Should be configured as the same for `pfdcm`            |
| `CHRIS_SCP_AET`               | DICOM AET name (PACS pushing to `oxidicom` should be configured to push to this name)                   |
| `CHRIS_SCP_STRICT`            | Whether receiving PDUs must not surpass the negotiated maximum PDU length.                              |
| `CHRIS_SCP_MAX_PDU_LENGTH`    | Maximum PDU length                                                                                      |
| `CHRIS_SCP_UNCOMPRESSED_ONLY` | Only accept native/uncompressed transfer syntaxes                                                       |                                                      
| `CHRIS_SCP_THREADS`           | Connection thread pool size                                                                             |
| `CHRIS_VERBOSE`               | Set as `yes` to show debugging messages                                                                 |
| `PORT`                        | TCP port number to listen on                                                                            |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OpenTelemetry Collector HTTP endpoint                                                                   |
| `OTEL_RESOURCE_ATTRIBUTES`    | Resource attributes, e.g. `service.name=oxidicom-test`                                                  |

## Failure Modes

- An error with an individual instance does not terminate the association
  (meaning, subsequent instances will still have the chance to be received).
- Currently, the following tags are required:
  StudyInstanceUID, SeriesInstanceUID, SOPInstanceUID, PatientID, and StudyDate.
  If any of the tags are missing, the DICOM instance will not be stored.
- Files are first written to storage, then registered to CUBE. If CUBE does not
  accept the file registration, the file will still remain in storage.
- Registering the file to _CUBE_ is synchronous with file reception from the SCU.
  If _CUBE_'s response is slow, the SCU might time out the connection and disconnect,
  so all subsequent instances will be lost. Async functionality is blocked, see
  https://github.com/Enet4/dicom-rs/issues/476
- If an unknown SOP class UID is encountered, the SCU will (probably) choose to abort
  the association. In this case, `oxidicom` will be aware that the abortion and the
  OpenTelemetry span for this association will have `status=error`. This can maybe
  be resolved, see https://github.com/Enet4/dicom-rs/issues/477

## Development

The development scripts are hard-coded to work with an instance of _miniChRIS_.
Follow these instructions to spin up the backend: 
https://github.com/FNNDSC/miniChRIS-docker#readme

To speak to _CUBE_, `oxidicom` needs to run in a Docker container in the same network and mounting
the same volume as _CUBE_'s container. This is facilitated by the `run.sh` command whish is called
by the `justfile`.

### Tools

You need to have installed:

- Docker Compose
- https://github.com/casey/just
- [DCMTK](https://dicom.offis.de/dcmtk.php.en)

### Run tests

```shell
just reset
just test
```

### Observability

Traces and metrics can be collected using OpenTelemetry collector.
Most notably, there is a span for every retrieval of a DICOM series,
and an event for every DICOM instance.

#### Example Span Attributes

```json
{
  "_timestamp": "Mar 14, 2024 07:11:34.715 -04:00",
  "aet": "HOSPITALPACS",
  "client_address": "127.0.0.1",
  "client_port": "51926",
  "duration": "70290310us",
  "end_time": 1710414765005441300,
  "flags": 1,
  "operation_name": "association",
  "service_name": "oxidicom-test",
  "service_telemetry_sdk_language": "rust",
  "service_telemetry_sdk_name": "opentelemetry",
  "service_telemetry_sdk_version": "0.22.1",
  "span_id": "59edb22075d938b2",
  "span_kind": "Server",
  "span_status": "OK",
  "start_time": 1710414694715131100,
  "trace_id": "70fca6f7b4d07cef2c55833c6ad2e965"
}
```

#### Example Event Attributes

```json
{
  "name": "register_to_chris",
  "_timestamp": 1710414695118726100,
  "url": "http://localhost:8000/api/v1/pacsfiles/2700/",
  "SeriesInstanceUID": "1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0",
  "success": "true",
  "fname": "SERVICES/PACS/HOSPITALPACS/1449c1d-anonymized-20090701-003Y/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/1-1.3.12.2.1107.5.2.19.45152.2013030808110258929186035.dcm"
}
```

### Usage of `opentelemetry` v.s. `tracing` in the codebase

`dicom-rs` itself uses the `tracing` crate, though for the spans described above,
I decided to use the `opentelemetry` crate. However, I am also using the `tracing`
crate as well. Log messages created by `tracing` _do not_ get exported to the
OpenTelemetry collector. They are primarily for debugging.
