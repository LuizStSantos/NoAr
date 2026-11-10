//! Bounded public feeds and persistent last-good records, independent of the frame thread.
use std::{collections::HashMap, net::IpAddr, time::Duration};

use quick_xml::{Reader, events::Event};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    db::{handles, models::GraphicsTemplate},
    player::controller::ChannelManager,
    utils::{config::PlayoutConfig, errors::ServiceError},
};

pub const MAX_DOCUMENT: usize = 1024 * 1024;
pub type DataRows = Vec<HashMap<String, String>>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataField {
    pub source_id: i32,
    pub column: String,
    #[serde(default)]
    pub row: usize,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SourceInput {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub url: String,
    #[serde(default = "default_interval")]
    pub interval: i64,
    #[serde(default)]
    pub records_path: String,
    #[serde(default)]
    pub document: String,
    #[serde(default)]
    pub revision: i64,
}

fn default_interval() -> i64 {
    30
}

pub fn validate_input(input: &mut SourceInput) -> Result<(), ServiceError> {
    input.name = input.name.trim().to_string();
    input.url = input.url.trim().to_string();

    if input.name.is_empty()
        || input.name.chars().count() > 80
        || !["rss", "csv", "json"].contains(&input.kind.as_str())
        || !(5..=3600).contains(&input.interval)
    {
        return Err(ServiceError::BadRequest(
            "A source needs a name, RSS/CSV/JSON type and a 5–3600 second interval".to_string(),
        ));
    }

    if input.document.len() > MAX_DOCUMENT
        || input.records_path.len() > 200
        || input.url.len() > 2048
    {
        return Err(ServiceError::BadRequest(
            "Data documents are limited to 1 MiB".to_string(),
        ));
    }

    if input.url.is_empty() {
        if input.kind != "csv" || input.document.trim().is_empty() {
            return Err(ServiceError::BadRequest(
                "Enter a public HTTP(S) URL or upload a CSV document".to_string(),
            ));
        }
        parse_csv(&input.document).map_err(ServiceError::BadRequest)?;
    } else {
        public_url(&input.url).map_err(ServiceError::BadRequest)?;
        input.document.clear();
    }

    Ok(())
}

fn checked_rows(rows: DataRows) -> Result<DataRows, String> {
    if rows.is_empty()
        || rows.len() > 200
        || rows.iter().any(|row| {
            row.is_empty()
                || row.len() > 64
                || row.iter().any(|(key, value)| {
                    key.is_empty() || key.len() > 100 || value.chars().count() > 1000
                })
        })
    {
        return Err(
            "Use 1–200 records, up to 64 named columns and 1000 characters per value".to_string(),
        );
    }

    Ok(rows)
}

/// RFC 4180 quoting, including escaped quotes/newlines and UTF-8 BOM; semicolon spreadsheets are accepted.
pub fn parse_csv(document: &str) -> Result<DataRows, String> {
    let document = document.trim_start_matches('\u{feff}');
    let start = document
        .trim_start()
        .get(..15)
        .unwrap_or_default()
        .to_ascii_lowercase();

    if start.starts_with("<!doctype html") || start.starts_with("<html") {
        return Err(
            "The address returned a web page, not CSV. For Google Sheets, share the sheet with \
             anyone with the link, or publish it to the web, and paste its link"
                .to_string(),
        );
    }

    let mut quote = false;
    let mut counts = (0, 0);

    for character in document.chars() {
        match character {
            '"' => quote = !quote,
            '\n' | '\r' if !quote => break,
            ',' if !quote => counts.0 += 1,
            ';' if !quote => counts.1 += 1,
            _ => {}
        }
    }

    let delimiter = if counts.1 > counts.0 { ';' } else { ',' };
    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut characters = document.chars().peekable();
    let mut quoted = false;
    let mut closed = false;

    while let Some(character) = characters.next() {
        if quoted {
            if character == '"' {
                if characters.peek() == Some(&'"') {
                    field.push('"');
                    characters.next();
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(character);
            }
        } else if character == '"' && field.is_empty() && !closed {
            quoted = true;
        } else if character == delimiter {
            row.push(std::mem::take(&mut field));
            closed = false;
        } else if character == '\n' || character == '\r' {
            if character == '\r' && characters.peek() == Some(&'\n') {
                characters.next();
            }
            row.push(std::mem::take(&mut field));

            if row.iter().any(|value| !value.is_empty()) {
                records.push(std::mem::take(&mut row));
            }
            row.clear();
            closed = false;
        } else if closed {
            return Err("Unexpected text after a quoted CSV field".to_string());
        } else {
            field.push(character);
        }

        if records.len() > 201 || row.len() > 64 || field.len() > 8000 {
            return Err("CSV document exceeds record limits".to_string());
        }
    }

    if quoted {
        return Err("Unclosed CSV quote".to_string());
    }
    row.push(field);

    if row.iter().any(|value| !value.is_empty()) {
        records.push(row);
    }

    if records.len() < 2 {
        return Err("CSV needs a header and at least one data row".to_string());
    }

    // Spreadsheets often export formatted but empty columns after the data.
    let width = records[0]
        .iter()
        .rposition(|name| !name.trim().is_empty())
        .map_or(0, |last| last + 1);

    if records.iter().all(|record| {
        record
            .iter()
            .skip(width)
            .all(|value| value.trim().is_empty())
    }) {
        for record in &mut records {
            record.truncate(width);
        }
    }

    let headers: Vec<_> = records
        .remove(0)
        .into_iter()
        .map(|name| name.trim().to_string())
        .collect();

    if headers
        .iter()
        .enumerate()
        .any(|(index, name)| name.is_empty() || headers[..index].contains(name))
    {
        return Err("CSV column names must be nonempty and unique".to_string());
    }

    let mut rows = Vec::new();

    for record in records {
        if record.len() != headers.len() {
            return Err("CSV rows must have the same number of columns as the header".to_string());
        }
        rows.push(headers.iter().cloned().zip(record).collect());
    }

    checked_rows(rows)
}

pub fn parse_json(document: &str, records_path: &str) -> Result<DataRows, String> {
    let value: Value =
        serde_json::from_str(document).map_err(|_| "Invalid JSON document".to_string())?;
    let value = if records_path.is_empty() {
        &value
    } else {
        value
            .pointer(records_path)
            .ok_or("JSON record path was not found")?
    };
    let records: Vec<&Value> = match value {
        Value::Array(rows) => rows.iter().collect(),
        Value::Object(_) => vec![value],
        _ => return Err("JSON records must be objects".to_string()),
    };
    let mut rows = Vec::new();

    for record in records {
        let object = record
            .as_object()
            .ok_or("Each JSON record must be an object")?;
        let mut row = HashMap::new();

        for (key, value) in object {
            row.insert(
                key.clone(),
                match value {
                    Value::String(text) => text.clone(),
                    Value::Null => String::new(),
                    _ => value.to_string(),
                },
            );
        }
        rows.push(row);
    }

    checked_rows(rows)
}

pub fn parse_rss(document: &str) -> Result<DataRows, String> {
    let mut reader = Reader::from_str(document);
    reader.config_mut().trim_text(false);
    let mut row = None::<HashMap<String, String>>;
    let mut tag = String::new();
    let mut rows = Vec::new();

    loop {
        match reader
            .read_event()
            .map_err(|_| "Invalid RSS/Atom XML".to_string())?
        {
            Event::DocType(_) => {
                return Err("XML document type declarations are not supported".to_string());
            }
            Event::Start(event) => {
                let name = String::from_utf8_lossy(event.local_name().as_ref()).to_string();

                if name == "item" || name == "entry" {
                    row = Some(HashMap::new());
                }

                if row.is_some() {
                    tag = name;
                }

                if tag == "link"
                    && let Some(ref mut row) = row
                {
                    for attr in event.attributes().flatten() {
                        if attr.key.local_name().as_ref() == b"href" {
                            row.insert(
                                "link".to_string(),
                                attr.decoded_and_normalized_value(
                                    quick_xml::XmlVersion::Implicit1_0,
                                    reader.decoder(),
                                )
                                .map_err(|_| "Invalid XML link")?
                                .to_string(),
                            );
                        }
                    }
                }
            }
            Event::Empty(event) => {
                if event.local_name().as_ref() == b"link"
                    && let Some(ref mut row) = row
                {
                    for attr in event.attributes().flatten() {
                        if attr.key.local_name().as_ref() == b"href" {
                            row.insert(
                                "link".to_string(),
                                attr.decoded_and_normalized_value(
                                    quick_xml::XmlVersion::Implicit1_0,
                                    reader.decoder(),
                                )
                                .map_err(|_| "Invalid XML link")?
                                .to_string(),
                            );
                        }
                    }
                }
            }
            Event::Text(event) => {
                if let Some(ref mut row) = row {
                    let value = event.decode().map_err(|_| "Invalid XML text")?;
                    let value =
                        quick_xml::escape::unescape(&value).map_err(|_| "Invalid XML entity")?;
                    row.entry(tag.clone()).or_default().push_str(&value);
                }
            }
            Event::GeneralRef(event) => {
                if let Some(ref mut row) = row {
                    let reference = event.decode().map_err(|_| "Invalid XML entity")?;
                    let value = quick_xml::escape::unescape(&format!("&{reference};"))
                        .map_err(|_| "Unsupported XML entity")?
                        .into_owned();
                    row.entry(tag.clone()).or_default().push_str(&value);
                }
            }
            Event::CData(event) => {
                if let Some(ref mut row) = row {
                    row.entry(tag.clone())
                        .or_default()
                        .push_str(&event.decode().map_err(|_| "Invalid XML CDATA")?);
                }
            }
            Event::End(event) => {
                if [b"item".as_slice(), b"entry".as_slice()].contains(&event.local_name().as_ref())
                    && let Some(record) = row.take()
                {
                    rows.push(
                        record
                            .into_iter()
                            .map(|(key, value)| {
                                (key, value.trim().chars().take(1000).collect::<String>())
                            })
                            .collect(),
                    );
                }
            }
            Event::Eof => break,
            _ => {}
        }

        if rows.len() > 200 {
            return Err("RSS feed exceeds 200 records".to_string());
        }
    }

    checked_rows(rows)
}

fn public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(ip) => {
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_unspecified()
                && !ip.is_multicast()
                && !ip.is_broadcast()
                && !ip.is_documentation()
                && !(ip.octets()[0] == 198 && (18..=19).contains(&ip.octets()[1]))
                && !(ip.octets()[0] == 192 && ip.octets()[1] == 0 && ip.octets()[2] == 0)
                && ip.octets()[0] != 0
                && ip.octets()[0] < 224
                && !(ip.octets()[0] == 100 && (64..=127).contains(&ip.octets()[1]))
        }
        IpAddr::V6(ip) => ip.to_ipv4().map_or(
            (ip.segments()[0] & 0xe000) == 0x2000
                && !(ip.segments()[0] == 0x2001 && ip.segments()[1] == 0xdb8),
            |ip| public_address(IpAddr::V4(ip)),
        ),
    }
}

