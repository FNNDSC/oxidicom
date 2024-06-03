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

### Series-Wise Convention

At the FNNDSC, structural MRI is our biggest area of research.

- One DICOM instance is a 2D MRI slice.
- One DICOM series is a 3D MRI scan (or a 4D fMRI).
- One DICOM study is a collection of MRI scans.

Since a DICOM series is "one scan," `oxidicom` keeps track of the series being received.

### DICOM Terminology

The hospital PACS _pushes_ data to us, hence the hospital PACS is a _client._ (We often call it a "PACS Server,"
however during the retrieval of DICOM files, the PACS' role is a client.)

#### Association

The TCP connection made by the hospital PACS to `oxidicom` in which DICOM files are received is called a
**DICOM association.** During an association, we typically receive one series or one study, which consists
of [1, N) DICOM instances.

In reality, the PACS could possibly send us a study, a patient, anything, or nothing. `oxidicom` will
accept whatever it is given without fuss.

## Association ULID Path

We typically assume some properties are upheld by the DICOM protocol:

- StudyInstanceUID is globally unique for all studies
- SeriesInstanceUID is globally unique for all series
- The NumberOfSeriesRelatedInstances for a series will always be the same

In reality, a PACS server will push to us whatever it wants. `oxidicom` does not assume the above are
always true.

`oxidicom` assigns a [ULID](https://github.com/ulid/spec) to each [Association](#association).
It will register key-value pairs to

```
SERVICES/PACS/org.fnndsc.oxidicom/{ABSOLUTE_SERIES_DIR}/{ASSOCIATION_ULID}/{KEY}={VALUE}
```

### Example

Suppose you'd expect a DICOM file to be registered at

```
SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/96-1.3.12.2.1107.5.2.19.45152.2013030808105959806985847.dcm
```

The `ABSOLUTE_SERIES_DIR` is `SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06`.

After trying to retrieve the series once, you will find the following files to be created:

```
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/01HZ7TF03EZD364005NP332RBQ/NumberOfSeriesRelatedInstances=192
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/01HZ7TF03EZD364005NP332RBQ/OxidicomAttemptedPushCount=192
```

Let's say that you attempt to retrieve the series a second time. You will now find:

```
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/01HZ7WN2KMQ36T7E85SVX6G4V4/NumberOfSeriesRelatedInstances=192
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/01HZ7WN2KMQ36T7E85SVX6G4V4/OxidicomAttemptedPushCount=192
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/01HZ7TF03EZD364005NP332RBQ/NumberOfSeriesRelatedInstances=192
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/01HZ7TF03EZD364005NP332RBQ/OxidicomAttemptedPushCount=192
```

What if the hospital PACS _misbehaves_, sending us a different `NumberOfSeriesRelatedInstances` on the third retrieve attempt?
You will find:

```
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/01HZ7WP273YRHSH33TC3BNDJEB/NumberOfSeriesRelatedInstances=43
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/01HZ7WP273YRHSH33TC3BNDJEB/OxidicomAttemptedPushCount=43
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/01HZ7WN2KMQ36T7E85SVX6G4V4/NumberOfSeriesRelatedInstances=192
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/01HZ7WN2KMQ36T7E85SVX6G4V4/OxidicomAttemptedPushCount=192
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/01HZ7TF03EZD364005NP332RBQ/NumberOfSeriesRelatedInstances=192
SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/HOSPITAL_PACS/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/01HZ7TF03EZD364005NP332RBQ/OxidicomAttemptedPushCount=192
```

## Key-Value Pairs

The basename of a file representing a key-value pair will always be `{KEY}={VALUE}`. The file contents will be empty.
Furthermore, the key will also be the value of `ProtocolName` and the value will be the value of `SeriesDescription`.

This naming convention facilitates search. For example, suppose you want to get the `NumberOfSeriesRelatedInstances`
for a series with `SeriesInstanceUID=1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0`. Make a GET request to

```
/api/v1/pacsfiles/search/?pacs_identifier=org.fnndsc.oxidicom&SeriesInstanceUID=1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0&ProtocolName=NumberOfSeriesRelatedInstances
```

Or, leave out the `&ProtocolName=` query to get both `NumberOfSeriesRelatedInstances` and `OxidicomAttemptedPushCount`.

## Example Response Body from CUBE

```json
{
    "count": 4,
    "next": null,
    "previous": null,
    "results": [
        {
            "url": "https://example.org/api/v1/pacsfiles/1747/",
            "id": 1747,
            "creation_date": "2024-03-20T17:22:41.432808-04:00",
            "fname": "SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/OXITESTORTHANC/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/01HZ7WP273YRHSH33TC3BNDJEB/OxidicomAttemptedPushCount=192",
            "fsize": 0,
            "PatientID": "1449c1d",
            "PatientName": "",
            "PatientBirthDate": null,
            "PatientAge": null,
            "PatientSex": "",
            "StudyDate": "2013-03-08",
            "AccessionNumber": "",
            "Modality": "",
            "ProtocolName": "OxidicomAttemptedPushCount",
            "StudyInstanceUID": "1.2.840.113845.11.1000000001785349915.20130308061609.6346698",
            "StudyDescription": "",
            "SeriesInstanceUID": "1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0",
            "SeriesDescription": "192",
            "pacs_identifier": "org.fnndsc.oxidicom",
            "file_resource": "https://example.org/api/v1/pacsfiles/1747/OxidicomAttemptedPushCount=192"
        },
        {
            "url": "https://example.org/api/v1/pacsfiles/1553/",
            "id": 1553,
            "creation_date": "2024-03-20T17:22:17.754581-04:00",
            "fname": "SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/OXITESTORTHANC/1449c1d-anonymized-20090701/MR-Brain_w_o_Contrast-98edede8b2-20130308/5-SAG_MPRAGE_220_FOV-a27cf06/01HZ7WP273YRHSH33TC3BNDJEB/NumberOfSeriesRelatedInstances=192",
            "fsize": 0,
            "PatientID": "1449c1d",
            "PatientName": "",
            "PatientBirthDate": null,
            "PatientAge": null,
            "PatientSex": "",
            "StudyDate": "2013-03-08",
            "AccessionNumber": "",
            "Modality": "",
            "ProtocolName": "NumberOfSeriesRelatedInstances",
            "StudyInstanceUID": "1.2.840.113845.11.1000000001785349915.20130308061609.6346698",
            "StudyDescription": "",
            "SeriesInstanceUID": "1.3.12.2.1107.5.2.19.45152.2013030808061520200285270.0.0.0",
            "SeriesDescription": "192",
            "pacs_identifier": "org.fnndsc.oxidicom",
            "file_resource": "https://example.org/api/v1/pacsfiles/1553/NumberOfSeriesRelatedInstances=192"
        },
        {
            "url": "https://example.org/api/v1/pacsfiles/2126/",
            "id": 2126,
            "creation_date": "2024-03-20T17:23:29.017147-04:00",
            "fname": "SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/OXITESTORTHANC/02-Jane_Doe-19660101/Hanke_Stadler_0024_transrep-AccessionNumber-20130717/401-anat-T1w-661b8fc/772bc789-429e-474d-aa64-044b4002f56e/OxidicomAttemptedPushCount=384",
            "fsize": 0,
            "PatientID": "02",
            "PatientName": "",
            "PatientBirthDate": null,
            "PatientAge": null,
            "PatientSex": "",
            "StudyDate": "2013-07-17",
            "AccessionNumber": "",
            "Modality": "",
            "ProtocolName": "OxidicomAttemptedPushCount",
            "StudyInstanceUID": "1.2.826.0.1.3680043.2.1143.2592092611698916978113112155415165916",
            "StudyDescription": "",
            "SeriesInstanceUID": "1.2.826.0.1.3680043.2.1143.515404396022363061013111326823367652",
            "SeriesDescription": "384",
            "pacs_identifier": "org.fnndsc.oxidicom",
            "file_resource": "https://example.org/api/v1/pacsfiles/2126/OxidicomAttemptedPushCount=384"
        },
        {
            "url": "https://example.org/api/v1/pacsfiles/1742/",
            "id": 1742,
            "creation_date": "2024-03-20T17:22:40.538847-04:00",
            "fname": "SERVICES/PACS/org.fnndsc.oxidicom/SERVICES/PACS/OXITESTORTHANC/02-Jane_Doe-19660101/Hanke_Stadler_0024_transrep-AccessionNumber-20130717/401-anat-T1w-661b8fc/772bc789-429e-474d-aa64-044b4002f56e/NumberOfSeriesRelatedInstances=384",
            "fsize": 0,
            "PatientID": "02",
            "PatientName": "",
            "PatientBirthDate": null,
            "PatientAge": null,
            "PatientSex": "",
            "StudyDate": "2013-07-17",
            "AccessionNumber": "",
            "Modality": "",
            "ProtocolName": "NumberOfSeriesRelatedInstances",
            "StudyInstanceUID": "1.2.826.0.1.3680043.2.1143.2592092611698916978113112155415165916",
            "StudyDescription": "",
            "SeriesInstanceUID": "1.2.826.0.1.3680043.2.1143.515404396022363061013111326823367652",
            "SeriesDescription": "384",
            "pacs_identifier": "org.fnndsc.oxidicom",
            "file_resource": "https://example.org/api/v1/pacsfiles/1742/NumberOfSeriesRelatedInstances=384"
        }
    ]
}
```

## NumberOfSeriesRelatedInstances

For each series of an [association](#association), `oxidicom` will ask the PACS server for the
[NumberOfSeriesRelatedInstances](https://dicom.nema.org/medical/dicom/current/output/chtml/part04/sect_C.3.4.html).
NumberOfSeriesRelatedInstances is one of:

- a Nat, e.g. 192
- literal "unknown"

"unknown" will be registered in any case of error, e.g.

- `oxidicom` was not configured with `OXIDICOM_PACS_ADDRESS` so it does not know how to contact the PACS
- The PACS did not return a value
- The PACS returned an invalid value

## OxidicomAttemptedPushCount

After all files for an [association](#association) were pushed, `oxidicom` will register the number of files it
_attempted_ to push as `OxidicomAttemptedPushCount`.

### OxidicomAttemptedPushCount Errors

- If the value for `OxidicomAttemptedPushCount` is not the same as the value for `NumberOfSeriesRelatedInstances`,
  the PACS server is misbehaved.
- If the value for `OxidicomAttemptedPushCount` is not the same as the `count` reported by CUBE
  `api/v1/pacsfiles/search/?SeriesInstanceUID=x.x.x.xxxxx`, CUBE is misbehaved.

## File Appearance Timing

The file for `NumberOfSeriesRelatedInstances=*` will be relatively slow to appear, because it can only be queried
for after the first DICOM instance of a series is received.

The file `OxidicomAttemptedPushCount=*` is guaranteed to be the last file to be registered. In other words, the
appearance of the file `OxidicomAttemptedPushCount=*` indicates the retrieval is "done" and no more DICOM files
will be received for the series (in this association).

Here's what a timeline **might** look like for a retrieve of 192 DICOM instances:

```
                                               time --->
DICOM Association    [====================]
                      |            |     |
Push to CUBE          |      [=====|=====|=============================]
                      |        |   |     |                        |   |
                      |        |   |     |                        |   OxidicomAttemptedPushCount=192 received by CUBE
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

Data reception and handling are asynchronous. In testing, it is often the case that the DICOM association
pushes data much faster than storage speed. In this situation, the spans look like

```
                                               time --->
DICOM Association    [====================]

Push to CUBE                                         [=====================================]
```

## Suggested Client Implementation

A simple client implementation would just poll for the existence of a `OxidicomAttemptedPushCount=*` to know
when a PACS retrieve operation is complete. Doing so assumes that (a) the PACS server is well-behaved,
(b) everything between PFDCM <--> PACS <--> oxidicom <--> Postgres <--> CUBE is working smoothly. These
assumptions are usually true, however this implementation can cause silent errors.

Ideally, a client who wants to monitor the progress of a PACS pull operation _should_ do:

1. Poll until `NumberOfSeriesRelatedInstances=*` appears, so that you know how many DICOM files to expect.
2. Poll the value of `count` until it is equal to the `NumberOfSeriesRelatedInstances`
3. Poll until `OxidicomAttemptedPushCount=*` appears, to triple-check that everything worked.

The GET requests corresponding to the steps above would be:

1. The `NumberOfSeriesRelatedInstances`, e.g. `api/v1/pacsfiles/search/?min_creation_date=TTTTTTTT&pacs_identifier=org.fnndsc.oxidicom&SeriesInstanceUID=x.x.x.xxxxx&ProtocolName=NumberOfSeriesRelatedInstances`
2. The `count` of real DICOM files, e.g. `api/v1/pacsfiles/search/?pacs_identifier=HOSPITALPACS&SeriesInstanceUID=x.x.x.xxxxx`
3. The `OxidicomAttemptedPushCount`, e.g. `api/v1/pacsfiles/search/?min_creation_date=TTTTTTTT&pacs_identifier=HOSPITALPACS&SeriesInstanceUID=x.x.x.xxxxx&ProtocolName=OxidicomAttemptedPushCount`

Where:

- `HOSPITALPACS` is the PACS AE title the DICOMs are being retrieved from
- `x.x.x.xxxxx` is the SeriesInstanceUID of interest
- `TTTTTTTT` is the timestamp the retrieve operation was initiated by PFDCM

Explanation of query string parameters:

- `pacs_identifier=HOSPITALPACS&SeriesInstanceUID=x.x.x.xxxxx` searches for DICOM files of the series
- `pacs_identifier=org.fnndsc.oxidicom&SeriesInstanceUID=x.x.x.xxxxx` searches for "Oxidicom Custom Metadata" for the series
- `pacs_identifier=org.fnndsc.oxidicom&ProtocolName=NumberOfSeriesRelatedInstances` searches for files representing `NumberOfSeriesRelatedInstances=*`
- `pacs_identifier=org.fnndsc.oxidicom&ProtocolName=OxidicomAttemptedPushCount` searches for files representing `OxidicomAttemptedPushCount=*`
- `min_creation_date=TTTTTTTT` limits search results to only the most recent PACS retrieve attempt (ignoring the "Oxidicom Custom Metadata" of prior attempts)
