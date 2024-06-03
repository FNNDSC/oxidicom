# How It Works

## Direct to Postgres Database File Registration

Historically, _oxidicom_ would make an HTTP request to _CUBE_ and _CUBE_ would put a row into the database
for each file. In version 2 of _oxidicom_, it registers files to the database directly for performance optimizations:

- We avoid the slow Python code of _CUBE_
- _oxidicom_ can (a) check for duplicates and (b) insert multiple rows, all in one transaction.

### Possible Approaches

I have considered two possible behaviors:

- "simple": _oxidicom_ registers files to the database in batches.
- "smart": _oxidicom_ registers files at the end of an association, and attempts to validate whether the association
  was typical (i.e. number of files per series is equal to `NumberOfSeriesRelatedInstances`)
