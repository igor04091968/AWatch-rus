use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Parser;
use regex::Regex;
use serde::Serialize;
use serde_yaml::Value;

const OUTPUT_JSON: &str = "ioc_blacklist.json";
const OUTPUT_CSV: &str = "ioc_blacklist.csv";
const OUTPUT_SQL: &str = "ioc_blacklist.sql";
const CSV_FIELDS: &[&str] = &[
    "ioc_type",
    "ioc_value",
    "field",
    "rule_id",
    "rule_title",
    "source_file",
];

#[derive(Debug, Parser)]
#[command(about = "Extract IOC-like Sigma values for DLP preload")]
struct Cli {
    #[arg(long, default_value = "rules")]
    rules_root: PathBuf,

    #[arg(long, default_value = "ioc_export")]
    out_dir: PathBuf,

    #[arg(long, default_value = "dlp_blacklist_ioc")]
    table_name: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
struct IocRow {
    ioc_type: String,
    ioc_value: String,
    field: String,
    rule_id: String,
    rule_title: String,
    source_file: String,
}

fn main() {
    let code = match run() {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("{err:#}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    extract_to_outputs(&cli.rules_root, &cli.out_dir, &cli.table_name)
}

fn extract_to_outputs(rules_root: &Path, out_dir: &Path, table_name: &str) -> Result<()> {
    if !rules_root.exists() {
        bail!("rules root not found: {}", rules_root.display());
    }
    if !rules_root.is_dir() {
        bail!("rules root is not a directory: {}", rules_root.display());
    }

    let yaml_files = collect_yaml_files(rules_root)?;
    let mut rows = Vec::new();
    for path in &yaml_files {
        rows.extend(extract_from_yaml(path)?);
    }
    let rows = dedupe_and_sort(rows);

    fs::create_dir_all(out_dir).with_context(|| format!("create {}", out_dir.display()))?;
    write_json(&out_dir.join(OUTPUT_JSON), &rows)?;
    write_csv(&out_dir.join(OUTPUT_CSV), &rows)?;
    write_sql(&out_dir.join(OUTPUT_SQL), &rows, table_name)?;

    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for row in &rows {
        *counts.entry(row.ioc_type.as_str()).or_default() += 1;
    }

    println!("rules_scanned={}", yaml_files.len());
    println!("iocs_extracted={}", rows.len());
    for (kind, count) in counts {
        println!("{kind}={count}");
    }
    println!("json={}", out_dir.join(OUTPUT_JSON).display());
    println!("csv={}", out_dir.join(OUTPUT_CSV).display());
    println!("sql={}", out_dir.join(OUTPUT_SQL).display());
    Ok(())
}

fn collect_yaml_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    collect_yaml_files_inner(root, &mut out)?;
    out.sort();
    Ok(out)
}

fn collect_yaml_files_inner(path: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(path).with_context(|| format!("read dir {}", path.display()))? {
        let entry = entry.with_context(|| format!("read dir entry {}", path.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("read file type {}", path.display()))?;
        if file_type.is_dir() {
            collect_yaml_files_inner(&path, out)?;
        } else if file_type.is_file() && is_yaml_path(&path) {
            out.push(path);
        }
    }
    Ok(())
}

fn is_yaml_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "yml" | "yaml"))
}

fn extract_from_yaml(path: &Path) -> Result<Vec<IocRow>> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let doc: Value = match serde_yaml::from_str(&text) {
        Ok(doc) => doc,
        Err(_) => return Ok(Vec::new()),
    };
    let Some(mapping) = doc.as_mapping() else {
        return Ok(Vec::new());
    };
    let Some(detection) = mapping_get(mapping, "detection") else {
        return Ok(Vec::new());
    };

    let rule_id = mapping_get(mapping, "id")
        .map(scalar_to_string)
        .unwrap_or_default();
    let rule_title = mapping_get(mapping, "title")
        .map(scalar_to_string)
        .unwrap_or_default();
    let mut rows = Vec::new();
    let sha256_re = Regex::new(r"\b[a-fA-F0-9]{64}\b").expect("valid sha256 regex");
    walk(
        detection,
        &rule_id,
        &rule_title,
        &path.display().to_string(),
        &sha256_re,
        &mut rows,
    );
    Ok(rows)
}

fn mapping_get<'a>(mapping: &'a serde_yaml::Mapping, key: &str) -> Option<&'a Value> {
    mapping.get(Value::String(key.to_string()))
}

