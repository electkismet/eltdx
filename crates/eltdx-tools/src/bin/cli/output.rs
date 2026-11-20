use std::io::{self, Write};

use eltdx::f10::JsonNumber;
use serde::Serialize;

use super::args::{Format, Output};

pub(super) use eltdx::f10::JsonValue as Value;

macro_rules! object {
    ($($key:literal => $value:expr),* $(,)?) => {
        $crate::cli::output::Value::Object(vec![
            $(($key.to_owned(), $crate::cli::output::scalar($value))),*
        ])
    };
}

pub(super) use object;

pub(super) fn scalar(value: impl Serialize) -> Value {
    from_serde(serde_json::to_value(value).unwrap_or_default())
}

fn from_serde(value: serde_json::Value) -> Value {
    match value {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(value) => Value::Bool(value),
        serde_json::Value::Number(value) => JsonNumber::new(value.to_string())
            .map(Value::Number)
            .unwrap_or(Value::Null),
        serde_json::Value::String(value) => Value::String(value),
        serde_json::Value::Array(values) => {
            Value::Array(values.into_iter().map(from_serde).collect())
        }
        serde_json::Value::Object(entries) => Value::Object(
            entries
                .into_iter()
                .map(|(key, value)| (key, from_serde(value)))
                .collect(),
        ),
    }
}

type Row = Vec<(String, Value)>;

fn rows(payload: Value) -> Vec<Row> {
    match payload {
        Value::Object(entries)
            if !entries.is_empty()
                && entries.iter().all(|(_, value)| value.as_object().is_some()) =>
        {
            entries
                .into_iter()
                .map(|(code, value)| {
                    let mut row = vec![("code".to_owned(), code.into())];
                    if let Value::Object(fields) = value {
                        for (name, field) in fields {
                            if name == "code" {
                                row[0].1 = field;
                            } else {
                                row.push((name, field));
                            }
                        }
                    }
                    row
                })
                .collect()
        }
        Value::Array(items) => items.into_iter().map(as_row).collect(),
        other => vec![as_row(other)],
    }
}

fn as_row(value: Value) -> Row {
    match value {
        Value::Object(row) => row,
        other => vec![("value".to_owned(), other)],
    }
}

#[derive(Clone, Copy)]
enum JsonStyle {
    Compact,
    Spaced,
    Indented(usize),
}

fn spaces(output: &mut impl Write, mut count: usize) -> io::Result<()> {
    const SPACES: &[u8; 64] = &[b' '; 64];
    while count > 0 {
        let chunk = count.min(SPACES.len());
        output.write_all(&SPACES[..chunk])?;
        count -= chunk;
    }
    Ok(())
}

fn json(output: &mut impl Write, value: &Value, style: JsonStyle, depth: usize) -> io::Result<()> {
    match value {
        Value::Null => output.write_all(b"null"),
        Value::Bool(true) => output.write_all(b"true"),
        Value::Bool(false) => output.write_all(b"false"),
        Value::Number(number) => output.write_all(number.lexeme().as_bytes()),
        Value::String(text) => serde_json::to_writer(output, text).map_err(io::Error::other),
        Value::Array(values) => {
            json_sequence(output, b"[]", values, style, depth, |output, item| {
                json(output, item, style, depth + 1)
            })
        }
        Value::Object(entries) => json_sequence(
            output,
            b"{}",
            entries,
            style,
            depth,
            |output, (key, item)| {
                serde_json::to_writer(&mut *output, key).map_err(io::Error::other)?;
                output.write_all(if matches!(style, JsonStyle::Compact) {
                    b":"
                } else {
                    b": "
                })?;
                json(output, item, style, depth + 1)
            },
        ),
    }
}

fn json_sequence<W: Write, T>(
    output: &mut W,
    brackets: &[u8; 2],
    items: &[T],
    style: JsonStyle,
    depth: usize,
    mut item: impl FnMut(&mut W, &T) -> io::Result<()>,
) -> io::Result<()> {
    output.write_all(&brackets[..1])?;
    for (index, value) in items.iter().enumerate() {
        if index > 0 {
            output.write_all(if matches!(style, JsonStyle::Spaced) {
                b", "
            } else {
                b","
            })?;
        }
        if let JsonStyle::Indented(width) = style {
            output.write_all(b"\n")?;
            spaces(output, width.saturating_mul(depth + 1))?;
        }
        item(output, value)?;
    }
    if let JsonStyle::Indented(width) = style {
        if !items.is_empty() {
            output.write_all(b"\n")?;
            spaces(output, width.saturating_mul(depth))?;
        }
    }
    output.write_all(&brackets[1..])
}

