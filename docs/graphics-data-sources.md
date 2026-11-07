# Graphics data sources

Open **Graphics → Data sources** to create a public feed or spreadsheet, then
map template fields in Composer's Save panel. A new installation has no data
sources and only one removable Logo example.

## Add information

- **CSV:** upload a UTF-8 `.csv`, paste CSV text or enter a published CSV URL.
  The first row contains unique column names. Commas and semicolons are accepted,
  including quoted separators, escaped quotes, multiline cells and a UTF-8 BOM.
  To use Google Sheets, publish the intended sheet as CSV and paste that CSV URL.
  An ordinary private/edit sharing link is not a published CSV. This delivery
  does not sign into Google accounts or provide private Sheets OAuth.
- **RSS / Atom:** enter the public feed URL. Records expose fields such as
  `title`, `link`, `description`, `summary`, `pubDate` or `updated`, depending on
  the feed. Entity references and CDATA are decoded. XML document type
  declarations are rejected; RSS values are truncated to 1000 characters.
- **HTTP JSON:** enter a public JSON URL. Use a JSON Pointer such as `/data/items`
  to select its array of records; leave it empty for a root array or one object.
  Each record must be an object. Scalar values become text; nested values use
  their JSON representation. Formatting financial values is the provider's
  responsibility; quoted CSV/JSON strings retain their original decimal places.

Sources support up to 200 records, 64 named columns and 1000 characters per value.
CSV/JSON values exceeding the limit are rejected; they are not silently shortened.
Documents are limited to 1 MiB and must contain at least one record. A channel
accepts 16 sources. Public HTTP(S) addresses only: private/local addresses and
embedded username/password are rejected, including redirects. Normal proxy/CA
configuration is retained. Public-address checks require working DNS on the
NoAr server, even when an HTTP proxy is configured. Custom HTTP headers are not supported in this delivery.
Source URLs are administrative metadata and are blanked in regular-user reads;
errors/logs use bounded messages without echoing provider URLs.

Saving a source fetches/parses it immediately. A saved configuration is distinct
from a successful update: check the records and **Last successful update**.
A provider error may leave the configuration saved with no initial rows. An
existing uploaded CSV remains unchanged when its edit form's CSV text is empty;
upload or paste replacement text to change it. Duplicate source names are allowed;
source IDs remain distinct and are not reused after deletion.

## Connect a model

Use named fields in text, for example `USD {price}`, or one whole image field,
for example `{badge}`. In Composer's Save panel, **Data sources and fields**
selects a source, column and record for each field. The interface numbers records
from 1; the API's `row` starts at 0. Repeated occurrences of a named field share
one mapping. Removing the placeholder removes its saved mapping.

Image cells contain paths to files already in the channel's **Storage**, such as
`Graphics/badge.png`. Remote image URLs are not downloaded automatically. Source
refresh retains previous data if a mapped image no longer names an existing file.

On **On air**, a temporary template can select a record from each mapped source.
A nonempty manually entered field overrides its source value for that take.
Clear the input to return the preview to automatic data; taking it again applies
that change on air. Editing preview fields alone does not alter an existing take.
Other mapped values continue updating during the take without restarting its
entry animation, elapsed time or duration. Permanent templates use their saved
mapping/record and require literal or fully mapped content.

## When information changes or fails

Public URL sources poll while playout runs, with a configurable 5–3600 second
interval. **Refresh now** works independently of playout. Uploaded CSV changes
when explicitly replaced; it does not poll a local workstation file. Network
work has at most four concurrent requests, a 10-second total fetch timeout and
bounded redirects. Intervals are scheduling targets, not freshness guarantees.
Rendering updates run separately, nominally every 250 ms outside the frame thread.
A slow/bad source must not restart a take or stop clocks in other compositions.

The last successful records and their timestamp are stored in SQLite. Provider,
parsing or referenced-column/record failures keep those records and show the
error/last-success time in Data sources and On air. A source cannot silently
remove a column/record used by a saved template or an active record selection.
Late responses from an older configuration/request are discarded. Without any
initial usable data, a new take or permanent save fails explicitly; enter manual
values for missing temporary fields or repair/refresh the source first.

On restart, cached successful records load before templates. URL polling resumes
with playout. A stopped playout does not keep polling, and its clock renders the
current server time when started again. Normal database backup includes source
configuration, uploaded CSV, cached records, template mappings and uploaded fonts.

Deleting a source used by a model returns a conflict naming the dependent models.
Unlink its fields or remove those templates first. Source deletion does not delete
artwork or fonts. No default feed, ticker, clock or extra L-shape model is created.

See [API reference](api.md#graphics-data-source-api) and
[executed graphics handoff](graphics-handoff/README.md) for evidence and limits.
