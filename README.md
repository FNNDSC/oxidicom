# oxidicom

`oxidicom` is a high-performance DICOM receiver for the
[_ChRIS_ backend](https://github.com/FNNDSC/ChRIS_ultron_backEnd) (CUBE).
It **partially** replaces [pfdcm](https://github.com/FNNDSC/pfdcm).

More technically, `oxidicom` implements a DICOM C-STORE service class provider (SCP),
a "server," which listens for incoming DICOM files. For every DICOM file received,
it writes it to the storage of _CUBE_ and "registers" the file with _CUBE_.

`oxidicom` does _not_ 

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