/// A Google Sheets link as copied from the browser or the Share dialog
/// (`/spreadsheets/d/<id>/edit#gid=<tab>`) becomes the CSV export of that tab,
/// and a sheet published to the web (`/d/e/<id>/pubhtml`) its published CSV.
/// Export, query (`gviz`) and published CSV links, and other hosts, are kept.
fn sheets_csv_url(value: &str) -> String {
    let Ok(url) = reqwest::Url::parse(value) else {
        return value.to_string();
    };

    if url.host_str() != Some("docs.google.com") {
        return value.to_string();
    }

    let segments: Vec<_> = url.path_segments().map_or_else(Vec::new, Iterator::collect);
    let gid = url
        .query_pairs()
        .find(|(key, _)| key == "gid")
        .map(|(_, value)| value.into_owned())
        .or_else(|| {
            url.fragment()?
                .split('&')
                .find_map(|part| part.strip_prefix("gid=").map(str::to_string))
        })
        .filter(|gid| !gid.is_empty() && gid.chars().all(|c| c.is_ascii_digit()));
    let with_gid = |base: String| match &gid {
        Some(gid) => format!("{base}&gid={gid}"),
        None => base,
    };

    match segments.as_slice() {
        ["spreadsheets", "d", "e", id, page, ..] if ["pubhtml", "pub"].contains(page) => {
            if *page == "pub"
                && url
                    .query_pairs()
                    .any(|(key, value)| key == "output" && value != "html")
            {
                return value.to_string();
            }

            with_gid(format!(
                "https://docs.google.com/spreadsheets/d/e/{id}/pub?output=csv"
            ))
        }
        ["spreadsheets", "d", id, rest @ ..]
            if *id != "e" && !matches!(rest.first(), Some(&"export" | &"gviz")) =>
        {
            with_gid(format!(
                "https://docs.google.com/spreadsheets/d/{id}/export?format=csv"
            ))
        }
        _ => value.to_string(),
    }
}

