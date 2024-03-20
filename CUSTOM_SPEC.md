# "Oxidicom Custom Metadata" Spec

`oxidicom` will push empty files to CUBE and register them under the `api/v1/pacsfiles/` API.
These files contain useful information about the PACS retrieval process, such as:

- [`NumberOfSeriesRelatedInstances`](#numberofseriesrelatedinstances)
- Number of DICOM files received by `oxidicom` for each series per [association](#association).
- Any errors (TODO)

These empty files will always live under the path `SERVICES/PACS/org.fnndsc.oxidicom` and be searchable by
`pacs_identifier=org.fnndsc.oxidicom`.

## Background

A "PACS Pull" is initiated when _pfdcm_ asks the (hospital) PACS server to send us DICOMs.
The hospital PACS server will open a TCP connection with `oxidicom` and send it some DICOM objects.
In a typical _ChRIS_ workflow, users pull DICOM **series**, which contain zero or more DICOM **instances**.
Each DICOM instance is represented by one DICOM file.

_CUBE_ keeps track of individual DICOM files in its `api/v1/pacsfiles/` API.

### _ChRIS_ Series-Wise Convention

In _ChRIS_, it is typical for both users and the system to operate series-wise. For instance,
we typically pull DICOM series. `oxidicom` operates series-wise but does not make any incorrect
assumptions about series-wise operation.

### DICOM Terminology

The hospital PACS _pushes_ data to us, hence the hospital PACS is a _client._ (We often call it a "PACS Server,"
however during the retrieval of DICOM files, the PACS' role is a client.)

#### Association

The TCP connection made by the hospital PACS to `oxidicom` in which DICOM files are received is called a
**DICOM association.** During an association, we typically receive one series, which is typically comprised of
one or more DICOM instances. In reality, the PACS could possibly send us a study, a patient, anything, or nothing.

## Association UUID Path

We typically assume some properties are upheld by the DICOM protocol:

- StudyInstanceUID is globally unique for all studies
- SeriesInstanceUID is globally unique for all series
- The NumberOfSeriesRelatedInstances for a series will always be the same

In reality, a PACS server will push to us whatever it wants. `oxidicom` does not assume the above are invariants.

`oxidicom` assigns a [UUID](https://www.rfc-editor.org/rfc/rfc4122#section-4.4) to each [Association](#association).
It will register key-value pairs to

```
SERVICES/PACS/org.fnndsc.oxidicom/{ABSOLUTE_SERIES_DIR}/{ASSOCIATION_UUID}/{KEY}={VALUE}
```

### Example

Suppose you'd expect a DICOM file to be registered at

```
SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/96-1.3.12.2.1107.5.2.19.45152.2013030808105959806985847.dcm
```

The `ABSOLUTE_SERIES_DIR` is `SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06`.

After trying to retrieve the series once, you will find the following files to be created:

```
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/d72b714f-c001-487d-b441-70a4f4f69174/NumberOfSeriesRelatedInstances=192
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/d72b714f-c001-487d-b441-70a4f4f69174/OxidicomCompletePushCount=192
```

Let's say that you attempt to retrieve the series a second time. You will now find:

```
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/c9af1e71-bf26-46fe-a821-6aa377027a8b/NumberOfSeriesRelatedInstances=192
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/c9af1e71-bf26-46fe-a821-6aa377027a8b/OxidicomCompletePushCount=192
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/d72b714f-c001-487d-b441-70a4f4f69174/NumberOfSeriesRelatedInstances=192
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/d72b714f-c001-487d-b441-70a4f4f69174/OxidicomCompletePushCount=192
```

What if the hospital PACS _misbehaves_, sending us a different `NumberOfSeriesRelatedInstances` on the third retrieve attempt?
You will find:

```
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/28718a6f-23bf-4770-b4da-a8b3eae8907d/NumberOfSeriesRelatedInstances=43
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/28718a6f-23bf-4770-b4da-a8b3eae8907d/OxidicomCompletePushCount=43
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/c9af1e71-bf26-46fe-a821-6aa377027a8b/NumberOfSeriesRelatedInstances=192
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/c9af1e71-bf26-46fe-a821-6aa377027a8b/OxidicomCompletePushCount=192
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/d72b714f-c001-487d-b441-70a4f4f69174/NumberOfSeriesRelatedInstances=192
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/d72b714f-c001-487d-b441-70a4f4f69174/OxidicomCompletePushCount=192
```

## Key-Value Pairs

The basename of a file representing a key-value pair will always be `{KEY}={VALUE}`. The file contents will be empty.
Furthermore, the key will also be the value of `ProtocolName` and the value will be the value of `SeriesDescription`.

This naming convention facilitates search. For example, suppose you want to get the `NumberOfSeriesRelatedInstances`
for a series with `SeriesInstanceUID=1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0`. Make a GET request to

```
/api/v1/pacsfiles/search/?pacs_identifier=org.fnndsc.oxidicom&SeriesInstanceUID=1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0&ProtocolName=NumberOfSeriesRelatedInstances
```

Or, leave out the `&ProtocolName=` query to get both `NumberOfSeriesRelatedInstances` and `OxidicomCompletePushCount`.

TODO EXAMPLE RESPONSE.

## NumberOfSeriesRelatedInstances

For each series of an [association](#association), `oxidicom` will ask the PACS server for the
[NumberOfSeriesRelatedInstances](https://dicom.nema.org/medical/dicom/current/output/chtml/part04/sect_C.3.4.html).
NumberOfSeriesRelatedInstances is one of:

- a Nat, e.g. 192
- literal "unknown"

"unknown" will be registered in any case of error, e.g.

- `oxidicom` was not configured with `CHRIS_PACS_ADDRESS` so it does not know how to contact the PACS
- The PACS did not return a value
- The PACS returned an invalid value

## OxidicomCompletePushCount

After all files for an [association](#association) were pushed, `oxidicom` will register the number of files it
_attempted_ to push as `OxidicomCompletePushCount`.

### OxidicomCompletePushCount Errors

- If the value for `OxidicomCompletePushCount` is not the same as the value for `NumberOfSeriesRelatedInstances`,
  the PACS server is misbehaved.
- If the value for `OxidicomCompletePushCount` is not the same as the `count` reported by CUBE
  `api/v1/pacsfiles/search/?SeriesInstanceUID=x.x.x.xxxxx`, CUBE is misbehaved.

## File Appearance Timing

The file for `NumberOfSeriesRelatedInstances=*` will be registered around the same time as the first DICOM file is
registered. When pulling a MR series containing >100 DICOM instances, it's usually safe to assume that the file
for `NumberOfSeriesRelatedInstances` will appear before the retrieval is complete. If your series is expected to
contain one or an otherwise small number of DICOM instances, then it is necessary to poll for the existence of the
file for `NumberOfSeriesRelatedInstances`.

The appearance of a file `OxidicomCompletePushCount=*` means no more DICOM files will appear in CUBE for the
association (I.E. the retrieval is "done"). Note that it does _not_ guarantee that the file for
`NumberOfSeriesRelatedInstances=*` has been received yet.

Here's what a timeline might look like for a retrieve of 192 DICOM instances:

```
                                               time --->
DICOM Association    [====================]
                      |            |     |
Push to CUBE          |      [=====|=====|=============================]
                      |        |   |     |                        |   |
                      |        |   |     |                        |   OxidicomCompletePushCount=192 received by CUBE
                      |        |   |     |                        |
                      |        |   |     |                        Last DICOM received by CUBE from oxidicom
                      |        |   |     |
                      |        |   |     Last DICOM received by oxidicom from PACS
                      |        |   |
                      |        |   NumberOfSeriesRelatedInstances=192 received by CUBE
                      |        |
                      |        First DICOM received by CUBE from oxidicom
                      |
                      First DICOM received by oxidicom from PACS
```

## Suggested Client Implementation

A simple client implementation would just poll for the existence of a `OxidicomCompletePushCount=*` to know
when a PACS retrieve operation is complete. Doing so assumes that (a) the PACS server is well-behaved,
(b) everything between PFDCM <--> PACS <--> oxidicom <--> CUBE is working smoothly. These assumptions are
usually true, however this implementation can cause silent errors.

Ideally, a client who wants to monitor the progress of a PACS pull operation _should_ poll _CUBE_ for:

- The `count` of real DICOM files, e.g. `api/v1/pacsfiles/search/?pacs_identifier=HOSPITALPACS&SeriesInstanceUID=x.x.x.xxxxx`
- The `NumberOfSeriesRelatedInstances`, e.g. `api/v1/pacsfiles/search/?min_creation_date=TTTTTTTT&pacs_identifier=org.fnndsc.oxidicom&SeriesInstanceUID=x.x.x.xxxxx&ProtocolName=NumberOfSeriesRelatedInstances`
- The `OxidicomCompletePushCount`, e.g. `api/v1/pacsfiles/search/?min_creation_date=TTTTTTTT&pacs_identifier=HOSPITALPACS&SeriesInstanceUID=x.x.x.xxxxx&ProtocolName=OxidicomCompletePushCount`

Where:

- `HOSPITALPACS` is the PACS AE title the DICOMs are being retrieved from
- `x.x.x.xxxxx` is the SeriesInstanceUID of interest
- `TTTTTTTT` is the timestamp the retrieve operation was initiated by PFDCM

Explanation of query string parameters:

- `pacs_identifier=HOSPITALPACS&SeriesInstanceUID=x.x.x.xxxxx` searches for DICOM files of the series
- `pacs_identifier=org.fnndsc.oxidicom&SeriesInstanceUID=x.x.x.xxxxx` searches for "Oxidicom Custom Metadata" for the series
- `pacs_identifier=org.fnndsc.oxidicom&ProtocolName=NumberOfSeriesRelatedInstances` searches for files representing `NumberOfSeriesRelatedInstances=*`
- `pacs_identifier=org.fnndsc.oxidicom&ProtocolName=OxidicomCompletePushCount` searches for files representing `OxidicomCompletePushCount=*`
- `min_creation_date=TTTTTTTT` limits search results to only the most recent PACS retrieve attempt (ignoring the "Oxidicom Custom Metadata" of prior attempts)