fn cell(value: &Value, limit: Option<usize>) -> io::Result<String> {
    let text = match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => {
            if value.lexeme().contains(['.', 'e', 'E']) {
                value
                    .as_f64()
                    .map(|number| {
                        format!("{number:.4}")
                            .trim_end_matches('0')
                            .trim_end_matches('.')
                            .to_owned()
                    })
                    .unwrap_or_else(|| value.lexeme().to_owned())
            } else {
                value.lexeme().to_owned()
            }
        }
        nested => {
            let mut bytes = Vec::new();
            json(&mut bytes, nested, JsonStyle::Compact, 0)?;
            String::from_utf8(bytes).map_err(io::Error::other)?
        }
    }
    .replace('\n', " ");
    if let Some(limit) = limit {
        if text.chars().count() > limit {
            return Ok(text.chars().take(limit - 1).chain(['…']).collect());
        }
    }
    Ok(text)
}

fn columns(rows: &[Row]) -> Vec<&str> {
    let mut seen = std::collections::HashSet::new();
    rows.iter()
        .flat_map(|row| row.iter().map(|(key, _)| key.as_str()))
        .filter(|key| seen.insert(*key))
        .collect()
}

fn cells(row: &Row, columns: &[&str], limit: Option<usize>) -> io::Result<Vec<String>> {
    columns
        .iter()
        .map(|name| {
            row.iter()
                .find(|(key, _)| key == name)
                .map_or_else(|| Ok(String::new()), |(_, value)| cell(value, limit))
        })
        .collect()
}

fn table(output: &mut impl Write, rows: &[Row], columns: &[&str]) -> io::Result<()> {
    if rows.is_empty() {
        return writeln!(output, "(无数据)");
    }
    let cells = rows
        .iter()
        .map(|row| cells(row, columns, Some(30)))
        .collect::<io::Result<Vec<_>>>()?;
    let widths: Vec<_> = columns
        .iter()
        .enumerate()
        .map(|(index, name)| {
            cells
                .iter()
                .map(|row| row[index].chars().count())
                .fold(name.chars().count(), usize::max)
        })
        .collect();
    let separator = format!(
        "+{}+",
        widths
            .iter()
            .map(|width| "-".repeat(width + 2))
            .collect::<Vec<_>>()
            .join("+")
    );
    let line = |values: &[&str]| {
        let padded = values
            .iter()
            .zip(&widths)
            .map(|(value, width)| format!("{value:<width$}"))
            .collect::<Vec<_>>();
        format!("| {} |", padded.join(" | "))
    };
    writeln!(output, "{separator}\n{}\n{separator}", line(columns))?;
    for row in &cells {
        writeln!(
            output,
            "{}",
            line(&row.iter().map(String::as_str).collect::<Vec<_>>())
        )?;
    }
    writeln!(output, "{separator}\n({} 行)", rows.len())
}

fn csv_line(output: &mut impl Write, cells: &[&str]) -> io::Result<()> {
    for (index, cell) in cells.iter().enumerate() {
        if index != 0 {
            output.write_all(b",")?;
        }
        if cell.contains([',', '"', '\r', '\n']) || (cells.len() == 1 && cell.is_empty()) {
            write!(output, "\"{}\"", cell.replace('"', "\"\""))?;
        } else {
            output.write_all(cell.as_bytes())?;
        }
    }
    output.write_all(b"\r\n")
}

pub(super) fn emit(output: &mut impl Write, options: &Output, payload: Value) -> io::Result<()> {
    if options.output == Format::Json {
        let style = options.indent.map_or(JsonStyle::Spaced, |indent| {
            JsonStyle::Indented(indent.max(0) as usize)
        });
        json(output, &payload, style, 0)?;
        return output.write_all(b"\n");
    }
    let rows = rows(payload);
    match options.output {
        Format::Json => Ok(()),
        Format::Jsonl => {
            for row in rows {
                json(output, &Value::Object(row), JsonStyle::Spaced, 0)?;
                output.write_all(b"\n")?;
            }
            Ok(())
        }
        Format::Table => table(output, &rows, &columns(&rows)),
        Format::Csv => {
            let columns = columns(&rows);
            csv_line(output, &columns)?;
            for row in &rows {
                let values = cells(row, &columns, None)?;
                csv_line(
                    output,
                    &values.iter().map(String::as_str).collect::<Vec<_>>(),
                )?;
            }
            Ok(())
        }
    }
}