fn public_url(value: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(value).map_err(|_| "Enter a public HTTP(S) URL")?;

    if !["http", "https"].contains(&url.scheme())
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none()
    {
        return Err(
            "Only public HTTP(S) URLs without embedded credentials are supported".to_string(),
        );
    }

    if let Some(host) = url.host_str() {
        let host = host.trim_matches(['[', ']']);

        if host.eq_ignore_ascii_case("localhost")
            || host.ends_with(".localhost")
            || host.parse::<IpAddr>().is_ok_and(|ip| !public_address(ip))
        {
            return Err("Private and local data source addresses are not allowed".to_string());
        }
    }

    Ok(url)
}

async fn response_text(mut response: reqwest::Response) -> Result<String, String> {
    if !response.status().is_success() {
        return Err(format!(
            "Data source returned HTTP {}",
            response.status().as_u16()
        ));
    }

    if response
        .content_length()
        .is_some_and(|length| length > MAX_DOCUMENT as u64)
    {
        return Err("Data source exceeds 1 MiB".to_string());
    }

    let mut data = Vec::new();

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Could not read the data source")?
    {
        if data.len() + chunk.len() > MAX_DOCUMENT {
            return Err("Data source exceeds 1 MiB".to_string());
        }
        data.extend_from_slice(&chunk);
    }

    String::from_utf8(data).map_err(|_| "Data source must use UTF-8".to_string())
}