fn split_key(key: &str) -> (String, Vec<String>) {
    let parts: Vec<_> = key
        .split('|')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        return (String::new(), Vec::new());
    }
    (
        parts[0].to_ascii_lowercase(),
        parts[1..]
            .iter()
            .map(|part| part.to_ascii_lowercase())
            .collect(),
    )
}

fn walk(
    node: &Value,
    rule_id: &str,
    rule_title: &str,
    source_file: &str,
    sha256_re: &Regex,
    out: &mut Vec<IocRow>,
) {
    match node {
        Value::Mapping(mapping) => {
            for (key, value) in mapping {
                let key_string = scalar_to_string(key);
                let (base, ops) = split_key(&key_string);
                if is_base_field(&base) {
                    for raw in flatten_values(value) {
                        let Some(ioc_type) = detect_ioc_type(&base, &ops, &raw, sha256_re) else {
                            continue;
                        };
                        if ioc_type == "sha256" {
                            for hash in parse_sha256(&raw, sha256_re) {
                                out.push(IocRow {
                                    ioc_type: "sha256".to_string(),
                                    ioc_value: hash,
                                    field: key_string.clone(),
                                    rule_id: rule_id.to_string(),
                                    rule_title: rule_title.to_string(),
                                    source_file: source_file.to_string(),
                                });
                            }
                        } else {
                            out.push(IocRow {
                                ioc_type: ioc_type.to_string(),
                                ioc_value: raw,
                                field: key_string.clone(),
                                rule_id: rule_id.to_string(),
                                rule_title: rule_title.to_string(),
                                source_file: source_file.to_string(),
                            });
                        }
                    }
                }
                walk(value, rule_id, rule_title, source_file, sha256_re, out);
            }
        }
        Value::Sequence(items) => {
            for item in items {
                walk(item, rule_id, rule_title, source_file, sha256_re, out);
            }
        }
        _ => {}
    }
}

fn is_base_field(base: &str) -> bool {
    matches!(
        base,
        "image" | "commandline" | "originalfilename" | "hashes"
    )
}

fn flatten_values(value: &Value) -> Vec<String> {
    match value {
        Value::Null => Vec::new(),
        Value::Bool(v) => vec![py_bool_string(*v).to_string()],
        Value::Number(v) => vec![v.to_string()],
        Value::String(v) => {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                Vec::new()
            } else {
                vec![pyyaml_scalar_string(trimmed)]
            }
        }
        Value::Sequence(items) => items.iter().flat_map(flatten_values).collect(),
        Value::Mapping(mapping) => {
            let mut out = Vec::new();
            for (key, value) in mapping {
                let key = scalar_to_string(key);
                for item in flatten_values(value) {
                    out.push(format!("{key}:{item}"));
                }
            }
            out
        }
        Value::Tagged(tagged) => flatten_values(&tagged.value),
    }
}

fn scalar_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(v) => py_bool_string(*v).to_string(),
        Value::Number(v) => v.to_string(),
        Value::String(v) => v.clone(),
        Value::Sequence(_) | Value::Mapping(_) | Value::Tagged(_) => serde_yaml::to_string(value)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}

fn pyyaml_scalar_string(value: &str) -> String {
    match value.to_ascii_lowercase().as_str() {
        "yes" | "true" | "on" => "True".to_string(),
        "no" | "false" | "off" => "False".to_string(),
        _ => value.to_string(),
    }
}

fn py_bool_string(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

fn detect_ioc_type<'a>(
    base: &str,
    ops: &[String],
    raw: &str,
    sha256_re: &Regex,
) -> Option<&'a str> {
    if base == "image" && ops.iter().any(|op| op == "endswith") {
        return Some("process_image_endswith");
    }
    if base == "commandline" && ops.iter().any(|op| op == "contains") {
        return Some("commandline_contains");
    }
    if base == "originalfilename" {
        return Some("original_filename");
    }
    if base == "hashes" && (ops.iter().any(|op| op == "sha256") || sha256_re.is_match(raw)) {
        return Some("sha256");
    }
    None
}

fn parse_sha256(raw: &str, sha256_re: &Regex) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for hit in sha256_re.find_iter(raw) {
        let value = hit.as_str().to_ascii_lowercase();
        if seen.insert(value.clone()) {
            out.push(value);
        }
    }
    out
}

