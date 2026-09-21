use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use clap::{Args, Parser, Subcommand, ValueEnum};
use rusqlite::{
    backup::Backup, params, Connection, OptionalExtension, Transaction, TransactionBehavior,
};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::{
    env, fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    sync::Semaphore,
    time::timeout,
};
use uuid::Uuid;

const MAX_REVIEW_SCRIPT: usize = 512;
const DEFAULT_LIMIT: u32 = 20;
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_OUTPUT: usize = 4096;
const SCHEMA_VERSION: i64 = 5;
const MAX_INBOX_INPUT_BYTES: u64 = 512 * 1024 * 1024;
const MAX_INBOX_BASENAME_BYTES: usize = 128;
const INBOX_STATUSES: &[&str] = &[
    "pending",
    "processing",
    "filed",
    "duplicate",
    "quarantined",
    "failed",
];
const MAX_CONCURRENT_CHECKS: usize = 4;
const MAX_SEARCH_LIMIT: u32 = 1_000;
const REVIEW_RUNTIME_LIMIT: &str = "RuntimeMaxSec=10s";
const ABANDONED_CHECK_SECONDS: i64 = 30;
const PROCESS_LEASE_SECONDS: i64 = 30;
const MAX_PROCESS_LIMIT: u32 = 1_000;
const MAX_CHUNK_LINES: usize = 25;
const MAX_CHUNK_BYTES: usize = 8 * 1024;
const MAX_PDF_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
const PDF_TIMEOUT: Duration = Duration::from_secs(20);
const SYSTEMD_RUN: &str = "/run/current-system/sw/bin/systemd-run";
const SOURCE_KINDS: &[&str] = &[
    "zotero",
    "confluence",
    "web",
    "local",
    "user",
    "session",
    "other",
];
const CAPABILITIES: &[&str] = &["network", "gh-auth"];
const SCHEMA: &str = r#"
PRAGMA foreign_keys=ON;
CREATE TABLE IF NOT EXISTS schema_version(version INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS sources(id TEXT PRIMARY KEY, kind TEXT NOT NULL CHECK(kind IN ('zotero','confluence','web','local','user','session','other')), external_id TEXT NOT NULL, uri TEXT, title TEXT NOT NULL, authority TEXT, sensitivity TEXT, metadata_json TEXT NOT NULL DEFAULT '{}' CHECK(json_type(metadata_json)='object'), current_revision_id TEXT REFERENCES source_revisions(id), created_at TEXT NOT NULL, updated_at TEXT NOT NULL, last_checked_at TEXT, UNIQUE(kind,external_id));
CREATE TABLE IF NOT EXISTS source_revisions(id TEXT PRIMARY KEY, source_id TEXT NOT NULL REFERENCES sources(id), version TEXT NOT NULL DEFAULT '', content_hash TEXT NOT NULL DEFAULT '', observed_at TEXT NOT NULL, UNIQUE(source_id,version,content_hash));
CREATE TABLE IF NOT EXISTS claims(id TEXT PRIMARY KEY, text TEXT NOT NULL, normalized_text TEXT NOT NULL UNIQUE, confidence TEXT NOT NULL CHECK(confidence IN ('low','medium','high')), temporal_kind TEXT NOT NULL CHECK(temporal_kind IN ('immutable','as_of','current','derived')), status TEXT NOT NULL CHECK(status IN ('provisional','verified','disputed','superseded')), valid_from TEXT, valid_until TEXT, review_kind TEXT CHECK(review_kind IS NULL OR review_kind IN ('condition','script')), review_after TEXT CHECK(review_after IS NULL OR length(review_after)<=512), review_script_hash TEXT, review_approved_hash TEXT, review_approved_identity TEXT, review_capabilities_json TEXT NOT NULL DEFAULT '[]' CHECK(json_type(review_capabilities_json)='array'), trigger_state TEXT NOT NULL DEFAULT 'pending' CHECK(trigger_state IN ('pending','executing','fired','retired')), review_started_at TEXT, last_review_checked_at TEXT, last_review_result TEXT, last_review_detail TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, last_verified_at TEXT);
CREATE TABLE IF NOT EXISTS topics(id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE, slug TEXT NOT NULL UNIQUE, aliases_json TEXT NOT NULL DEFAULT '[]');
CREATE TABLE IF NOT EXISTS claim_topics(claim_id TEXT NOT NULL REFERENCES claims(id) ON DELETE CASCADE, topic_id TEXT NOT NULL REFERENCES topics(id) ON DELETE CASCADE, PRIMARY KEY(claim_id,topic_id));
CREATE TABLE IF NOT EXISTS evidence(id TEXT PRIMARY KEY, source_revision_id TEXT NOT NULL REFERENCES source_revisions(id), locator TEXT NOT NULL, excerpt TEXT NOT NULL, excerpt_hash TEXT NOT NULL, observed_at TEXT NOT NULL, UNIQUE(source_revision_id,locator,excerpt_hash));
CREATE TABLE IF NOT EXISTS claim_evidence(claim_id TEXT NOT NULL REFERENCES claims(id) ON DELETE CASCADE, evidence_id TEXT NOT NULL REFERENCES evidence(id) ON DELETE CASCADE, stance TEXT NOT NULL CHECK(stance IN ('supports','contradicts','context')), PRIMARY KEY(claim_id,evidence_id));
CREATE TABLE IF NOT EXISTS claim_relations(from_claim TEXT NOT NULL REFERENCES claims(id) ON DELETE CASCADE, relation TEXT NOT NULL CHECK(relation IN ('supersedes','contradicts','refines','depends_on')), to_claim TEXT NOT NULL REFERENCES claims(id) ON DELETE CASCADE, PRIMARY KEY(from_claim,relation,to_claim));
CREATE TABLE IF NOT EXISTS review_reasons(claim_id TEXT NOT NULL REFERENCES claims(id) ON DELETE CASCADE, kind TEXT NOT NULL, reason_key TEXT NOT NULL, detail TEXT NOT NULL DEFAULT '', active INTEGER NOT NULL DEFAULT 1, PRIMARY KEY(claim_id,kind,reason_key));
CREATE TABLE IF NOT EXISTS events(id INTEGER PRIMARY KEY AUTOINCREMENT, occurred_at TEXT NOT NULL, object_type TEXT NOT NULL, object_id TEXT NOT NULL, operation TEXT NOT NULL, details_json TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS inbox_jobs(id TEXT PRIMARY KEY, status TEXT NOT NULL CHECK(status IN ('pending','processing','filed','duplicate','quarantined','failed')), sensitivity TEXT NOT NULL CHECK(sensitivity IN ('public','internal','restricted')), sha256 TEXT NOT NULL, original_basename TEXT NOT NULL, published_path TEXT, byte_count INTEGER NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, error_detail TEXT, lease_owner TEXT, lease_until TEXT);
CREATE INDEX IF NOT EXISTS inbox_jobs_sha256_idx ON inbox_jobs(sha256);
 CREATE TABLE IF NOT EXISTS documents(id TEXT PRIMARY KEY, byte_hash TEXT NOT NULL UNIQUE, media_type TEXT NOT NULL CHECK(media_type IN ('application/pdf','text/plain')), safe_extension TEXT NOT NULL, title TEXT NOT NULL, original_basename TEXT NOT NULL, byte_count INTEGER NOT NULL, sensitivity TEXT NOT NULL CHECK(sensitivity IN ('public','internal','restricted')), archive_path TEXT NOT NULL, derived_text_path TEXT NOT NULL, extraction_status TEXT NOT NULL CHECK(extraction_status IN ('filed','quarantined','needs_ocr')), page_count INTEGER, source_id TEXT NOT NULL REFERENCES sources(id), source_revision_id TEXT NOT NULL REFERENCES source_revisions(id), classification_status TEXT NOT NULL DEFAULT 'unclassified' CHECK(classification_status IN ('unclassified','classified')), doc_type TEXT CHECK(doc_type IS NULL OR doc_type IN ('paper','technical-document','other')), authority TEXT CHECK(authority IS NULL OR authority IN ('formal','baseline','delivered','working')), scholarly INTEGER NOT NULL DEFAULT 0 CHECK(scholarly IN (0,1)), zotero_status TEXT NOT NULL DEFAULT 'not_applicable' CHECK(zotero_status IN ('not_applicable','pending','imported','failed','skipped')), zotero_item_key TEXT, zotero_attachment_key TEXT, zotero_detail TEXT, classification_at TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS document_chunks(id TEXT PRIMARY KEY, document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE, ordinal INTEGER NOT NULL, page_number INTEGER, start_line INTEGER NOT NULL, end_line INTEGER NOT NULL, locator TEXT NOT NULL, text TEXT NOT NULL, text_hash TEXT NOT NULL, UNIQUE(document_id,ordinal), UNIQUE(document_id,locator));
 CREATE TABLE IF NOT EXISTS document_topics(document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE, topic_id TEXT NOT NULL REFERENCES topics(id) ON DELETE CASCADE, PRIMARY KEY(document_id,topic_id));
CREATE VIRTUAL TABLE IF NOT EXISTS document_fts USING fts5(chunk_id UNINDEXED, document_id UNINDEXED, content);
CREATE VIRTUAL TABLE IF NOT EXISTS search_fts USING fts5(claim_id UNINDEXED, content);
"#;

#[derive(Parser)]
#[command(name = "librarian-store", version)]
struct Cli {
    #[command(subcommand)]
    command: CommandLine,
}
#[derive(Subcommand)]
enum CommandLine {
    Init,
    Source {
        #[command(subcommand)]
        command: SourceCommand,
    },
    Claim {
        #[command(subcommand)]
        command: ClaimCommand,
    },
    Evidence(EvidenceArgs),
    Search(SearchArgs),
    Context(SearchArgs),
    Audit,
    Export(ExportArgs),
    Backup(BackupArgs),
    Inbox {
        #[command(subcommand)]
        command: InboxCommand,
    },
    Document {
        #[command(subcommand)]
        command: DocumentCommand,
    },
}
#[derive(Subcommand)]
enum InboxCommand {
    Add(InboxAdd),
    Status,
    Process(InboxProcess),
}
#[derive(Args)]
struct InboxProcess {
    #[arg(long)]
    limit: Option<u32>,
    #[arg()]
    job_ids: Vec<String>,
}
#[derive(Subcommand)]
enum DocumentCommand {
    List(DocumentList),
    Show(DocumentShow),
    Search(DocumentSearch),
    Classify(DocumentClassify),
    ZoteroLink(ZoteroLink),
    ZoteroFailed(ZoteroDetail),
    ZoteroSkip(ZoteroDetail),
}
#[derive(Args)]
struct DocumentShow {
    id: String,
    #[arg(long, default_value_t=DEFAULT_LIMIT)]
    limit: u32,
}
#[derive(Args)]
struct DocumentList {
    #[arg(long)]
    status: Option<String>,
    #[arg(long)]
    classification_status: Option<String>,
    #[arg(long)]
    zotero_status: Option<String>,
    #[arg(long, default_value_t=DEFAULT_LIMIT)]
    limit: u32,
}
#[derive(Args)]
struct DocumentClassify {
    id: String,
    #[arg(long)]
    title: String,
    #[arg(long = "doc-type")]
    doc_type: String,
    #[arg(long)]
    authority: String,
    #[arg(long = "topic")]
    topics: Vec<String>,
    #[arg(long)]
    scholarly: bool,
}
#[derive(Args)]
struct ZoteroLink {
    id: String,
    #[arg(long = "item-key")]
    item_key: String,
    #[arg(long = "attachment-key")]
    attachment_key: Option<String>,
}
#[derive(Args)]
struct ZoteroDetail {
    id: String,
    #[arg(long)]
    detail: String,
}
#[derive(Args)]
struct DocumentSearch {
    query: String,
    #[arg(long, default_value_t=DEFAULT_LIMIT)]
    limit: u32,
}
#[derive(Args)]
struct InboxAdd {
    #[arg(long, default_value = "internal")]
    sensitivity: String,
    #[arg(required = true)]
    files: Vec<PathBuf>,
}
#[derive(Subcommand)]
enum SourceCommand {
    Upsert(SourceUpsert),
    List(SourceList),
}
#[derive(Args)]
struct SourceUpsert {
    #[arg(long)]
    kind: String,
    #[arg(long = "external-id")]
    external_id: String,
    #[arg(long)]
    title: String,
    #[arg(long)]
    uri: Option<String>,
    #[arg(long)]
    version: Option<String>,
    #[arg(long = "content-hash")]
    content_hash: Option<String>,
    #[arg(long = "make-current")]
    make_current: bool,
    #[arg(long)]
    authority: Option<String>,
    #[arg(long)]
    sensitivity: Option<String>,
    #[arg(long = "metadata-json", default_value = "{}")]
    metadata_json: String,
}
#[derive(Args)]
struct SourceList {
    #[arg(long)]
    kind: Option<String>,
}
#[derive(Subcommand)]
enum ClaimCommand {
    Add(ClaimAdd),
    Show(IdArg),
    ReviewApproval(ReviewApprovalArgs),
    ApproveReview(ApproveArgs),
    Review(IdArg),
    Relate(RelateArgs),
}
#[derive(Args)]
struct ClaimAdd {
    #[arg(long)]
    text: String,
    #[arg(long = "topic")]
    topics: Vec<String>,
    #[arg(long, default_value = "medium")]
    confidence: String,
    #[arg(long = "temporal-kind", default_value = "current")]
    temporal_kind: String,
    #[arg(long)]
    valid_from: Option<String>,
    #[arg(long)]
    valid_until: Option<String>,
    #[arg(long = "review-kind")]
    review_kind: Option<String>,
    #[arg(long = "review-after")]
    review_after: Option<String>,
}
#[derive(Args)]
struct IdArg {
    id: String,
}
#[derive(Args)]
struct ApproveArgs {
    id: String,
    #[arg(long)]
    hash: String,
    #[arg(long = "capability")]
    capabilities: Vec<String>,
}
#[derive(Args)]
struct ReviewApprovalArgs {
    id: String,
    #[arg(long = "capability")]
    capabilities: Vec<String>,
}
#[derive(Args)]
struct RelateArgs {
    from: String,
    relation: String,
    to: String,
}
#[derive(Args)]
struct EvidenceArgs {
    #[arg(long)]
    claim: String,
    #[arg(long)]
    source: String,
    #[arg(long)]
    locator: String,
    #[arg(long)]
    excerpt: String,
    #[arg(long, default_value = "supports")]
    stance: String,
}
#[derive(Args)]
struct SearchArgs {
    query: String,
    #[arg(long, default_value_t=DEFAULT_LIMIT)]
    limit: u32,
}
#[derive(Clone, ValueEnum)]
enum ExportFormat {
    Jsonl,
    Markdown,
}
#[derive(Args)]
struct ExportArgs {
    #[arg(long)]
    format: ExportFormat,
    #[arg(long)]
    output: PathBuf,
}
#[derive(Args)]
struct BackupArgs {
    #[arg(long)]
    output: PathBuf,
}

fn now() -> String {
    Utc::now().to_rfc3339()
}
fn id() -> String {
    Uuid::new_v4().to_string()
}
fn normalize(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
fn hash(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}
fn canonical_capabilities(values: &[String]) -> Result<Vec<String>> {
    let mut out = values.to_vec();
    out.sort();
    out.dedup();
    if let Some(value) = out
        .iter()
        .find(|value| !CAPABILITIES.contains(&value.as_str()))
    {
        bail!("unknown review capability: {value}")
    }
    if out.iter().any(|value| value == "gh-auth") && !out.iter().any(|value| value == "network") {
        out.push("network".to_string());
        out.sort();
    }
    Ok(out)
}
fn approval_identity(script: &str, capabilities: &[String]) -> String {
    let canonical = serde_json::to_string(capabilities).expect("capabilities are serializable");
    hash(&format!("{script}\n{canonical}"))
}
fn normalize_revision(value: Option<String>) -> String {
    value.unwrap_or_default().trim().to_string()
}
fn numeric_version(value: &str) -> Option<Vec<u64>> {
    if value.is_empty() {
        return None;
    }
    value.split('.').map(|part| part.parse().ok()).collect()
}
fn version_regresses(current: &str, next: &str) -> bool {
    matches!(
        (numeric_version(current), numeric_version(next)),
        (Some(current), Some(next)) if next < current
    )
}
fn parse_timestamp(value: &Option<String>, name: &str) -> Result<()> {
    if let Some(value) = value {
        chrono::DateTime::parse_from_rfc3339(value)
            .with_context(|| format!("{name} must be RFC3339"))?;
    }
    Ok(())
}
fn db_path() -> Result<PathBuf> {
    if let Ok(p) = env::var("LIBRARIAN_DB") {
        return Ok(p.into());
    }
    let base = data_home(
        env::var_os("XDG_DATA_HOME").as_deref(),
        env::var_os("HOME").as_deref(),
    )?;
    Ok(base.join("opencode/librarian/knowledge.db"))
}
fn data_home(
    xdg_data_home: Option<&std::ffi::OsStr>,
    home: Option<&std::ffi::OsStr>,
) -> Result<PathBuf> {
    if let Some(path) = xdg_data_home
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        if path.is_absolute() {
            return Ok(path);
        }
    }
    home.map(|value| PathBuf::from(value).join(".local/share"))
        .filter(|path| path.is_absolute())
        .context("HOME is not set to an absolute path")
}
fn open(path: &Path) -> Result<Connection> {
    let existed = fs::symlink_metadata(path).is_ok();
    let fresh = existed && fs::metadata(path)?.len() == 0;
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?
    }
    let c = Connection::open(path)?;
    let has_schema_version: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_version')",
        [],
        |r| r.get(0),
    )?;
    if !has_schema_version && existed && !fresh {
        bail!(
            "database has no recognized schema version; back up it, delete it, and run init to reinitialize"
        )
    }
    if has_schema_version {
        let mut statement = c.prepare("SELECT version FROM schema_version")?;
        let versions = statement
            .query_map([], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if versions.is_empty() {
            bail!("database has no schema version; back up it, delete it, and run init to reinitialize");
        }
        if versions.len() != 1 || versions[0] != SCHEMA_VERSION {
            bail!(
                "database has unsupported or conflicting schema versions; back up it, delete it, and run init to reinitialize (expected {SCHEMA_VERSION})"
            );
        }
    } else if existed && !fresh {
        bail!(
            "database has no recognized schema version; back up it, delete it, and run init to reinitialize"
        );
    }
    c.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
    c.execute_batch(SCHEMA)?;
    c.execute("INSERT INTO schema_version(version) SELECT ? WHERE NOT EXISTS (SELECT 1 FROM schema_version)", params![SCHEMA_VERSION])?;
    Ok(c)
}
fn store_root(c: &Connection) -> Result<&Path> {
    Path::new(c.path().context("database has no filesystem path")?)
        .parent()
        .context("database path has no parent")
}
fn private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
    Ok(())
}
fn ensure_layout(c: &Connection) -> Result<()> {
    let root = store_root(c)?;
    private_dir(root)?;
    for directory in ["inbox", "archive/objects", "archive/text", "quarantine"] {
        private_dir(&root.join(directory))?;
    }
    Ok(())
}
fn validate_sensitivity(value: &str) -> Result<()> {
    if ["public", "internal", "restricted"].contains(&value) {
        Ok(())
    } else {
        bail!("invalid sensitivity: expected public, internal, or restricted")
    }
}
fn sanitized_basename(path: &Path) -> String {
    let original = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    let mut result = String::with_capacity(original.len().min(MAX_INBOX_BASENAME_BYTES));
    for character in original.chars() {
        let safe = character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-');
        if safe {
            result.push(character);
        } else if !result.ends_with('_') {
            result.push('_');
        }
        if result.len() >= MAX_INBOX_BASENAME_BYTES {
            break;
        }
    }
    let result = result.trim_matches('.').trim_matches('_').to_string();
    if result.is_empty() {
        "unnamed".to_string()
    } else {
        result
    }
}
fn copy_to_inbox(root: &Path, source: &Path) -> Result<(String, String, u64, PathBuf)> {
    let metadata = fs::symlink_metadata(source)
        .with_context(|| format!("cannot inspect input {}", source.display()))?;
    if !metadata.file_type().is_file() {
        bail!(
            "input is not a regular non-symlink file: {}",
            source.display()
        )
    }
    let mut input_options = fs::OpenOptions::new();
    input_options.read(true);
    #[cfg(unix)]
    input_options.custom_flags(libc::O_NOFOLLOW);
    let mut input = input_options
        .open(source)
        .with_context(|| format!("cannot read input {}", source.display()))?;
    let job_id = id();
    let temporary = root.join(format!(".{job_id}.tmp"));
    let result = (|| -> Result<(String, String, u64, PathBuf)> {
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let mut hasher = Sha256::new();
        let mut bytes = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = input.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            bytes = bytes
                .checked_add(read as u64)
                .context("input size overflow")?;
            validate_inbox_size(bytes)?;
            hasher.update(&buffer[..read]);
            output.write_all(&buffer[..read])?;
        }
        if bytes == 0 {
            bail!("input file is empty")
        }
        output.sync_all()?;
        drop(output);
        let filename = format!("job-{job_id}-{}", sanitized_basename(source));
        let published = root.join(&filename);
        fs::rename(&temporary, &published)?;
        Ok((job_id, format!("{:x}", hasher.finalize()), bytes, published))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
fn validate_inbox_size(bytes: u64) -> Result<()> {
    if bytes > MAX_INBOX_INPUT_BYTES {
        bail!("input exceeds maximum size of {MAX_INBOX_INPUT_BYTES} bytes")
    }
    Ok(())
}
fn existing_published_file(
    root: &Path,
    published_path: &str,
    expected_sha256: &str,
) -> Result<String> {
    let relative = Path::new(published_path)
        .strip_prefix("inbox")
        .ok()
        .filter(|path| path.components().count() == 1)
        .context("published path is outside the inbox")?;
    let path = root.join(relative);
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.file_type().is_file() {
        bail!("published path is not a regular file")
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW);
    let mut file = options.open(&path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    if format!("{:x}", hasher.finalize()) != expected_sha256 {
        bail!("published path has unexpected content")
    }
    Ok(format!(
        "inbox/{}",
        relative
            .file_name()
            .context("published path has no filename")?
            .to_string_lossy()
    ))
}
fn inbox_add(c: &mut Connection, a: InboxAdd, root: &Path) -> Result<serde_json::Value> {
    inbox_add_internal(c, a, root, false)
}
fn inbox_add_internal(
    c: &mut Connection,
    a: InboxAdd,
    root: &Path,
    force_db_failure: bool,
) -> Result<serde_json::Value> {
    validate_sensitivity(&a.sensitivity)?;
    ensure_layout(c)?;
    let mut results = Vec::with_capacity(a.files.len());
    for source in a.files {
        let basename = source
            .file_name()
            .map(|x| x.to_string_lossy().into_owned())
            .context("input path has no basename")?;
        let (job_id, sha256, byte_count, published) = copy_to_inbox(root, &source)?;
        let result = (|| -> Result<serde_json::Value> {
            let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let filed: Option<String> = tx
                .query_row(
                    "SELECT id FROM documents WHERE byte_hash=?",
                    params![sha256],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(document_id) = filed {
                fs::remove_file(&published)?;
                return Ok(serde_json::json!({
                    "status":"duplicate",
                    "document_id":document_id,
                    "sha256":sha256,
                    "published_path":null
                }));
            }
            let existing: Option<(String, Option<String>)> = tx.query_row(
                "SELECT id,published_path FROM inbox_jobs WHERE sha256=? AND status <> 'failed' ORDER BY created_at LIMIT 1",
                params![sha256], |row| Ok((row.get(0)?, row.get(1)?)),
            ).optional()?;
            if let Some((existing_id, existing_path)) = existing {
                if let Some(existing_path) = existing_path.as_deref() {
                    if let Ok(canonical_path) =
                        existing_published_file(root, existing_path, &sha256)
                    {
                        fs::remove_file(&published)?;
                        event(
                            &tx,
                            "inbox_job",
                            &existing_id,
                            "inbox_add",
                            serde_json::json!({"status":"duplicate","sha256":sha256,"original_basename":basename}),
                        )?;
                        tx.commit()?;
                        return Ok(serde_json::json!({
                            "id":existing_id,
                            "status":"duplicate",
                            "sha256":sha256,
                            "published_path":canonical_path
                        }));
                    }
                }
                let detail = "existing published file failed validation";
                tx.execute(
                    "UPDATE inbox_jobs SET status='failed',error_detail=?,updated_at=? WHERE id=?",
                    params![detail, now(), existing_id],
                )?;
                event(
                    &tx,
                    "inbox_job",
                    &existing_id,
                    "inbox_validation_failed",
                    serde_json::json!({"sha256":sha256,"detail":detail}),
                )?;
            }
            if force_db_failure {
                bail!("forced inbox database failure")
            }
            let published_path = format!(
                "inbox/{}",
                published
                    .file_name()
                    .context("published filename missing")?
                    .to_string_lossy()
            );
            let timestamp = now();
            tx.execute("INSERT INTO inbox_jobs(id,status,sensitivity,sha256,original_basename,published_path,byte_count,created_at,updated_at,error_detail) VALUES(?,?,?,?,?,?,?,?,?,NULL)", params![job_id,"pending",a.sensitivity,sha256,basename,published_path,byte_count,timestamp,timestamp])?;
            event(
                &tx,
                "inbox_job",
                &job_id,
                "inbox_add",
                serde_json::json!({"status":"pending","sha256":sha256}),
            )?;
            tx.commit()?;
            Ok(
                serde_json::json!({"id":job_id,"status":"pending","sha256":sha256,"path":published_path,"published_path":published_path}),
            )
        })();
        if result.is_err() {
            let _ = fs::remove_file(&published);
        }
        results.push(result?);
    }
    Ok(serde_json::json!({"jobs":results}))
}
fn inbox_status(c: &Connection) -> Result<serde_json::Value> {
    let mut jobs_statement = c.prepare("SELECT id,status,sensitivity,sha256,original_basename,published_path,byte_count,created_at,updated_at,error_detail FROM inbox_jobs ORDER BY created_at,id")?;
    let jobs = jobs_statement.query_map([], |row| Ok(serde_json::json!({
        "id": row.get::<_, String>(0)?, "status": row.get::<_, String>(1)?, "sensitivity": row.get::<_, String>(2)?,
        "sha256": row.get::<_, String>(3)?, "original_basename": row.get::<_, String>(4)?, "published_path": row.get::<_, Option<String>>(5)?,
        "byte_count": row.get::<_, i64>(6)?, "created_at": row.get::<_, String>(7)?, "updated_at": row.get::<_, String>(8)?, "error_detail": row.get::<_, Option<String>>(9)?
    })))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut counts = INBOX_STATUSES
        .iter()
        .map(|status| ((*status).to_string(), serde_json::json!(0)))
        .collect::<serde_json::Map<_, _>>();
    let mut count_statement =
        c.prepare("SELECT status,count(*) FROM inbox_jobs GROUP BY status")?;
    for row in count_statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })? {
        let (status, count) = row?;
        counts.insert(status, serde_json::json!(count));
    }
    Ok(serde_json::json!({"jobs":jobs,"counts":counts}))
}
fn event(
    tx: &Transaction<'_>,
    typ: &str,
    object: &str,
    op: &str,
    detail: serde_json::Value,
) -> Result<()> {
    tx.execute("INSERT INTO events(occurred_at,object_type,object_id,operation,details_json) VALUES(?,?,?,?,?)",params![now(),typ,object,op,detail.to_string()])?;
    Ok(())
}
fn rebuild_fts(tx: &Transaction<'_>) -> Result<()> {
    tx.execute("DELETE FROM search_fts", [])?;
    tx.execute(
        "INSERT INTO search_fts(claim_id,content) SELECT c.id, c.text || ' ' || COALESCE((SELECT group_concat(t.name,' ') FROM claim_topics ct JOIN topics t ON t.id=ct.topic_id WHERE ct.claim_id=c.id),'') || ' ' || COALESCE((SELECT group_concat(s.title,' ') FROM claim_evidence ce JOIN evidence e ON e.id=ce.evidence_id JOIN source_revisions sr ON sr.id=e.source_revision_id JOIN sources s ON s.id=sr.source_id WHERE ce.claim_id=c.id),'') || ' ' || COALESCE((SELECT group_concat(e.excerpt,' ') FROM claim_evidence ce JOIN evidence e ON e.id=ce.evidence_id WHERE ce.claim_id=c.id),'') FROM claims c",
        [],
    )?;
    Ok(())
}

fn source_upsert(c: &mut Connection, a: SourceUpsert) -> Result<serde_json::Value> {
    if !SOURCE_KINDS.contains(&a.kind.as_str()) {
        bail!("invalid source kind")
    }
    if a.external_id.trim().is_empty() || a.title.trim().is_empty() {
        bail!("source external-id and title must not be empty")
    }
    let metadata: serde_json::Value =
        serde_json::from_str(&a.metadata_json).context("metadata-json must be JSON")?;
    if !metadata.is_object() {
        bail!("metadata-json must be an object")
    }
    let version = normalize_revision(a.version);
    let content_hash = normalize_revision(a.content_hash);
    if version.is_empty() && content_hash.is_empty() {
        bail!("version or content-hash is required for a revision")
    }
    let t = now();
    let tx = c.transaction()?;
    let existing: Option<String> = tx
        .query_row(
            "SELECT id FROM sources WHERE kind=? AND external_id=?",
            params![a.kind, a.external_id],
            |r| r.get(0),
        )
        .optional()?;
    let sid = existing.unwrap_or_else(id);
    tx.execute("INSERT INTO sources(id,kind,external_id,uri,title,authority,sensitivity,metadata_json,created_at,updated_at,last_checked_at) VALUES(?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(kind,external_id) DO UPDATE SET uri=excluded.uri,title=excluded.title,authority=excluded.authority,sensitivity=excluded.sensitivity,metadata_json=excluded.metadata_json,updated_at=excluded.updated_at,last_checked_at=excluded.last_checked_at",params![sid,a.kind,a.external_id,a.uri,a.title,a.authority,a.sensitivity,a.metadata_json,t,t,t])?;
    {
        let rid = id();
        let current_revision: Option<(String, String)> = tx
            .query_row(
                "SELECT sr.version,sr.content_hash FROM sources s JOIN source_revisions sr ON sr.id=s.current_revision_id WHERE s.id=?",
                params![sid],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let known_revision: Option<String> = tx
            .query_row(
                "SELECT id FROM source_revisions WHERE source_id=? AND version=? AND content_hash=?",
                params![sid, version, content_hash],
                |row| row.get(0),
            )
            .optional()?;
        if known_revision.is_none() {
            if let Some((current_version, current_hash)) = &current_revision {
                let ordered_newer = matches!(
                    (numeric_version(current_version), numeric_version(&version)),
                    (Some(current), Some(next)) if next > current
                );
                if !ordered_newer && !a.make_current {
                    let reason = if version_regresses(current_version, &version) {
                        "older"
                    } else {
                        "unordered"
                    };
                    bail!(
                        "source revision is {reason} relative to current revision ({current_version}, {current_hash}); pass --make-current only after verifying it is current"
                    )
                }
            }
        }
        let changed=tx.execute("INSERT OR IGNORE INTO source_revisions(id,source_id,version,content_hash,observed_at) VALUES(?,?,?,?,?)",params![rid,sid,version,content_hash,t])?;
        if changed > 0 {
            tx.execute(
                "UPDATE sources SET current_revision_id=? WHERE id=?",
                params![rid, sid],
            )?;
            let mut st=tx.prepare("SELECT DISTINCT ce.claim_id FROM claim_evidence ce JOIN evidence e ON e.id=ce.evidence_id JOIN source_revisions sr ON sr.id=e.source_revision_id WHERE sr.source_id=? AND sr.id <> ?")?;
            let claims = st
                .query_map(params![sid, rid], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for cid in claims {
                tx.execute("INSERT OR REPLACE INTO review_reasons(claim_id,kind,reason_key,detail) VALUES(?,?,?,?)",params![cid,"source_changed",sid,"source has a newer revision"])?;
            }
        }
    }
    event(
        &tx,
        "source",
        &sid,
        "upsert",
        serde_json::json!({"title":a.title}),
    )?;
    rebuild_fts(&tx)?;
    tx.commit()?;
    Ok(serde_json::json!({"id":sid}))
}
fn source_list(c: &Connection, kind: Option<String>) -> Result<serde_json::Value> {
    if kind
        .as_deref()
        .is_some_and(|value| !SOURCE_KINDS.contains(&value))
    {
        bail!("invalid source kind")
    }
    let mut q = "SELECT id,kind,external_id,title,uri,current_revision_id FROM sources".to_string();
    if kind.is_some() {
        q.push_str(" WHERE kind=?")
    }
    q.push_str(" ORDER BY updated_at DESC");
    let mut st = c.prepare(&q)?;
    let rows = if let Some(k) = kind {
        st.query_map(params![k], row_source)?
    } else {
        st.query_map([], row_source)?
    };
    Ok(serde_json::to_value(
        rows.collect::<rusqlite::Result<Vec<_>>>()?,
    )?)
}
fn row_source(r: &rusqlite::Row<'_>) -> rusqlite::Result<serde_json::Value> {
    Ok(
        serde_json::json!({"id":r.get::<_,String>(0)?,"kind":r.get::<_,String>(1)?,"external_id":r.get::<_,String>(2)?,"title":r.get::<_,String>(3)?,"uri":r.get::<_,Option<String>>(4)?,"revision":r.get::<_,Option<String>>(5)?}),
    )
}

fn claim_add(c: &mut Connection, a: ClaimAdd) -> Result<serde_json::Value> {
    if a.text.trim().is_empty() {
        bail!("claim text must not be empty")
    }
    if let Some(kind) = &a.review_kind {
        if kind != "condition" && kind != "script" {
            bail!("review-kind must be condition or script")
        }
    }
    if a.review_after.as_deref().unwrap_or("").chars().count() > MAX_REVIEW_SCRIPT {
        bail!("review-after must be at most 512 characters")
    }
    let review_after = a.review_after.filter(|value| !value.is_empty());
    if review_after.is_some() != a.review_kind.is_some() {
        bail!("review-kind and review-after must be supplied together")
    }
    if a.confidence != "low" && a.confidence != "medium" && a.confidence != "high" {
        bail!("invalid confidence")
    }
    if a.temporal_kind != "immutable"
        && a.temporal_kind != "as_of"
        && a.temporal_kind != "current"
        && a.temporal_kind != "derived"
    {
        bail!("invalid temporal-kind")
    }
    parse_timestamp(&a.valid_from, "valid-from")?;
    parse_timestamp(&a.valid_until, "valid-until")?;
    if let (Some(from), Some(until)) = (&a.valid_from, &a.valid_until) {
        if chrono::DateTime::parse_from_rfc3339(from)?
            > chrono::DateTime::parse_from_rfc3339(until)?
        {
            bail!("valid-until precedes valid-from")
        }
    }
    let tx = c.transaction()?;
    let cid = id();
    let t = now();
    tx.execute("INSERT INTO claims(id,text,normalized_text,confidence,temporal_kind,status,valid_from,valid_until,review_kind,review_after,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",params![cid,a.text,normalize(&a.text),a.confidence,a.temporal_kind,"provisional",a.valid_from,a.valid_until,a.review_kind,review_after,t,t]).context("claim text already exists")?;
    for topic in a.topics {
        if topic.trim().is_empty() {
            bail!("topics must not be empty")
        }
        let n = normalize(&topic);
        let tid = id();
        tx.execute(
            "INSERT OR IGNORE INTO topics(id,name,slug) VALUES(?,?,?)",
            params![tid, topic, n],
        )?;
        let tid: String = tx.query_row("SELECT id FROM topics WHERE slug=?", params![n], |r| {
            r.get(0)
        })?;
        tx.execute(
            "INSERT OR IGNORE INTO claim_topics VALUES(?,?)",
            params![cid, tid],
        )?;
    }
    event(&tx, "claim", &cid, "add", serde_json::json!({}))?;
    rebuild_fts(&tx)?;
    tx.commit()?;
    Ok(serde_json::json!({"id":cid}))
}

async fn main_async() -> Result<serde_json::Value> {
    let cli = Cli::parse();
    execute(cli).await
}
async fn execute(cli: Cli) -> Result<serde_json::Value> {
    execute_at(cli, db_path()?).await
}
async fn execute_at(cli: Cli, path: PathBuf) -> Result<serde_json::Value> {
    if matches!(cli.command, CommandLine::Init) {
        let c = open(&path)?;
        ensure_layout(&c)?;
        return Ok(serde_json::json!({"path":path}));
    }
    let mut c = open(&path)?;
    match cli.command {
        CommandLine::Init => unreachable!(),
        CommandLine::Source { command: x } => match x {
            SourceCommand::Upsert(a) => source_upsert(&mut c, a),
            SourceCommand::List(a) => source_list(&c, a.kind),
        },
        CommandLine::Claim { command: x } => claim_cmd(&mut c, x).await,
        CommandLine::Evidence(a) => evidence_add(&mut c, a),
        CommandLine::Search(a) => present(&mut c, &a.query, a.limit, "search").await,
        CommandLine::Context(a) => present(&mut c, &a.query, a.limit, "context").await,
        CommandLine::Audit => audit(&c),
        CommandLine::Export(a) => export(&mut c, a).await,
        CommandLine::Backup(a) => backup(&c, a.output),
        CommandLine::Inbox {
            command: InboxCommand::Add(a),
        } => {
            let root = store_root(&c)?.join("inbox");
            inbox_add(&mut c, a, &root)
        }
        CommandLine::Inbox {
            command: InboxCommand::Status,
        } => inbox_status(&c),
        CommandLine::Inbox {
            command: InboxCommand::Process(a),
        } => {
            let root = store_root(&c)?.to_path_buf();
            filing::process(&mut c, a, &root).await
        }
        CommandLine::Document { command } => match command {
            DocumentCommand::List(a) => filing::document_list(
                &c,
                a.status,
                a.classification_status,
                a.zotero_status,
                a.limit,
            ),
            DocumentCommand::Show(a) => filing::document_show_with_limit(&c, &a.id, a.limit),
            DocumentCommand::Search(a) => filing::document_search(&c, &a.query, a.limit),
            DocumentCommand::Classify(a) => filing::document_classify(&mut c, a),
            DocumentCommand::ZoteroLink(a) => filing::document_zotero_link(&mut c, a),
            DocumentCommand::ZoteroFailed(a) => {
                filing::document_zotero_outcome(&mut c, a, "failed")
            }
            DocumentCommand::ZoteroSkip(a) => filing::document_zotero_outcome(&mut c, a, "skipped"),
        },
    }
}

async fn claim_cmd(c: &mut Connection, x: ClaimCommand) -> Result<serde_json::Value> {
    match x {
        ClaimCommand::Add(a) => claim_add(c, a),
        ClaimCommand::Show(a) => present_id(c, &a.id).await,
        ClaimCommand::ReviewApproval(a) => review_approval(c, a),
        ClaimCommand::ApproveReview(a) => approve(c, a),
        ClaimCommand::Review(a) => review(c, a),
        ClaimCommand::Relate(a) => relate(c, a),
    }
}
fn claim_row(c: &Connection, cid: &str) -> Result<Option<serde_json::Value>> {
    let v=c.query_row("SELECT id,text,confidence,temporal_kind,status,valid_from,valid_until,review_kind,review_after,trigger_state,last_review_checked_at,last_review_result,last_review_detail FROM claims WHERE id=?",params![cid],|r|Ok(serde_json::json!({"id":r.get::<_,String>(0)?,"text":r.get::<_,String>(1)?,"confidence":r.get::<_,String>(2)?,"temporal_kind":r.get::<_,String>(3)?,"status":r.get::<_,String>(4)?,"valid_from":r.get::<_,Option<String>>(5)?,"valid_until":r.get::<_,Option<String>>(6)?,"review_kind":r.get::<_,Option<String>>(7)?,"review_after":r.get::<_,Option<String>>(8)?,"trigger_state":r.get::<_,String>(9)?,"checked_at":r.get::<_,Option<String>>(10)?,"outcome":r.get::<_,Option<String>>(11)?,"detail":r.get::<_,Option<String>>(12)?}))).optional()?;
    Ok(v)
}
fn reasons(c: &Connection, cid: &str) -> Result<Vec<String>> {
    let mut s=c.prepare("SELECT kind||':'||reason_key FROM review_reasons WHERE claim_id=? AND active=1 ORDER BY kind,reason_key")?;
    let result = s
        .query_map(params![cid], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(result)
}
async fn present_id(c: &mut Connection, cid: &str) -> Result<serde_json::Value> {
    present_id_with_runner(c, cid, Arc::new(SystemdReviewRunner)).await
}

async fn present_id_with_runner(
    c: &mut Connection,
    cid: &str,
    runner: Arc<dyn ReviewRunner>,
) -> Result<serde_json::Value> {
    claim_row(c, cid)?.context("claim not found")?;
    refresh_expirations(c, std::slice::from_ref(&cid.to_string()))?;
    run_reviews_with_runner(c, &[cid.to_string()], runner).await?;
    let mut v = claim_row(c, cid)?.context("claim disappeared")?;
    add_presentation(c, &mut v, cid)?;
    Ok(v)
}
fn safe_fts_query(input: &str) -> String {
    input
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| format!("\"{word}\"*"))
        .collect::<Vec<_>>()
        .join(" OR ")
}
fn retrieve_ids(c: &Connection, q: &str, limit: u32) -> Result<Vec<String>> {
    if limit == 0 || limit > MAX_SEARCH_LIMIT {
        bail!("limit must be between 1 and {MAX_SEARCH_LIMIT}")
    }
    let query = safe_fts_query(q);
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let mut st = c.prepare("SELECT claim_id, bm25(search_fts) FROM search_fts WHERE search_fts MATCH ? ORDER BY bm25(search_fts)")?;
    let mut ids = Vec::new();
    for row in st.query_map(params![query], |r| r.get::<_, String>(0))? {
        let id = row?;
        if !ids.contains(&id) {
            ids.push(id);
            if ids.len() >= limit as usize {
                break;
            }
        }
    }
    Ok(ids)
}
async fn present(c: &mut Connection, q: &str, limit: u32, mode: &str) -> Result<serde_json::Value> {
    let ids = retrieve_ids(c, q, limit)?;
    refresh_expirations(c, &ids)?;
    run_reviews(c, &ids).await?;
    let mut out = Vec::new();
    for cid in ids {
        if let Some(mut v) = claim_row(c, &cid)? {
            add_presentation(c, &mut v, &cid)?;
            out.push(v)
        }
    }
    Ok(serde_json::json!({"mode":mode,"claims":out}))
}
fn refresh_expirations(c: &mut Connection, ids: &[String]) -> Result<()> {
    let tx = c.transaction()?;
    for cid in ids {
        let expired: Option<String> = tx.query_row(
            "SELECT valid_until FROM claims WHERE id=?",
            params![cid],
            |r| r.get(0),
        )?;
        if expired.as_deref().is_some_and(|x| {
            chrono::DateTime::parse_from_rfc3339(x)
                .map(|t| t < chrono::Utc::now())
                .unwrap_or(false)
        }) {
            if activate_reason(&tx, cid, "expiration", "valid_until", "claim expired")? {
                event(
                    &tx,
                    "claim",
                    cid,
                    "expiration",
                    serde_json::json!({"valid_until":expired}),
                )?;
            }
        }
    }
    tx.commit()?;
    Ok(())
}
fn add_presentation(c: &Connection, v: &mut serde_json::Value, cid: &str) -> Result<()> {
    let rs = reasons(c, cid)?;
    let unknown = v.get("outcome").and_then(|x| x.as_str()) == Some("unknown");
    let expired = v
        .get("valid_until")
        .and_then(|x| x.as_str())
        .is_some_and(|x| {
            chrono::DateTime::parse_from_rfc3339(x)
                .map(|t| t < chrono::Utc::now())
                .unwrap_or(false)
        });
    let condition_unknown = v.get("review_kind").and_then(|x| x.as_str()) == Some("condition")
        && v.get("trigger_state").and_then(|x| x.as_str()) == Some("pending");
    let check_in_progress = v.get("trigger_state").and_then(|x| x.as_str()) == Some("executing");
    let needs = !rs.is_empty() || expired;
    v["freshness"] = serde_json::json!(if needs {
        "needs_review"
    } else if unknown || check_in_progress {
        "unknown"
    } else if condition_unknown {
        "unknown"
    } else {
        "current"
    });
    if condition_unknown {
        v["outcome"] = serde_json::json!("unknown");
        v["detail"] = serde_json::json!("condition reviews have no automatic evaluator in MVP");
    } else if check_in_progress {
        v["outcome"] = serde_json::json!("unknown");
        v["detail"] = serde_json::json!("review check is already running");
    }
    v["active_review_reasons"] = serde_json::to_value(reasons(c, cid)?)?;
    let mut stmt = c.prepare("SELECT s.id,s.title,sr.version,e.locator,e.excerpt FROM claim_evidence ce JOIN evidence e ON e.id=ce.evidence_id JOIN source_revisions sr ON sr.id=e.source_revision_id JOIN sources s ON s.id=sr.source_id WHERE ce.claim_id=? ORDER BY e.observed_at")?;
    let citations = stmt
        .query_map(params![cid], |r| {
            Ok(serde_json::json!({
                "source_id": r.get::<_, String>(0)?, "source_title": r.get::<_, String>(1)?,
                "version": r.get::<_, Option<String>>(2)?, "locator": r.get::<_, String>(3)?,
                "excerpt": r.get::<_, String>(4)?
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    v["citations"] = serde_json::to_value(citations)?;
    Ok(())
}

fn activate_reason(
    tx: &Transaction<'_>,
    claim_id: &str,
    kind: &str,
    reason_key: &str,
    detail: &str,
) -> Result<bool> {
    let active = tx
        .query_row(
            "SELECT active FROM review_reasons WHERE claim_id=? AND kind=? AND reason_key=?",
            params![claim_id, kind, reason_key],
            |row| row.get::<_, bool>(0),
        )
        .optional()?
        .unwrap_or(false);
    tx.execute(
        "INSERT INTO review_reasons(claim_id,kind,reason_key,detail,active) VALUES(?,?,?,?,1) ON CONFLICT(claim_id,kind,reason_key) DO UPDATE SET detail=excluded.detail,active=1",
        params![claim_id, kind, reason_key, detail],
    )?;
    Ok(!active)
}

#[derive(Debug)]
struct CheckResult {
    cid: String,
    result: String,
    detail: String,
    expected_identity: String,
    expected_capabilities: String,
}
struct ReviewJob {
    claim_id: String,
    script: String,
    approved_identity: String,
    capabilities_json: String,
    capabilities: Vec<String>,
}
#[async_trait]
trait ReviewRunner: Send + Sync {
    async fn run(&self, script: &str, capabilities: &[String]) -> Result<(i32, String)>;
}
struct SystemdReviewRunner;
#[async_trait]
impl ReviewRunner for SystemdReviewRunner {
    async fn run(&self, script: &str, capabilities: &[String]) -> Result<(i32, String)> {
        run_script(script, capabilities).await
    }
}
async fn run_reviews(c: &mut Connection, ids: &[String]) -> Result<()> {
    run_reviews_with_runner(c, ids, Arc::new(SystemdReviewRunner)).await
}
async fn run_reviews_with_runner(
    c: &mut Connection,
    ids: &[String],
    runner: Arc<dyn ReviewRunner>,
) -> Result<()> {
    let jobs = claim_review_jobs(c, ids)?;
    let sem = Arc::new(Semaphore::new(MAX_CONCURRENT_CHECKS));
    let mut tasks = Vec::new();
    for job in jobs {
        let permit = sem.clone().acquire_owned().await?;
        let runner = runner.clone();
        tasks.push(tokio::spawn(async move {
            let _p = permit;
            match runner.run(&job.script, &job.capabilities).await {
                Ok((code, detail)) => CheckResult {
                    cid: job.claim_id,
                    result: if code == 0 {
                        "fired"
                    } else if code == 1 {
                        "false"
                    } else {
                        "unknown"
                    }
                    .into(),
                    detail,
                    expected_identity: job.approved_identity,
                    expected_capabilities: job.capabilities_json,
                },
                Err(e) => CheckResult {
                    cid: job.claim_id,
                    result: "unknown".into(),
                    detail: e.to_string(),
                    expected_identity: job.approved_identity,
                    expected_capabilities: job.capabilities_json,
                },
            }
        }))
    }
    let results = futures_join(tasks).await;
    let tx = c.transaction()?;
    for r in results {
        let current: Option<(Option<String>,Option<String>,Option<String>,Option<String>,String)> = tx.query_row("SELECT review_after,review_script_hash,review_approved_hash,review_approved_identity,review_capabilities_json FROM claims WHERE id=? AND trigger_state='executing'", params![r.cid], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).optional()?;
        let valid =
            current
                .as_ref()
                .is_some_and(|(script, script_hash, approved_hash, identity, caps)| {
                    let parsed: Vec<String> = serde_json::from_str(caps).unwrap_or_default();
                    let (Some(script), Some(script_hash), Some(approved_hash), Some(identity)) =
                        (script, script_hash, approved_hash, identity)
                    else {
                        return false;
                    };
                    caps == &r.expected_capabilities
                        && identity == &r.expected_identity
                        && script_hash == &hash(script)
                        && approved_hash == script_hash
                        && identity == &approval_identity(script, &parsed)
                });
        let result = if valid {
            r.result
        } else {
            "unknown".to_string()
        };
        let detail = if valid {
            r.detail
        } else {
            "review identity changed during execution".to_string()
        };
        let next_state = if result == "fired" {
            "fired"
        } else {
            "pending"
        };
        let updated = tx.execute("UPDATE claims SET trigger_state=?,review_started_at=NULL,last_review_checked_at=?,last_review_result=?,last_review_detail=?,updated_at=? WHERE id=? AND trigger_state='executing'",params![next_state,now(),result,detail,now(),r.cid])?;
        if updated == 0 {
            continue;
        }
        if result == "fired" {
            tx.execute(
                "INSERT OR REPLACE INTO review_reasons VALUES(?,?,?,?,1)",
                params![r.cid, "script_trigger", "script", "review script fired"],
            )?;
        }
        event(
            &tx,
            "claim",
            &r.cid,
            "review_execution",
            serde_json::json!({"result":result,"detail":detail}),
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn claim_review_jobs(c: &mut Connection, ids: &[String]) -> Result<Vec<ReviewJob>> {
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let cutoff = (Utc::now() - chrono::Duration::seconds(ABANDONED_CHECK_SECONDS)).to_rfc3339();
    let started_at = now();
    let mut jobs = Vec::new();
    for claim_id in ids {
        let recovered = tx.execute(
            "UPDATE claims SET trigger_state='pending',review_started_at=NULL,last_review_result='unknown',last_review_detail='abandoned review execution recovered' WHERE id=? AND trigger_state='executing' AND review_started_at < ?",
            params![claim_id, cutoff],
        )?;
        if recovered > 0 {
            event(
                &tx,
                "claim",
                claim_id,
                "review_recovered",
                serde_json::json!({}),
            )?;
        }

        let row: Option<(
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            String,
        )> = tx
            .query_row(
                "SELECT review_after,review_script_hash,review_approved_hash,review_approved_identity,review_capabilities_json,trigger_state FROM claims WHERE id=? AND review_kind='script'",
                params![claim_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
            )
            .optional()?;
        let Some((script, script_hash, approved_hash, approved_identity, caps_json, state)) = row
        else {
            continue;
        };
        if state != "pending" {
            continue;
        }
        let capabilities =
            canonical_capabilities(&serde_json::from_str::<Vec<String>>(&caps_json)?)?;
        let canonical_json = serde_json::to_string(&capabilities)?;
        let approved = match (&script_hash, &approved_hash, &approved_identity) {
            (Some(script_hash), Some(approved_hash), Some(identity)) => {
                script_hash == &hash(&script)
                    && approved_hash == script_hash
                    && identity == &approval_identity(&script, &capabilities)
                    && caps_json == canonical_json
            }
            _ => false,
        };
        if !approved {
            tx.execute(
                "UPDATE claims SET last_review_checked_at=?,last_review_result='unknown',last_review_detail='unapproved or changed script',updated_at=? WHERE id=? AND trigger_state='pending'",
                params![started_at, started_at, claim_id],
            )?;
            continue;
        }
        let claimed = tx.execute(
            "UPDATE claims SET trigger_state='executing',review_started_at=?,updated_at=? WHERE id=? AND trigger_state='pending'",
            params![started_at, started_at, claim_id],
        )?;
        if claimed == 1 {
            jobs.push(ReviewJob {
                claim_id: claim_id.clone(),
                script,
                approved_identity: approved_identity.unwrap(),
                capabilities_json: canonical_json,
                capabilities,
            });
        }
    }
    tx.commit()?;
    Ok(jobs)
}
async fn futures_join(tasks: Vec<tokio::task::JoinHandle<CheckResult>>) -> Vec<CheckResult> {
    let mut out = Vec::new();
    for t in tasks {
        if let Ok(x) = t.await {
            out.push(x)
        }
    }
    out
}
async fn run_script(script: &str, capabilities: &[String]) -> Result<(i32, String)> {
    const SYSTEMD_RUN: &str = "/run/current-system/sw/bin/systemd-run";
    if !Path::new(SYSTEMD_RUN).is_file() {
        bail!("trusted systemd-run executable is unavailable")
    }
    if capabilities
        .iter()
        .any(|cap| !CAPABILITIES.contains(&cap.as_str()))
    {
        bail!("unknown review capability")
    }
    let network = capabilities
        .iter()
        .any(|cap| cap == "network" || cap == "gh-auth");
    let gh_auth = capabilities.iter().any(|cap| cap == "gh-auth");
    let home = env::var("HOME").context("HOME unavailable")?;
    let gh_config = PathBuf::from(&home).join(".config/gh");
    if gh_auth && !gh_config.is_dir() {
        bail!("gh-auth requested but ~/.config/gh is unavailable")
    }
    let profile_path = format!(
        "/etc/profiles/per-user/{}/bin:/run/current-system/sw/bin:/usr/bin:/bin",
        env::var("USER").unwrap_or_default()
    );
    let path_env = format!("PATH={profile_path}");
    let home_env = if gh_auth {
        format!("HOME={home}")
    } else {
        "HOME=/nonexistent".to_string()
    };
    let mut cmd = Command::new(SYSTEMD_RUN);
    cmd.args([
        "--user",
        "--pipe",
        "--wait",
        "--collect",
        "--quiet",
        "-p",
        "NoNewPrivileges=yes",
        "-p",
        if network {
            "PrivateNetwork=no"
        } else {
            "PrivateNetwork=yes"
        },
        "-p",
        "PrivateTmp=yes",
        "-p",
        "PrivateDevices=yes",
        "-p",
        "RestrictNamespaces=yes",
        "-p",
        "CapabilityBoundingSet=",
        "-p",
        "DevicePolicy=closed",
        "-p",
        "ProtectSystem=strict",
        "-p",
        "ProtectHome=tmpfs",
        "-p",
        "ProtectProc=invisible",
        "-p",
        "ProcSubset=pid",
        "-p",
        "ProtectControlGroups=yes",
        "-p",
        "ProtectKernelModules=yes",
        "-p",
        "ProtectKernelTunables=yes",
        "-p",
        "LockPersonality=yes",
        "-p",
        "RestrictSUIDSGID=yes",
        "-p",
        "MemoryDenyWriteExecute=yes",
        "-p",
        "SystemCallArchitectures=native",
        "-p",
        "TasksMax=16",
        "-p",
        "MemoryMax=128M",
        "-p",
        REVIEW_RUNTIME_LIMIT,
        "-p",
        "WorkingDirectory=/tmp",
    ]);
    if gh_auth {
        cmd.args(["-p", &format!("BindReadOnlyPaths={}", gh_config.display())]);
        // The only home path permitted to a transient service is the read-only gh config.
    }
    cmd.args([
        "/run/current-system/sw/bin/env",
        "-i",
        &path_env,
        &home_env,
        "/run/current-system/sw/bin/bash",
        "--noprofile",
        "--norc",
        "-c",
        script,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    cmd.kill_on_drop(true);
    let mut child = cmd.spawn().context("failed to start review sandbox")?;
    let stdout = child.stdout.take().context("review stdout unavailable")?;
    let stderr = child.stderr.take().context("review stderr unavailable")?;
    let stdout_task = tokio::spawn(read_capped(stdout));
    let stderr_task = tokio::spawn(read_capped(stderr));
    let status = match timeout(CHECK_TIMEOUT, child.wait()).await {
        Ok(result) => result.context("failed to wait for review sandbox")?,
        Err(_) => {
            let _ = child.kill().await;
            stdout_task.abort();
            stderr_task.abort();
            bail!("review timeout")
        }
    };
    let stdout = stdout_task.await.context("stdout reader failed")??;
    let stderr = stderr_task.await.context("stderr reader failed")??;
    let mut text = String::from_utf8_lossy(&stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&stderr));
    text.truncate(MAX_OUTPUT);
    Ok((status.code().unwrap_or(2), text))
}

async fn read_capped(mut reader: impl AsyncRead + Unpin) -> Result<Vec<u8>> {
    const READ_BUFFER_SIZE: usize = 8 * 1024;
    let mut result = Vec::with_capacity(MAX_OUTPUT);
    let mut buffer = [0_u8; READ_BUFFER_SIZE];
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            return Ok(result);
        }
        let remaining = MAX_OUTPUT.saturating_sub(result.len());
        result.extend_from_slice(&buffer[..read.min(remaining)]);
    }
}

fn review_approval(c: &Connection, a: ReviewApprovalArgs) -> Result<serde_json::Value> {
    let script: String = c
        .query_row(
            "SELECT review_after FROM claims WHERE id=? AND review_kind='script' AND trigger_state='pending'",
            params![a.id],
            |row| row.get(0),
        )
        .context("pending script review not found")?;
    let capabilities = canonical_capabilities(&a.capabilities)?;
    Ok(serde_json::json!({
        "id": a.id,
        "script": script,
        "capabilities": capabilities,
        "approval_hash": approval_identity(&script, &capabilities),
    }))
}

fn approve(c: &mut Connection, a: ApproveArgs) -> Result<serde_json::Value> {
    let tx = c.transaction()?;
    let script: String = tx
        .query_row(
            "SELECT review_after FROM claims WHERE id=? AND review_kind='script' AND trigger_state='pending'",
            params![a.id],
            |r| r.get(0),
        )
        .context("pending script review not found")?;
    let capabilities = canonical_capabilities(&a.capabilities)?;
    let identity = approval_identity(&script, &capabilities);
    if identity != a.hash {
        bail!("hash does not match script and canonical capabilities")
    }
    let caps = serde_json::to_string(&capabilities)?;
    tx.execute("UPDATE claims SET review_script_hash=?,review_approved_hash=?,review_approved_identity=?,review_capabilities_json=?,updated_at=? WHERE id=?",params![hash(&script),hash(&script),a.hash,caps,now(),a.id])?;
    event(
        &tx,
        "claim",
        &a.id,
        "approve_review",
        serde_json::json!({"hash":a.hash,"capabilities":capabilities}),
    )?;
    tx.commit()?;
    Ok(serde_json::json!({"id":a.id,"approved":true}))
}
fn review(c: &mut Connection, a: IdArg) -> Result<serde_json::Value> {
    let tx = c.transaction()?;
    if tx
        .query_row(
            "SELECT 1 FROM claims WHERE id=? AND review_kind='script' AND trigger_state='fired'",
            params![a.id],
            |_| Ok(()),
        )
        .optional()?
        .is_none()
    {
        bail!("claim does not have a fired script review")
    }
    tx.execute(
        "UPDATE claims SET trigger_state='retired',last_verified_at=?,last_review_result='reviewed',last_review_detail='review acknowledged',updated_at=? WHERE id=?",
        params![now(), now(), a.id],
    )?;
    tx.execute(
        "UPDATE review_reasons SET active=0 WHERE claim_id=? AND kind='script_trigger'",
        params![a.id],
    )?;
    event(&tx, "claim", &a.id, "review", serde_json::json!({}))?;
    tx.commit()?;
    Ok(serde_json::json!({"id":a.id,"reviewed":true}))
}
fn relate(c: &mut Connection, a: RelateArgs) -> Result<serde_json::Value> {
    let tx = c.transaction()?;
    if !["supersedes", "contradicts", "refines", "depends_on"].contains(&a.relation.as_str()) {
        bail!("invalid relation")
    }
    for claim in [&a.from, &a.to] {
        if tx
            .query_row(
                "SELECT 1 FROM claims WHERE id=?",
                params![claim],
                |_| Ok(()),
            )
            .optional()?
            .is_none()
        {
            bail!("claim not found")
        }
    }
    tx.execute(
        "INSERT INTO claim_relations VALUES(?,?,?)",
        params![a.from, a.relation, a.to],
    )?;
    if a.relation == "supersedes" {
        tx.execute(
            "UPDATE claims SET status='superseded' WHERE id=?",
            params![a.to],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO review_reasons VALUES(?,?,?,?,1)",
            params![a.to, "superseded", a.from, "claim superseded"],
        )?;
    }
    if a.relation == "contradicts" {
        for (cid, opposite) in [(&a.from, &a.to), (&a.to, &a.from)] {
            tx.execute(
                "UPDATE claims SET status='disputed' WHERE id=?",
                params![cid],
            )?;
            tx.execute(
                "INSERT OR REPLACE INTO review_reasons VALUES(?,?,?,?,1)",
                params![cid, "contradiction", opposite, "contradictory claim"],
            )?;
        }
    }
    event(
        &tx,
        "claim",
        &a.from,
        "relate",
        serde_json::json!({"relation":a.relation,"to":a.to}),
    )?;
    tx.commit()?;
    Ok(serde_json::json!({"from":a.from,"relation":a.relation,"to":a.to}))
}
fn evidence_add(c: &mut Connection, a: EvidenceArgs) -> Result<serde_json::Value> {
    if !["supports", "contradicts", "context"].contains(&a.stance.as_str()) {
        bail!("invalid evidence stance")
    }
    if a.locator.trim().is_empty() || a.excerpt.trim().is_empty() {
        bail!("evidence locator and excerpt must not be empty")
    }
    let tx = c.transaction()?;
    if tx
        .query_row("SELECT 1 FROM claims WHERE id=?", params![a.claim], |_| {
            Ok(())
        })
        .optional()?
        .is_none()
    {
        bail!("claim not found")
    }
    let sid: String = tx
        .query_row(
            "SELECT id FROM sources WHERE id=?",
            params![a.source],
            |r| r.get(0),
        )
        .context("source not found")?;
    let rev: Option<String> = tx
        .query_row(
            "SELECT current_revision_id FROM sources WHERE id=?",
            params![sid],
            |r| r.get(0),
        )
        .optional()?;
    let rev = rev.context("source has no revision")?;
    let eid = id();
    tx.execute("INSERT OR IGNORE INTO evidence(id,source_revision_id,locator,excerpt,excerpt_hash,observed_at) VALUES(?,?,?,?,?,?)",params![eid,rev,a.locator,a.excerpt,hash(&a.excerpt),now()])?;
    let eid: String = tx.query_row(
        "SELECT id FROM evidence WHERE source_revision_id=? AND locator=? AND excerpt_hash=?",
        params![rev, a.locator, hash(&a.excerpt)],
        |r| r.get(0),
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO claim_evidence VALUES(?,?,?)",
        params![a.claim, eid, a.stance],
    )?;
    if a.stance == "supports" {
        tx.execute(
            "UPDATE claims SET status=CASE WHEN status='provisional' THEN 'verified' ELSE status END,last_verified_at=?,updated_at=? WHERE id=?",
            params![now(), now(), a.claim],
        )?;
        tx.execute(
            "UPDATE review_reasons SET active=0 WHERE claim_id=? AND kind='source_changed' AND reason_key=?",
            params![a.claim, sid],
        )?;
    } else if a.stance == "contradicts" {
        tx.execute(
            "UPDATE claims SET status='disputed',updated_at=? WHERE id=?",
            params![now(), a.claim],
        )?;
        activate_reason(
            &tx,
            &a.claim,
            "contradiction",
            &format!("evidence:{eid}"),
            "contradictory evidence",
        )?;
    }
    event(
        &tx,
        "claim",
        &a.claim,
        "evidence_add",
        serde_json::json!({"evidence":eid}),
    )?;
    rebuild_fts(&tx)?;
    tx.commit()?;
    Ok(serde_json::json!({"id":eid}))
}
fn audit(c: &Connection) -> Result<serde_json::Value> {
    let mut s=c.prepare("SELECT occurred_at,object_type,object_id,operation,details_json FROM events ORDER BY id DESC LIMIT 100")?;
    let rows=s.query_map([],|r|Ok(serde_json::json!({"at":r.get::<_,String>(0)?,"object_type":r.get::<_,String>(1)?,"object_id":r.get::<_,String>(2)?,"operation":r.get::<_,String>(3)?,"details":serde_json::from_str::<serde_json::Value>(&r.get::<_,String>(4)?).unwrap_or_default()})))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let integrity: String = c.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    let active: i64 = c.query_row(
        "SELECT count(*) FROM review_reasons WHERE active=1",
        [],
        |r| r.get(0),
    )?;
    let unknown: i64 = c.query_row("SELECT count(*) FROM claims WHERE review_kind='script' AND (last_review_result='unknown' OR review_approved_identity IS NULL)", [], |r| r.get(0))?;
    let mut expiry_stmt =
        c.prepare("SELECT valid_until FROM claims WHERE valid_until IS NOT NULL")?;
    let expired = expiry_stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .filter_map(Result::ok)
        .filter(|value| {
            chrono::DateTime::parse_from_rfc3339(value)
                .map(|time| time < chrono::Utc::now())
                .unwrap_or(false)
        })
        .count();
    Ok(
        serde_json::json!({"integrity_check":integrity,"active_review_reasons":active,"unknown_or_unapproved_checks":unknown,"expired_claims":expired,"events":rows}),
    )
}
fn managed_output_path(c: &Connection, requested: &Path, directory: &str) -> Result<PathBuf> {
    let database = PathBuf::from(c.path().context("database has no filesystem path")?);
    let root = database.parent().context("database path has no parent")?;
    let base = root.join(directory);
    fs::create_dir_all(&base)?;

    let relative = if requested.is_absolute() {
        requested
            .strip_prefix(&base)
            .with_context(|| format!("output must be inside {}", base.display()))?
            .to_path_buf()
    } else {
        requested.to_path_buf()
    };
    let components = relative.components().collect::<Vec<_>>();
    if components.len() != 1 || !matches!(components[0], Component::Normal(_)) {
        bail!("output must be a single file name")
    }

    let output = base.join(relative);
    if fs::symlink_metadata(&output).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        bail!("output must not be a symbolic link")
    }
    Ok(output)
}
async fn export(c: &mut Connection, a: ExportArgs) -> Result<serde_json::Value> {
    let mut ids_stmt = c.prepare("SELECT id FROM claims ORDER BY created_at")?;
    let ids = ids_stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(ids_stmt);
    refresh_expirations(c, &ids)?;
    run_reviews(c, &ids).await?;
    let mut values = Vec::new();
    for cid in ids {
        let mut value = claim_row(c, &cid)?.context("claim disappeared during export")?;
        add_presentation(c, &mut value, &cid)?;
        values.push(value);
    }
    let claims = serde_json::Value::Array(values);
    let text: String = match a.format {
        ExportFormat::Jsonl => claims
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .map(|x| x.to_string() + "\n")
            .collect(),
        ExportFormat::Markdown => claims
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .map(|x| {
                format!(
                    "## {}\n\n- Freshness: {}\n- Status: {}\n\n",
                    x["text"], x["freshness"], x["status"]
                )
            })
            .collect(),
    };
    let output = managed_output_path(c, &a.output, "exports")?;
    fs::write(&output, text)?;
    Ok(serde_json::json!({"output":output}))
}
fn backup(c: &Connection, path: PathBuf) -> Result<serde_json::Value> {
    let output = managed_output_path(c, &path, "backups")?;
    if output.exists() {
        bail!("backup output already exists")
    }
    let mut dest = Connection::open(&output)?;
    let b = Backup::new(c, &mut dest)?;
    b.run_to_completion(100, Duration::from_millis(10), None)?;
    Ok(serde_json::json!({"output":output}))
}

fn main() {
    let result = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(anyhow::Error::from)
        .and_then(|rt| rt.block_on(main_async()));
    match result {
        Ok(data) => println!("{}", output_envelope(Ok(data))),
        Err(e) => {
            eprintln!("{e:#}");
            println!("{}", output_envelope(Err(e)));
            std::process::exit(1)
        }
    }
}
fn output_envelope(result: Result<serde_json::Value>) -> serde_json::Value {
    match result {
        Ok(data) => serde_json::json!({"ok":true,"data":data}),
        Err(error) => serde_json::json!({"ok":false,"error":error.to_string()}),
    }
}

mod filing;
#[cfg(test)]
mod tests;