fn http_client() -> reqwest::ClientBuilder {
    let _ = rustls::crypto::ring::default_provider().install_default();

    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
}

async fn download(value: &str) -> Result<String, String> {
    let mut url = public_url(value)?;

    for _ in 0..4 {
        let host = url
            .host_str()
            .ok_or("Missing source hostname")?
            .trim_matches(['[', ']']);
        let port = url.port_or_known_default().ok_or("Missing source port")?;
        let addresses: Vec<_> = tokio::time::timeout(
            Duration::from_secs(3),
            tokio::net::lookup_host((host, port)),
        )
        .await
        .map_err(|_| "Source DNS lookup timed out")?
        .map_err(|_| "Source DNS lookup failed")?
        .collect();

        if addresses.is_empty()
            || addresses
                .iter()
                .any(|address| !public_address(address.ip()))
        {
            return Err("Private and local data source addresses are not allowed".to_string());
        }

        let client = http_client()
            .resolve_to_addrs(host, &addresses)
            .build()
            .map_err(|_| "Could not create the data source client")?;
        let response = client
            .get(url.clone())
            .send()
            .await
            .map_err(|_| "Data source request failed or timed out")?;

        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or("Redirect has no destination")?;
            url = public_url(url.join(location).map_err(|_| "Invalid redirect")?.as_str())?;
            continue;
        }
        return response_text(response).await;
    }

    Err("Too many data source redirects".to_string())
}

