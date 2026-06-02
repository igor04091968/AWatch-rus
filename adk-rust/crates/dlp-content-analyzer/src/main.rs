use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Parser;
use regex::Regex;
use serde::Serialize;
use serde_json::Value;

const DEFAULT_BASE_DIR: &str = "/opt/activitywatch/dlp-content-analysis";

#[derive(Debug, Parser)]
#[command(about = "Analyze text or screenshot with DLP dictionaries/regex packs.")]
struct Cli {
    #[arg(long)]
    text: Option<String>,

    #[arg(long)]
    image: Option<PathBuf>,

    #[arg(long)]
    dictionary_pack: Option<String>,

    #[arg(long)]
    regex_pack: Option<String>,

    #[arg(long, default_value = DEFAULT_BASE_DIR)]
    base_dir: PathBuf,

    #[arg(
        long,
        default_value = "/opt/activitywatch/dlp-content-analysis/.venv/bin/python"
    )]
    legacy_python: PathBuf,

    #[arg(
        long,
        default_value = "/opt/activitywatch/dlp-content-analysis/content_analyzer.py"
    )]
    legacy_analyzer: PathBuf,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct MatchResult {
    name: String,
    description: String,
    value: String,
    start: usize,
    end: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    severity: Option<String>,
}

#[derive(Debug, Serialize)]
struct Analysis {
    text: String,
    dictionary_pack: Option<String>,
    regex_pack: Option<String>,
    dictionary_matches: Vec<MatchResult>,
    regex_matches: Vec<MatchResult>,
    source: String,
}

fn digits_only(value: &str) -> String {
    value.chars().filter(|ch| ch.is_ascii_digit()).collect()
}

fn validate_inn(value: &str) -> bool {
    let digits = digits_only(value);
    let nums = digits
        .bytes()
        .map(|byte| (byte - b'0') as u32)
        .collect::<Vec<_>>();
    match nums.len() {
        10 => {
            let coef = [2, 4, 10, 3, 5, 9, 4, 6, 8];
            let chk = nums
                .iter()
                .take(9)
                .zip(coef)
                .map(|(digit, coef)| digit * coef)
                .sum::<u32>()
                % 11
                % 10;
            chk == nums[9]
        }
        12 => {
            let c11 = [7, 2, 4, 10, 3, 5, 9, 4, 6, 8];
            let c12 = [3, 7, 2, 4, 10, 3, 5, 9, 4, 6, 8];
            let chk11 = nums
                .iter()
                .take(10)
                .zip(c11)
                .map(|(digit, coef)| digit * coef)
                .sum::<u32>()
                % 11
                % 10;
            let chk12 = nums
                .iter()
                .take(11)
                .zip(c12)
                .map(|(digit, coef)| digit * coef)
                .sum::<u32>()
                % 11
                % 10;
            chk11 == nums[10] && chk12 == nums[11]
        }
        _ => false,
    }
}

fn validate_snils(value: &str) -> bool {
    let digits = digits_only(value);
    if digits.len() != 11 {
        return false;
    }
    let nums = digits
        .bytes()
        .map(|byte| (byte - b'0') as u32)
        .collect::<Vec<_>>();
    let checksum = nums[9] * 10 + nums[10];
    let sum = nums
        .iter()
        .take(9)
        .enumerate()
        .map(|(idx, digit)| digit * (9 - idx as u32))
        .sum::<u32>();
    let expected = match sum {
        0..=99 => sum,
        100 | 101 => 0,
        _ => {
            let value = sum % 101;
            if value == 100 { 0 } else { value }
        }
    };
    checksum == expected
}

fn validate_passport(value: &str) -> bool {
    let digits = digits_only(value);
    digits.len() == 10
        && digits != "0000000000"
        && digits.chars().any(|ch| ch != digits.as_bytes()[0] as char)
}

fn validate_checksum(kind: &str, value: &str) -> bool {
    match kind {
        "inn" => validate_inn(value),
        "snils" => validate_snils(value),
        "passport" => validate_passport(value),
        _ => true,
    }
}

fn char_offset(text: &str, byte_offset: usize) -> usize {
    text[..byte_offset].chars().count()
}

fn load_json(path: &Path) -> Result<Value> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

fn resolve_pack(base_dir: &Path, kind: &str, name: Option<&str>) -> Option<PathBuf> {
    let name = name?;
    let path = base_dir.join(kind).join(format!("{name}.json"));
    path.exists().then_some(path)
}

fn match_dictionary(text: &str, path: Option<&Path>) -> Result<Vec<MatchResult>> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let rules = load_json(path)?;
    let Some(rules) = rules.as_object() else {
        return Ok(Vec::new());
    };
    let mut results = Vec::new();
    for (name, rule) in rules {
        let Some(regex) = rule.get("regex").and_then(Value::as_str) else {
            continue;
        };
        let regex =
            Regex::new(regex).with_context(|| format!("compile dictionary regex {name}"))?;
        let checksum = rule
            .get("checksum")
            .and_then(Value::as_str)
            .unwrap_or("none");
        let description = rule
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or(name)
            .to_string();
        for capture in regex.find_iter(text) {
            let value = capture.as_str();
            if validate_checksum(checksum, value) {
                results.push(MatchResult {
                    name: name.to_string(),
                    description: description.clone(),
                    value: value.to_string(),
                    start: char_offset(text, capture.start()),
                    end: char_offset(text, capture.end()),
                    severity: None,
                });
            }
        }
    }
    results.sort_by_key(|result| (result.start, result.end, result.name.clone()));
    Ok(results)
}