fn dedupe_and_sort(rows: Vec<IocRow>) -> Vec<IocRow> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for row in rows {
        let key = (
            row.ioc_type.clone(),
            row.ioc_value.to_lowercase(),
            row.field.clone(),
        );
        if seen.insert(key) {
            out.push(row);
        }
    }
    out.sort_by(|a, b| {
        (
            a.ioc_type.as_str(),
            a.ioc_value.to_lowercase(),
            a.field.as_str(),
        )
            .cmp(&(
                b.ioc_type.as_str(),
                b.ioc_value.to_lowercase(),
                b.field.as_str(),
            ))
    });
    out
}

fn write_json(path: &Path, rows: &[IocRow]) -> Result<()> {
    fs::write(path, serde_json::to_string_pretty(rows)?)
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn write_csv(path: &Path, rows: &[IocRow]) -> Result<()> {
    let mut out = String::new();
    out.push_str(&CSV_FIELDS.join(","));
    out.push('\n');
    for row in rows {
        out.push_str(&csv_escape(&row.ioc_type));
        out.push(',');
        out.push_str(&csv_escape(&row.ioc_value));
        out.push(',');
        out.push_str(&csv_escape(&row.field));
        out.push(',');
        out.push_str(&csv_escape(&row.rule_id));
        out.push(',');
        out.push_str(&csv_escape(&row.rule_title));
        out.push(',');
        out.push_str(&csv_escape(&row.source_file));
        out.push('\n');
    }
    fs::write(path, out).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn write_sql(path: &Path, rows: &[IocRow], table_name: &str) -> Result<()> {
    let mut out = format!(
        "CREATE TABLE IF NOT EXISTS {table_name} (\n\
         \x20 id INTEGER PRIMARY KEY AUTOINCREMENT,\n\
         \x20 ioc_type TEXT NOT NULL,\n\
         \x20 ioc_value TEXT NOT NULL,\n\
         \x20 field TEXT,\n\
         \x20 rule_id TEXT,\n\
         \x20 rule_title TEXT,\n\
         \x20 source_file TEXT\n\
         );\n\n"
    );
    for row in rows {
        out.push_str(&format!(
            "INSERT INTO {table_name} (ioc_type, ioc_value, field, rule_id, rule_title, source_file) VALUES \
             ('{}','{}','{}','{}','{}','{}');\n",
            sql_escape(&row.ioc_type),
            sql_escape(&row.ioc_value),
            sql_escape(&row.field),
            sql_escape(&row.rule_id),
            sql_escape(&row.rule_title),
            sql_escape(&row.source_file),
        ));
    }
    fs::write(path, out).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn sql_escape(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn extracts_supported_iocs_and_dedupes() {
        let tmp = tempdir().unwrap();
        let rule = tmp.path().join("rule.yml");
        fs::write(
            &rule,
            r#"
id: test-rule
title: Test Rule
detection:
  selection:
    Image|endswith:
      - '\bad.exe'
      - '\bad.exe'
    CommandLine|contains: ['--dump', 'sekret']
    OriginalFileName: evil.exe
    Hashes|SHA256:
      - 'SHA256=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA'
      - 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
  condition: selection
"#,
        )
        .unwrap();

        let rows = dedupe_and_sort(extract_from_yaml(&rule).unwrap());
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0].ioc_type, "commandline_contains");
        assert!(rows.iter().any(|row| row.ioc_type == "sha256"));
        assert_eq!(
            rows.iter()
                .filter(|row| row.ioc_type == "process_image_endswith")
                .count(),
            1
        );
    }

    #[test]
    fn writes_expected_outputs() {
        let tmp = tempdir().unwrap();
        let rules = tmp.path().join("rules");
        let out = tmp.path().join("out");
        fs::create_dir_all(&rules).unwrap();
        fs::write(
            rules.join("rule.yml"),
            r#"
id: write-rule
title: "CSV, SQL Rule"
detection:
  selection:
    CommandLine|contains: "a,b"
  condition: selection
"#,
        )
        .unwrap();

        extract_to_outputs(&rules, &out, "dlp_blacklist_ioc").unwrap();
        let json = fs::read_to_string(out.join(OUTPUT_JSON)).unwrap();
        let csv = fs::read_to_string(out.join(OUTPUT_CSV)).unwrap();
        let sql = fs::read_to_string(out.join(OUTPUT_SQL)).unwrap();
        assert!(json.contains("commandline_contains"));
        assert!(csv.contains("\"a,b\""));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS dlp_blacklist_ioc"));
    }

    #[test]
    fn skips_invalid_yaml() {
        let tmp = tempdir().unwrap();
        let rule = tmp.path().join("bad.yml");
        fs::write(&rule, ": not yaml: :").unwrap();
        assert!(extract_from_yaml(&rule).unwrap().is_empty());
    }
}