pub async fn refresh(manager: &ChannelManager, source_id: i32) -> Result<(), ServiceError> {
    let source = handles::begin_graphics_source_refresh(&manager.db_pool, manager.id, source_id)
        .await?
        .ok_or_else(|| ServiceError::NotFound("Data source not found".to_string()))?;
    let result = async {
        let document = if source.url.is_empty() {
            source.document.clone()
        } else {
            let url = if source.kind == "csv" {
                sheets_csv_url(&source.url)
            } else {
                source.url.clone()
            };

            tokio::time::timeout(Duration::from_secs(10), download(&url))
                .await
                .map_err(|_| "Data source request timed out")??
        };

        let kind = source.kind.clone();
        let records_path = source.records_path.clone();

        tokio::task::spawn_blocking(move || match kind.as_str() {
            "csv" => parse_csv(&document),
            "json" => parse_json(&document, &records_path),
            "rss" => parse_rss(&document),
            _ => Err("Unsupported source type".to_string()),
        })
        .await
        .map_err(|_| "Data source parser failed".to_string())?
    }
    .await;
    let _edit = manager.graphics_takes.begin_edit().await;
    let result = match result {
        Ok(rows) => validate_referenced_rows(manager, source_id, rows).await,
        Err(error) => Err(error),
    };

    if let Err(error) = &result
        && source.error.as_ref() != Some(error)
    {
        log::warn!(channel=manager.id; "Data source {} kept its last successful data: {error}", source.id);
    }
    handles::finish_graphics_source_refresh(&manager.db_pool, &source, result).await?;
    // Read under the publishing lock: a slower refresh cannot replace a newer cache snapshot.
    let rows = handles::select_graphics_sources(&manager.db_pool, manager.id).await?;
    manager.config.write().await.graphics_data = rows
        .into_iter()
        .map(|source| (source.id, std::sync::Arc::new(source.rows)))
        .collect();

    Ok(())
}

async fn validate_referenced_rows(
    manager: &ChannelManager,
    source_id: i32,
    rows: DataRows,
) -> Result<DataRows, String> {
    let templates = handles::select_graphics_templates(&manager.db_pool, manager.id)
        .await
        .map_err(|_| "Could not validate source mappings")?;
    let active = manager.graphics_takes.records().clone();
    let storage = manager.config.read().await.channel.storage.clone();

    for template in templates {
        for (field, binding) in bindings(&template)
            .iter()
            .filter(|(_, field)| field.source_id == source_id)
        {
            let mut needed = vec![binding.row];
            needed.extend(
                active
                    .iter()
                    .filter(|take| take.template_id == template.id)
                    .filter_map(|take| take.records.get(&source_id).copied()),
            );
            let image_field = template.layers.iter().any(|layer| {
                layer.kind == super::LayerKind::Image
                    && layer
                        .image
                        .as_deref()
                        .and_then(super::whole_placeholder)
                        .as_deref()
                        == Some(field.as_str())
            });

            if image_field
                && needed.iter().any(|index| {
                    rows.get(*index)
                        .and_then(|row| row.get(&binding.column))
                        .is_some_and(|image| {
                            !crate::file::norm_abs_path(&storage, image)
                                .is_ok_and(|(path, _, _)| path.is_file())
                        })
                })
            {
                return Err("Image data must name a file already in Storage; keeping the last successful data".to_string());
            }

            if needed.iter().any(|index| {
                rows.get(*index)
                    .and_then(|row| row.get(&binding.column))
                    .is_none()
            }) {
                return Err("Source removed a column or record used by a template; keeping the last successful data".to_string());
            }
        }
    }

    Ok(rows)
}

pub async fn run(manager: ChannelManager, cancel: tokio_util::sync::CancellationToken) {
    loop {
        tokio::select! { _=cancel.cancelled()=>break, _=tokio::time::sleep(Duration::from_secs(1))=>{} }
        let Ok(sources) = handles::select_graphics_sources(&manager.db_pool, manager.id).await
        else {
            continue;
        };
        let now = chrono::Utc::now().timestamp_millis();
        let mut pending = tokio::task::JoinSet::new();

        for source in sources.into_iter().filter(|source| {
            !source.url.is_empty()
                && now - source.last_attempt.unwrap_or(0) >= source.interval * 1000
        }) {
            if pending.len() >= 4 {
                tokio::select! { _=cancel.cancelled()=>{pending.abort_all(); return;}, _=pending.join_next()=>{} }
            }

            let manager = manager.clone();
            pending.spawn(async move { refresh(&manager, source.id).await });
        }

        while !pending.is_empty() {
            tokio::select! { _=cancel.cancelled()=>{pending.abort_all(); return;}, _=pending.join_next()=>{} }
        }
    }
}

pub fn bindings(template: &GraphicsTemplate) -> HashMap<String, DataField> {
    template
        .layers
        .iter()
        .flat_map(|layer| layer.style.data_fields.clone())
        .collect()
}