fn regex_entries(pack: &Value) -> Vec<(String, String, String, String)> {
    let mut entries = Vec::new();
    if let Some(rules) = pack.get("rules").and_then(Value::as_array) {
        for rule in rules.iter().filter_map(Value::as_object) {
            let Some(regex) = rule.get("regex").and_then(Value::as_str) else {
                continue;
            };
            let id = rule
                .get("id")
                .or_else(|| rule.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("regex-rule");
            let description = rule
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or(id);
            let severity = rule
                .get("severity")
                .and_then(Value::as_str)
                .unwrap_or("medium");
            entries.push((
                id.to_string(),
                description.to_string(),
                regex.to_string(),
                severity.to_string(),
            ));
        }
    } else if let Some(patterns) = pack.get("patterns").and_then(Value::as_object) {
        for (id, rule) in patterns {
            let Some(rule) = rule.as_object() else {
                continue;
            };
            let Some(regex) = rule.get("regex").and_then(Value::as_str) else {
                continue;
            };
            let description = rule
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or(id);
            let severity = rule
                .get("severity")
                .and_then(Value::as_str)
                .unwrap_or("medium");
            entries.push((
                id.to_string(),
                description.to_string(),
                regex.to_string(),
                severity.to_string(),
            ));
        }
    }
    entries
}

fn match_regex_pack(text: &str, path: Option<&Path>) -> Result<Vec<MatchResult>> {
    let Some(path) = path else {
        return Ok(Vec::new());
    };
    let pack = load_json(path)?;
    let mut results = Vec::new();
    for (name, description, regex, severity) in regex_entries(&pack) {
        let regex =
            Regex::new(&regex).with_context(|| format!("compile regex pack rule {name}"))?;
        for capture in regex.find_iter(text) {
            results.push(MatchResult {
                name: name.clone(),
                description: description.clone(),
                value: capture.as_str().to_string(),
                start: char_offset(text, capture.start()),
                end: char_offset(text, capture.end()),
                severity: Some(severity.clone()),
            });
        }
    }
    Ok(results)
}

fn analyze_text(cli: &Cli) -> Result<Analysis> {
    let text = cli.text.clone().unwrap_or_default();
    let dictionary_path = resolve_pack(
        &cli.base_dir,
        "dictionaries",
        cli.dictionary_pack.as_deref(),
    );
    let regex_pack_path = resolve_pack(&cli.base_dir, "regex-packs", cli.regex_pack.as_deref());
    Ok(Analysis {
        dictionary_matches: match_dictionary(&text, dictionary_path.as_deref())?,
        regex_matches: match_regex_pack(&text, regex_pack_path.as_deref())?,
        text,
        dictionary_pack: cli.dictionary_pack.clone(),
        regex_pack: cli.regex_pack.clone(),
        source: "text".to_string(),
    })
}

fn run_legacy_image(cli: &Cli, image: &Path) -> Result<Value> {
    if !cli.legacy_python.exists() || !cli.legacy_analyzer.exists() {
        bail!(
            "image OCR requires legacy analyzer: {} {}",
            cli.legacy_python.display(),
            cli.legacy_analyzer.display()
        );
    }
    let mut command = Command::new(&cli.legacy_python);
    command.arg(&cli.legacy_analyzer).arg("--image").arg(image);
    if let Some(pack) = &cli.dictionary_pack {
        command.arg("--dictionary-pack").arg(pack);
    }
    if let Some(pack) = &cli.regex_pack {
        command.arg("--regex-pack").arg(pack);
    }
    let output = command.output().context("run legacy image analyzer")?;
    if !output.status.success() {
        bail!(
            "legacy image analyzer failed rc={:?}: {}{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    serde_json::from_slice(&output.stdout).context("parse legacy image analyzer JSON")
}

fn run(cli: &Cli) -> Result<Value> {
    if let Some(image) = &cli.image {
        return run_legacy_image(cli, image);
    }
    Ok(serde_json::to_value(analyze_text(cli)?)?)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let value = run(&cli)?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_inn_like_python() {
        assert!(validate_inn("7707083893"));
        assert!(validate_inn("500100732259"));
        assert!(!validate_inn("7707083894"));
    }

    #[test]
    fn validates_snils_like_python() {
        assert!(validate_snils("112-233-445 95"));
        assert!(!validate_snils("112-233-445 96"));
    }

    #[test]
    fn validates_passport_like_python() {
        assert!(validate_passport("1234 567890"));
        assert!(!validate_passport("0000 000000"));
        assert!(!validate_passport("1111 111111"));
    }

    #[test]
    fn regex_pack_patterns_shape_is_supported() {
        let pack = serde_json::json!({"patterns": {"secret": {"regex": "token", "description": "Token", "severity": "high"}}});
        assert_eq!(
            regex_entries(&pack),
            vec![(
                "secret".to_string(),
                "Token".to_string(),
                "token".to_string(),
                "high".to_string()
            )]
        );
    }
}
