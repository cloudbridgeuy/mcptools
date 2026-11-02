use std::collections::BTreeMap;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::diagram::{self, Bounds};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct PlanNode {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct PlanEdge {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WriteArgs {
    pub id: String,
    pub nodes: Vec<PlanNode>,
    pub edges: Vec<PlanEdge>,
    pub share_seconds: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UrlArgs {
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShareArgs {
    pub id: String,
    pub share_seconds: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CloseOutput {
    pub closed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlanMeta {
    pub id: String,
    pub token: String,
    pub rev: u64,
}

pub const SAVE_BODY_LIMIT: usize = 10 * 1024 * 1024;

pub fn revision_header(revision: u64) -> String {
    revision.to_string()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveBody {
    pub rev: u64,
    pub snapshot: Value,
    pub png: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveError {
    BadBody,
    StaleRevision,
    TooLarge,
}

pub fn save_token<'a>(
    bound: std::net::SocketAddr,
    hosts: &[&str],
    method: &str,
    path: &'a str,
) -> Option<&'a str> {
    let token = path.strip_prefix("/d/")?.strip_suffix("/save")?;
    (crate::diagram_serve::host_matches(bound, hosts)
        && method == "POST"
        && !token.is_empty()
        && !token.contains('/'))
    .then_some(token)
}

pub fn check_save_size(size: u64) -> Result<(), SaveError> {
    if size > SAVE_BODY_LIMIT as u64 {
        Err(SaveError::TooLarge)
    } else {
        Ok(())
    }
}

pub fn parse_save(body: &[u8]) -> Result<SaveBody, SaveError> {
    check_save_size(body.len() as u64)?;
    let body: SaveBody = serde_json::from_slice(body).map_err(|_| SaveError::BadBody)?;
    if body.snapshot["tldrawFileFormatVersion"] != 1
        || !body.snapshot["schema"].is_object()
        || !body.snapshot["records"].is_array()
        || body.png.is_empty()
    {
        return Err(SaveError::BadBody);
    }
    Ok(body)
}

pub fn prepare_save(body: &SaveBody, png: &[u8], meta: &PlanMeta) -> Result<PlanMeta, SaveError> {
    if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(SaveError::BadBody);
    }
    if body.rev != meta.rev {
        return Err(SaveError::StaleRevision);
    }
    next_meta(&meta.id, Some(meta), [0; 16]).map_err(|_| SaveError::BadBody)
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct WriteOutput {
    pub url: String,
    pub rev: u64,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct UrlOutput {
    pub url: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DiffOutput {
    pub diff: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct PngOutput {
    pub path: PathBuf,
}

pub fn prepare_diff(baseline: Option<&Value>, current: &Value) -> Result<DiffOutput, String> {
    Ok(DiffOutput {
        diff: crate::plan_diff::render_diff(&crate::plan_diff::diff_snapshots(
            baseline.unwrap_or(current),
            current,
        )?),
    })
}

/// Diff for a browser save. Without a baseline (no `plan_diff` or save since
/// `plan_write`), the stored plan the browser loaded is the reference, so the
/// person's first edits are reported instead of diffing the save against itself.
pub fn prepare_save_diff(
    baseline: Option<&Value>,
    stored: Option<&Value>,
    saved: &Value,
) -> Result<DiffOutput, String> {
    prepare_diff(baseline.or(stored), saved)
}

pub fn latest_png_path(data_dir: &Path, id: &str) -> Result<PathBuf, String> {
    validate_id(id)?;
    if !data_dir.is_absolute() {
        return Err("plan data directory must be absolute".into());
    }
    Ok(data_dir.join("diagrams").join(id).join("latest.png"))
}

pub fn png_output(path: PathBuf, is_file: bool) -> Result<PngOutput, String> {
    if !path.is_absolute() || !is_file {
        return Err("latest plan PNG must be an existing file with an absolute path".into());
    }
    Ok(PngOutput { path })
}

pub struct StoredWrite {
    pub snapshot: Value,
    pub meta: PlanMeta,
    pub output: WriteOutput,
}

pub fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id == "."
        || id == ".."
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
    {
        return Err(
            "plan id must be a single ASCII name using letters, digits, '-', '_' or '.'".into(),
        );
    }
    Ok(())
}

pub fn validate_write(args: &WriteArgs) -> Result<Value, String> {
    validate_id(&args.id)?;
    let plan = serde_json::to_value(args).map_err(|error| error.to_string())?;
    diagram::validate_plan(&plan)?;
    Ok(plan)
}

pub fn validate_meta(id: &str, meta: &PlanMeta) -> Result<(), String> {
    validate_id(id)?;
    if meta.id != id
        || meta.rev == 0
        || meta.token.len() != 32
        || !meta
            .token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("invalid stored metadata for plan: {id}"));
    }
    Ok(())
}

pub fn next_meta(
    id: &str,
    previous: Option<&PlanMeta>,
    token: [u8; 16],
) -> Result<PlanMeta, String> {
    validate_id(id)?;
    match previous {
        Some(meta) => {
            validate_meta(id, meta)?;
            Ok(PlanMeta {
                id: id.into(),
                token: meta.token.clone(),
                rev: meta.rev.checked_add(1).ok_or("plan revision overflow")?,
            })
        }
        None => Ok(PlanMeta {
            id: id.into(),
            token: format!("{:032x}", u128::from_be_bytes(token)),
            rev: 1,
        }),
    }
}

pub fn plan_url(meta: &PlanMeta, host: Option<&str>, port: Option<&str>) -> Result<String, String> {
    validate_meta(&meta.id, meta)?;
    let host = host.unwrap_or("127.0.0.1");
    let host = match host.parse::<IpAddr>() {
        Ok(IpAddr::V6(ip)) => format!("[{ip}]"),
        Ok(IpAddr::V4(ip)) => ip.to_string(),
        Err(_)
            if !host.is_empty()
                && host.split('.').all(|label| {
                    !label.is_empty()
                        && !label.starts_with('-')
                        && !label.ends_with('-')
                        && label
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                }) =>
        {
            host.to_owned()
        }
        _ => return Err("invalid diagram host".into()),
    };
    let port: u16 = port
        .unwrap_or("5199")
        .parse()
        .map_err(|_| "invalid diagram port")?;
    if port == 0 {
        return Err("diagram URL port must be greater than zero".into());
    }
    Ok(format!("http://{host}:{port}/d/{}", meta.token))
}

pub fn previous_positions(snapshot: Option<&Value>) -> Result<BTreeMap<String, Bounds>, String> {
    let Some(snapshot) = snapshot else {
        return Ok(BTreeMap::new());
    };
    let records = snapshot["records"]
        .as_array()
        .ok_or("stored plan has no records")?;
    let mut positions = BTreeMap::new();
    for record in records
        .iter()
        .filter(|record| record["typeName"] == "shape" && record["type"] == "geo")
    {
        let id = record["id"]
            .as_str()
            .and_then(|id| id.strip_prefix("shape:"))
            .ok_or("stored node has invalid id")?;
        let coordinate = |value: &Value| {
            value
                .as_f64()
                .ok_or_else(|| format!("invalid stored bounds for node: {id}"))
        };
        let bounds = Bounds {
            x: coordinate(&record["x"])?,
            y: coordinate(&record["y"])?,
            w: coordinate(&record["props"]["w"])?,
            h: coordinate(&record["props"]["h"])?,
        };
        if ![bounds.x, bounds.y, bounds.w, bounds.h]
            .into_iter()
            .all(f64::is_finite)
            || bounds.w <= 0.0
            || bounds.h <= 0.0
        {
            return Err(format!("invalid stored bounds for node: {id}"));
        }
        if positions.insert(id.to_owned(), bounds).is_some() {
            return Err(format!("duplicate stored node: {id}"));
        }
    }
    Ok(positions)
}

pub fn prepare_write(
    plan: &Value,
    previous: Option<&Value>,
    meta: Option<&PlanMeta>,
    token: [u8; 16],
    host: Option<&str>,
    port: Option<&str>,
) -> Result<StoredWrite, String> {
    diagram::validate_plan(plan)?;
    let id = plan["id"].as_str().ok_or("plan id must be a string")?;
    if previous.is_some() != meta.is_some() {
        return Err(format!("incomplete stored plan: {id}"));
    }
    let meta = next_meta(id, meta, token)?;
    let url = plan_url(&meta, host, port)?;
    let positions = diagram::layout_plan(plan, &previous_positions(previous)?)?;
    Ok(StoredWrite {
        snapshot: diagram::expand_plan(plan, &positions)?,
        output: WriteOutput { url, rev: meta.rev },
        meta,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_header_is_decimal() {
        assert_eq!(revision_header(1), "1");
        assert_eq!(revision_header(u64::MAX), "18446744073709551615");
    }
    use serde_json::json;

    #[test]
    fn save_size_includes_the_boundary() {
        assert_eq!(check_save_size(SAVE_BODY_LIMIT as u64), Ok(()));
        assert_eq!(
            check_save_size(SAVE_BODY_LIMIT as u64 + 1),
            Err(SaveError::TooLarge)
        );
        assert_eq!(check_save_size(u64::MAX), Err(SaveError::TooLarge));
    }

    #[test]
    fn save_routes_require_exact_host_method_and_path() {
        let bound = "127.0.0.1:5199".parse().unwrap();
        let hosts = ["127.0.0.1:5199"];
        assert_eq!(
            save_token(bound, &hosts, "POST", "/d/token/save"),
            Some("token")
        );
        assert_eq!(save_token(bound, &[], "POST", "/d/token/save"), None);
        assert_eq!(save_token(bound, &hosts, "GET", "/d/token/save"), None);
        for path in ["/d//save", "/d/token/extra/save", "/d/token/save/"] {
            assert_eq!(save_token(bound, &hosts, "POST", path), None);
        }
    }

    #[test]
    fn save_body_and_revision_decisions_are_pure() {
        let value = json!({"rev":1,"snapshot":{"tldrawFileFormatVersion":1,"schema":{},"records":[]},"png":"encoded"});
        let body = parse_save(&serde_json::to_vec(&value).unwrap()).unwrap();
        let meta = next_meta("example", None, [1; 16]).unwrap();
        let png = b"\x89PNG\r\n\x1a\n";
        let next = prepare_save(&body, png, &meta).unwrap();
        assert_eq!(next.rev, 2);
        assert_eq!(next.token, meta.token);
        assert_eq!(
            prepare_save(&body, png, &next),
            Err(SaveError::StaleRevision)
        );
        assert_eq!(
            prepare_save(&body, b"not PNG", &meta),
            Err(SaveError::BadBody)
        );
        assert_eq!(
            prepare_save(
                &SaveBody {
                    rev: u64::MAX,
                    ..body
                },
                png,
                &PlanMeta {
                    rev: u64::MAX,
                    ..meta
                }
            ),
            Err(SaveError::BadBody)
        );
        for invalid in [b"{broken".as_slice(), b"{}", b"null"] {
            assert!(matches!(parse_save(invalid), Err(SaveError::BadBody)));
        }
        for field in ["rev", "snapshot", "png"] {
            let mut invalid = value.clone();
            invalid[field] = Value::Null;
            assert!(matches!(
                parse_save(&serde_json::to_vec(&invalid).unwrap()),
                Err(SaveError::BadBody)
            ));
        }
    }

    #[test]
    fn missing_baseline_starts_at_current_and_invalid_snapshots_fail() {
        let current = json!({"records": []});
        assert_eq!(prepare_diff(None, &current).unwrap().diff, "");
        assert_eq!(prepare_diff(Some(&current), &current).unwrap().diff, "");
        assert!(prepare_diff(None, &Value::Null).is_err());
        assert!(prepare_diff(Some(&Value::Null), &current).is_err());
    }

    #[test]
    fn save_diff_without_baseline_compares_against_stored_plan() {
        let stored = json!({"records": []});
        let saved = json!({"records": [{
            "id":"shape:ink1", "typeName":"shape", "type":"draw", "x":0, "y":0, "props":{}
        }]});
        assert_eq!(
            prepare_save_diff(None, Some(&stored), &saved).unwrap().diff,
            "+ ink #1 (1 stroke) bbox [0.0, 0.0, 0.0, 0.0]"
        );
        assert_eq!(
            prepare_save_diff(Some(&saved), Some(&stored), &saved)
                .unwrap()
                .diff,
            ""
        );
        assert_eq!(prepare_save_diff(None, None, &saved).unwrap().diff, "");
    }

    #[test]
    fn png_paths_require_absolute_data_directory_valid_id_and_existing_file() {
        let base = Path::new(if cfg!(windows) { "C:\\" } else { "/" });
        let path = latest_png_path(base, "example").unwrap();
        assert_eq!(path, base.join("diagrams/example/latest.png"));
        assert!(latest_png_path(Path::new("relative"), "example").is_err());
        assert!(latest_png_path(base, "../escape").is_err());
        assert!(png_output(path.clone(), false).is_err());
        assert_eq!(png_output(path.clone(), true).unwrap().path, path);
        assert!(png_output(PathBuf::from("latest.png"), true).is_err());
    }

    #[test]
    fn tokens_are_lowercase_fixed_width_and_revisions_preserve_them() {
        let meta = next_meta("example", None, [0xab; 16]).unwrap();
        assert_eq!(meta.token, "abababababababababababababababab");
        assert_eq!(meta.rev, 1);
        let updated = next_meta("example", Some(&meta), [0; 16]).unwrap();
        assert_eq!(updated.token, meta.token);
        assert_eq!(updated.rev, 2);
        assert_eq!(
            next_meta("zero", None, [0; 16]).unwrap().token,
            "0".repeat(32)
        );
        assert!(next_meta("other", Some(&meta), [0; 16]).is_err());
        assert!(next_meta(
            "example",
            Some(&PlanMeta {
                rev: u64::MAX,
                ..meta
            }),
            [0; 16]
        )
        .is_err());
    }

    #[test]
    fn paths_and_stored_metadata_fail_closed() {
        for id in ["", ".", "..", "../escape", "/tmp", "a/b", "a\\b", "a\n"] {
            assert!(validate_id(id).is_err(), "{id:?}");
        }
        assert!(validate_id("GUZ-1131.plan_1").is_ok());
        let mut meta = next_meta("example", None, [0xab; 16]).unwrap();
        for token in [
            "ABCDEF0123456789ABCDEF0123456789",
            "short",
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
        ] {
            meta.token = token.into();
            assert!(validate_meta("example", &meta).is_err());
        }
        meta.token = "0".repeat(32);
        meta.rev = 0;
        assert!(validate_meta("example", &meta).is_err());
    }

    #[test]
    fn urls_use_explicit_endpoint_or_loopback_defaults() {
        let meta = next_meta("example", None, [0; 16]).unwrap();
        assert_eq!(
            plan_url(&meta, None, None).unwrap(),
            format!("http://127.0.0.1:5199/d/{}", meta.token)
        );
        assert_eq!(
            plan_url(&meta, Some("100.64.1.2"), Some("8080")).unwrap(),
            format!("http://100.64.1.2:8080/d/{}", meta.token)
        );
        assert_eq!(
            plan_url(&meta, Some("::1"), None).unwrap(),
            format!("http://[::1]:5199/d/{}", meta.token)
        );
        assert!(plan_url(&meta, Some("tailnet.example"), None).is_ok());
        for host in ["", "host/path", "user@host", "host:80", "host\n"] {
            assert!(plan_url(&meta, Some(host), None).is_err());
        }
        for port in ["0", "65536", "invalid", ""] {
            assert!(plan_url(&meta, None, Some(port)).is_err());
        }
    }

    #[test]
    fn updates_preserve_positions_and_ignore_arrow_records() {
        let plan = json!({"id":"example","nodes":[{"id":"a","text":"A"},{"id":"b","text":"B"}],"edges":[{"from":"a","to":"b"}]});
        let initial = prepare_write(&plan, None, None, [1; 16], None, None).unwrap();
        let mut snapshot = initial.snapshot;
        let shape = snapshot["records"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|record| record["id"] == "shape:a")
            .unwrap();
        shape["x"] = json!(-400);
        shape["y"] = json!(800);
        let before = previous_positions(Some(&snapshot)).unwrap();
        assert_eq!(before.len(), 2);
        let mut plan = plan;
        plan["nodes"][0]["text"] = json!("Updated");
        plan["nodes"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"c","text":"C"}));
        let updated = prepare_write(
            &plan,
            Some(&snapshot),
            Some(&initial.meta),
            [2; 16],
            None,
            None,
        )
        .unwrap();
        let after = previous_positions(Some(&updated.snapshot)).unwrap();
        assert_eq!(after["a"], before["a"]);
        assert_eq!(after["b"], before["b"]);
        assert_eq!(after.len(), 3);
        assert_eq!(updated.output.url, initial.output.url);
        assert_eq!(updated.output.rev, 2);
        assert!(prepare_write(&plan, None, Some(&initial.meta), [0; 16], None, None).is_err());
        assert!(previous_positions(Some(&json!({}))).is_err());
        snapshot["records"][2]["props"]["w"] = json!(0);
        assert!(previous_positions(Some(&snapshot)).is_err());
    }
}