pub fn resolved_values(
    template: &GraphicsTemplate,
    manual: &HashMap<String, String>,
    records: &HashMap<i32, usize>,
    config: &PlayoutConfig,
) -> Result<HashMap<String, String>, ServiceError> {
    let mut values = HashMap::new();

    for (field, binding) in bindings(template) {
        if manual.get(&field).is_some_and(|value| !value.is_empty()) {
            continue;
        }

        let row = records
            .get(&binding.source_id)
            .copied()
            .unwrap_or(binding.row);
        let value = config.graphics_data.get(&binding.source_id).and_then(|rows| rows.get(row)).and_then(|row| row.get(&binding.column)).ok_or_else(|| ServiceError::Conflict(format!("Data for {{{field}}} is unavailable; select a valid source record or enter a manual value")))?;
        values.insert(field, value.clone());
    }
    values.extend(
        manual
            .iter()
            .filter(|(_, value)| !value.is_empty())
            .map(|(key, value)| (key.clone(), value.clone())),
    );

    Ok(values)
}

pub async fn validate_template(
    pool: &sqlx::SqlitePool,
    template: &GraphicsTemplate,
) -> Result<(), ServiceError> {
    let sources = handles::select_graphics_sources(pool, template.channel_id).await?;
    let mut seen = HashMap::new();

    for (field, binding) in template
        .layers
        .iter()
        .flat_map(|layer| layer.style.data_fields.iter())
    {
        if !template.fields.contains(field)
            || binding.column.is_empty()
            || binding.column.len() > 100
            || binding.row >= 200
            || !sources.iter().any(|source| source.id == binding.source_id)
        {
            return Err(ServiceError::BadRequest("Map existing fields to a data source on this channel, a named column and row 0–199".to_string()));
        }

        if let Some(previous) = seen.insert(field, binding)
            && previous != binding
        {
            return Err(ServiceError::BadRequest(format!(
                "Conflicting mappings for {{{field}}}"
            )));
        }
    }

    Ok(())
}

pub fn has_unmapped_fields(template: &GraphicsTemplate) -> bool {
    let bindings = bindings(template);
    template
        .fields
        .iter()
        .any(|field| !bindings.contains_key(field))
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_supports_bom_semicolon_quotes_and_multiline_records() {
        let rows = parse_csv("\u{feff}symbol;price;title\r\nUSD;5.10;\"A; B\"\r\nEUR;6.20;\"Quoted \"\"title\"\"\nnext line\"\r\n").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["title"], "A; B");
        assert_eq!(rows[1]["title"], "Quoted \"title\"\nnext line");

        for invalid in [
            "name,name\na,b",
            "name,price\nx",
            "name\n\"unclosed",
            "name,\na,b",
            "name\n",
        ] {
            assert!(parse_csv(invalid).is_err(), "{invalid}");
        }
        assert!(parse_csv(&format!("name\n{}", "x".repeat(1001))).is_err());
    }

    #[test]
    fn json_selects_records_and_keeps_scalar_columns() {
        let rows = parse_json(
            r#"{"quotes":[{"symbol":"USD","price":5.12,"open":true,"note":null}]}"#,
            "/quotes",
        )
        .unwrap();
        assert_eq!(rows[0]["price"], "5.12");
        assert_eq!(rows[0]["open"], "true");
        assert_eq!(rows[0]["note"], "");
        assert!(parse_json("{}", "/missing").is_err());
        assert!(parse_json("[1]", "").is_err());
        assert!(parse_json("[]", "").is_err());
    }

    #[test]
    fn rss_and_atom_decode_entities_cdata_and_links_without_external_entities() {
        let rss = parse_rss(r#"<rss><channel><item><title>News &amp; markets &#49;</title><description><![CDATA[Data <b>today</b>]]></description><link>https://example.com/1</link></item></channel></rss>"#).unwrap();
        assert_eq!(rss[0]["title"], "News & markets 1");
        assert_eq!(rss[0]["description"], "Data <b>today</b>");
        let atom = parse_rss(r#"<feed xmlns="http://www.w3.org/2005/Atom"><entry><title>Markets</title><link href="https://example.com/2"/><updated>2026-11-06T12:00:00Z</updated></entry></feed>"#).unwrap();
        assert_eq!(atom[0]["link"], "https://example.com/2");
        assert!(parse_rss("<!DOCTYPE rss SYSTEM 'file:///etc/passwd'><rss/>").is_err());
        assert!(parse_rss("<rss/>").is_err());
    }

    #[test]
    fn public_feeds_reject_private_hosts_credentials_and_unsupported_schemes() {
        for url in [
            "file:///etc/passwd",
            "http://localhost/feed",
            "http://127.0.0.1/",
            "http://169.254.169.254/",
            "http://10.0.0.1/",
            "http://[::1]/",
            "http://[::ffff:192.168.1.1]/",
            "https://user:password@example.com/feed",
            "http://100.64.0.1/",
            "ftp://example.com/feed",
        ] {
            assert!(public_url(url).is_err(), "{url}");
        }
        assert!(
            public_url("https://docs.google.com/spreadsheets/d/e/public-id/pub?output=csv").is_ok()
        );
        assert!(!public_address("192.168.0.1".parse().unwrap()));
        assert!(public_address("1.1.1.1".parse().unwrap()));
    }

    #[test]
    fn google_sheets_links_become_their_csv() {
        let id = "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms";
        let export = format!("https://docs.google.com/spreadsheets/d/{id}/export?format=csv");

        for (link, csv) in [
            (
                format!("https://docs.google.com/spreadsheets/d/{id}/edit?usp=sharing"),
                export.clone(),
            ),
            (
                format!("https://docs.google.com/spreadsheets/d/{id}"),
                export.clone(),
            ),
            (
                format!("https://docs.google.com/spreadsheets/d/{id}/edit#gid=123"),
                format!("{export}&gid=123"),
            ),
            (
                format!("https://docs.google.com/spreadsheets/d/{id}/edit?gid=7#gid=7"),
                format!("{export}&gid=7"),
            ),
            (
                "https://docs.google.com/spreadsheets/d/e/2PACX-pub/pubhtml".to_string(),
                "https://docs.google.com/spreadsheets/d/e/2PACX-pub/pub?output=csv".to_string(),
            ),
            (
                "https://docs.google.com/spreadsheets/d/e/2PACX-pub/pubhtml?gid=5&single=true"
                    .to_string(),
                "https://docs.google.com/spreadsheets/d/e/2PACX-pub/pub?output=csv&gid=5"
                    .to_string(),
            ),
        ] {
            assert_eq!(sheets_csv_url(&link), csv, "{link}");
        }

        for kept in [
            format!("{export}&gid=0"),
            format!("https://docs.google.com/spreadsheets/d/{id}/gviz/tq?tqx=out:csv"),
            "https://docs.google.com/spreadsheets/d/e/2PACX-pub/pub?output=csv".to_string(),
            "https://docs.google.com/spreadsheets/d/e/2PACX-pub/pub?gid=0&single=true&output=csv"
                .to_string(),
            "https://example.com/spreadsheets/d/abc/edit".to_string(),
        ] {
            assert_eq!(sheets_csv_url(&kept), kept);
        }
    }

    #[test]
    fn csv_ignores_blank_trailing_columns_and_explains_a_web_page() {
        let rows = parse_csv("\"Name\",\"Role\",\"\",\"\"\n\"Ana\",\"Host\",\"\",\"\"\n").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].len(), 2);
        assert_eq!(rows[0]["Role"], "Host");
        assert!(parse_csv("Name,,Role\nAna,,Host").is_err());
        assert!(parse_csv("Name,\nAna,extra").is_err());
        let error = parse_csv("  <!DOCTYPE html><html><body>Sheet</body></html>").unwrap_err();
        assert!(error.contains("web page"), "{error}");
    }

    #[test]
    fn mapped_values_support_record_selection_and_nonempty_manual_overrides() {
        let template = crate::graphics::prepare_template(std::path::Path::new("/tmp"), 1, 1, serde_json::from_value(serde_json::json!({"name":"Quotes","layers":[{"id":"quote","x":5,"y":75,"width":50,"height":10,"text":"{symbol} {price}","data_fields":{"symbol":{"source_id":4,"column":"symbol","row":0},"price":{"source_id":4,"column":"price","row":0}}}]})).unwrap()).unwrap();
        let mut config = PlayoutConfig::default();
        config.graphics_data.insert(
            4,
            std::sync::Arc::new(parse_csv("symbol,price\nUSD,5.10\nEUR,6.20").unwrap()),
        );
        let automatic =
            resolved_values(&template, &HashMap::new(), &HashMap::new(), &config).unwrap();
        assert_eq!(automatic["price"], "5.10");
        let selected = resolved_values(
            &template,
            &HashMap::from([("price".to_string(), "".to_string())]),
            &HashMap::from([(4, 1)]),
            &config,
        )
        .unwrap();
        assert_eq!(selected["symbol"], "EUR");
        assert_eq!(selected["price"], "6.20");
        let manual = resolved_values(
            &template,
            &HashMap::from([("price".to_string(), "MANUAL".to_string())]),
            &HashMap::new(),
            &config,
        )
        .unwrap();
        assert_eq!(manual["price"], "MANUAL");
        assert!(
            resolved_values(
                &template,
                &HashMap::new(),
                &HashMap::from([(4, 9)]),
                &config
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn stale_refreshes_and_errors_cannot_replace_the_last_successful_records() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        handles::db_migrate(&pool).await.unwrap();
        let input = SourceInput {
            name: "Quotes".into(),
            kind: "csv".into(),
            document: "symbol,price\nUSD,5.10".into(),
            interval: 5,
            ..Default::default()
        };
        let id = handles::save_graphics_source(&pool, 1, None, &input)
            .await
            .unwrap();
        let old = handles::begin_graphics_source_refresh(&pool, 1, id)
            .await
            .unwrap()
            .unwrap();
        let latest = handles::begin_graphics_source_refresh(&pool, 1, id)
            .await
            .unwrap()
            .unwrap();
        handles::finish_graphics_source_refresh(&pool, &latest, parse_csv(&input.document))
            .await
            .unwrap();
        handles::finish_graphics_source_refresh(&pool, &old, Err("Old failure".into()))
            .await
            .unwrap();
        let source = handles::select_graphics_source(&pool, 1, id)
            .await
            .unwrap()
            .unwrap();
        assert!(source.error.is_none());
        assert_eq!(source.rows[0]["price"], "5.10");
        let next = handles::begin_graphics_source_refresh(&pool, 1, id)
            .await
            .unwrap()
            .unwrap();
        handles::finish_graphics_source_refresh(&pool, &next, Err("Provider down".into()))
            .await
            .unwrap();
        let failed = handles::select_graphics_source(&pool, 1, id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(failed.last_success, source.last_success);
        assert_eq!(failed.rows, source.rows);
        assert_eq!(failed.error.as_deref(), Some("Provider down"));
        assert!(
            handles::select_graphics_source(&pool, 2, id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(handles::delete_graphics_source(&pool, 1, id).await.unwrap());
        let replacement = handles::save_graphics_source(&pool, 1, None, &input)
            .await
            .unwrap();
        assert_ne!(
            replacement, id,
            "a late response cannot target a newly created source"
        );
        handles::finish_graphics_source_refresh(&pool, &next, parse_csv("symbol,price\nOLD,0.00"))
            .await
            .unwrap();
        assert!(
            handles::select_graphics_source(&pool, 1, replacement)
                .await
                .unwrap()
                .unwrap()
                .rows
                .is_empty()
        );
    }

    #[tokio::test]
    async fn http_bodies_and_errors_are_bounded_before_parsing() {
        use tokio::io::AsyncWriteExt;

        for (status, body, success) in [
            ("200 OK", "symbol,price\nUSD,5.10".to_string(), true),
            ("503 Unavailable", "down".to_string(), false),
            ("200 OK", "x".repeat(MAX_DOCUMENT + 1), false),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                use tokio::io::AsyncReadExt;
                let mut request = [0u8; 4096];
                let _ = stream.read(&mut request).await;
                let header = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(header.as_bytes()).await;
                let _ = stream.write_all(body.as_bytes()).await;
            });
            let response = http_client()
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap()
                .get(format!("http://{address}/data"))
                .send()
                .await
                .unwrap();
            assert_eq!(response_text(response).await.is_ok(), success);
            server.await.unwrap();
        }
    }
}
