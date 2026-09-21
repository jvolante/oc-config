use super::*;
use async_trait::async_trait;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};

#[derive(Clone)]
pub(crate) struct Extracted {
    pub(crate) text: String,
    pub(crate) pages: Option<i64>,
}

#[async_trait]
pub(crate) trait PdfExtractor: Send + Sync {
    async fn extract(&self, archive: &Path) -> Result<Extracted>;
}

pub(crate) struct SystemdPdfExtractor;

#[derive(Debug)]
pub(crate) struct PdfRunOutput {
    pub(crate) status: i32,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

#[async_trait]
pub(crate) trait PdfSandboxRunner: Send + Sync {
    async fn run(&self, archive: &Path, path: &str, trusted_path: &str) -> Result<PdfRunOutput>;
}

pub(crate) async fn extract_pdf_with_runner(
    archive: &Path,
    runner: &dyn PdfSandboxRunner,
) -> Result<Extracted> {
    let trusted_path = env::var("PATH").context("trusted PATH is unavailable")?;
    let pdf_path = executable_in_path("pdftotext", &trusted_path)?;
    let output = runner
        .run(
            archive,
            pdf_path.to_str().context("invalid pdftotext path")?,
            &trusted_path,
        )
        .await?;
    if output.status != 0 {
        bail!(
            "pdftotext failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let text = String::from_utf8(output.stdout).context("pdftotext emitted invalid UTF-8")?;
    Ok(Extracted {
        pages: Some(text.matches('\u{c}').count() as i64 + 1),
        text,
    })
}

struct SystemdPdfRunner;

fn executable_in_path(name: &str, path: &str) -> Result<PathBuf> {
    path.split(':')
        .map(Path::new)
        .map(|directory| directory.join(name))
        .find(|candidate| {
            fs::metadata(candidate)
                .map(|metadata| metadata.is_file())
                .unwrap_or(false)
        })
        .with_context(|| format!("trusted PATH does not contain executable {name}"))
}

pub(crate) fn pdf_command_args(archive: &Path, _pdf_path: &str, trusted_path: &str) -> Vec<String> {
    vec![
        "--user".into(),
        "--pipe".into(),
        "--wait".into(),
        "--collect".into(),
        "--quiet".into(),
        "-p".into(),
        "NoNewPrivileges=yes".into(),
        "-p".into(),
        "PrivateNetwork=yes".into(),
        "-p".into(),
        "PrivateTmp=yes".into(),
        "-p".into(),
        "PrivateDevices=yes".into(),
        "-p".into(),
        "ProtectProc=invisible".into(),
        "-p".into(),
        "ProtectSystem=strict".into(),
        "-p".into(),
        "ProtectHome=tmpfs".into(),
        "-p".into(),
        "RestrictNamespaces=yes".into(),
        "-p".into(),
        "CapabilityBoundingSet=".into(),
        "-p".into(),
        "DevicePolicy=closed".into(),
        "-p".into(),
        "TasksMax=16".into(),
        "-p".into(),
        "MemoryMax=256M".into(),
        "-p".into(),
        "RuntimeMaxSec=20s".into(),
        "-p".into(),
        format!("BindReadOnlyPaths={}:{}", archive.display(), "/input.pdf"),
        "/run/current-system/sw/bin/env".into(),
        "-i".into(),
        format!("PATH={trusted_path}"),
        "pdftotext".into(),
        "-layout".into(),
        "-enc".into(),
        "UTF-8".into(),
        "/input.pdf".into(),
        "-".into(),
    ]
}

#[async_trait]
impl PdfSandboxRunner for SystemdPdfRunner {
    async fn run(&self, archive: &Path, path: &str, trusted_path: &str) -> Result<PdfRunOutput> {
        let binding = format!("BindReadOnlyPaths={}:{}", archive.display(), "/input.pdf");
        let args = pdf_command_args(archive, path, trusted_path);
        debug_assert_eq!(args.iter().filter(|arg| *arg == &binding).count(), 1);
        let mut command = Command::new(SYSTEMD_RUN);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().context("failed to start PDF sandbox")?;
        let stdout = child.stdout.take().context("PDF stdout unavailable")?;
        let stderr = child.stderr.take().context("PDF stderr unavailable")?;
        let out_task = tokio::spawn(read_capped_to(stdout, MAX_PDF_OUTPUT_BYTES));
        let err_task = tokio::spawn(read_capped_to(stderr, MAX_OUTPUT));
        let status = match timeout(PDF_TIMEOUT, child.wait()).await {
            Ok(value) => value.context("failed waiting for PDF sandbox")?,
            Err(_) => {
                let _ = child.kill().await;
                bail!("PDF extraction timeout")
            }
        };
        Ok(PdfRunOutput {
            status: status.code().unwrap_or(2),
            stdout: out_task.await.context("PDF stdout reader failed")??,
            stderr: err_task.await.context("PDF stderr reader failed")??,
        })
    }
}

#[async_trait]
impl PdfExtractor for SystemdPdfExtractor {
    async fn extract(&self, archive: &Path) -> Result<Extracted> {
        extract_pdf_with_runner(archive, &SystemdPdfRunner).await
    }
}

async fn read_capped_to(mut reader: impl AsyncRead + Unpin, cap: usize) -> Result<Vec<u8>> {
    let mut result = Vec::with_capacity(cap.min(64 * 1024));
    let mut buffer = [0_u8; 8192];
    loop {
        let count = reader.read(&mut buffer).await?;
        if count == 0 {
            return Ok(result);
        }
        if result.len().saturating_add(count) > cap {
            bail!("extractor output exceeds limit")
        }
        result.extend_from_slice(&buffer[..count]);
    }
}

fn valid_job_path(root: &Path, value: &str) -> Result<PathBuf> {
    let relative = Path::new(value)
        .strip_prefix("inbox")
        .ok()
        .filter(|p| {
            p.components().count() == 1
                && matches!(p.components().next(), Some(Component::Normal(_)))
        })
        .context("published path is outside inbox")?;
    let inbox = root.canonicalize()?;
    let path = inbox.join(relative);
    let canonical = path
        .canonicalize()
        .context("published inbox object is missing")?;
    if !canonical.starts_with(&inbox) || !fs::symlink_metadata(&path)?.file_type().is_file() {
        bail!("published inbox object is not a regular non-symlink file")
    }
    Ok(path)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("output path has no parent")?;
    private_dir(parent)?;
    let temporary = parent.join(format!(".{}.tmp", id()));
    let result = (|| -> Result<()> {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        output.write_all(bytes)?;
        output.sync_all()?;
        drop(output);
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn archive_object(root: &Path, digest: &str, extension: &str, source: &Path) -> Result<PathBuf> {
    let directory = root.join("archive/objects").join(&digest[..2]);
    private_dir(&directory)?;
    let destination = directory.join(format!("{}.{}", &digest[2..], extension));
    if fs::symlink_metadata(&destination).is_ok() {
        if !fs::symlink_metadata(&destination)?.file_type().is_file() {
            bail!("archive collision is not a regular file")
        }
        let mut input = OpenOptions::new().read(true).open(&destination)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 65536];
        loop {
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        if format!("{:x}", hasher.finalize()) != digest {
            bail!("archive object collision has incorrect digest")
        }
        let mut source = OpenOptions::new().read(true).open(source)?;
        let mut source_hasher = Sha256::new();
        loop {
            let count = source.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            source_hasher.update(&buffer[..count]);
        }
        if format!("{:x}", source_hasher.finalize()) != digest {
            bail!("inbox bytes changed before archival")
        }
        return Ok(destination);
    }
    let temporary = directory.join(format!(".{}.tmp", id()));
    let result = (|| -> Result<()> {
        let mut input = OpenOptions::new().read(true).open(source)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 65536];
        loop {
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
            output.write_all(&buffer[..count])?;
        }
        if format!("{:x}", hasher.finalize()) != digest {
            bail!("inbox bytes changed during archival copy")
        }
        output.sync_all()?;
        drop(output);
        fs::rename(&temporary, &destination)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map(|_| destination)
}

fn normalize_text(bytes: &[u8]) -> Result<String> {
    let text = std::str::from_utf8(bytes)
        .context("file is not UTF-8")?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    if text.trim().is_empty() {
        bail!("text has no meaningful non-whitespace content")
    }
    Ok(text)
}

fn split_chunks(
    text: &str,
    pages: Option<i64>,
) -> Vec<(Option<i64>, usize, usize, String, String)> {
    let mut result = Vec::new();
    for (page_index, page) in text.split('\u{c}').enumerate() {
        let page_number = pages.map(|_| page_index as i64 + 1);
        let lines: Vec<&str> = page.lines().collect();
        let mut start = 0;
        while start < lines.len() {
            let mut end = start;
            let mut size = 0;
            while end < lines.len()
                && end - start < MAX_CHUNK_LINES
                && size + lines[end].len() <= MAX_CHUNK_BYTES
            {
                size += lines[end].len();
                end += 1;
            }
            if end == start {
                end += 1;
            }
            let value = lines[start..end].join("\n");
            let locator = match page_number {
                Some(page) => format!("p{page}:L{:03}-L{:03}", start + 1, end),
                None => format!("L{:03}-L{:03}", start + 1, end),
            };
            result.push((page_number, start + 1, end, locator, value));
            start = end;
        }
    }
    result
}

pub(crate) fn has_oversized_line(text: &str) -> bool {
    text.split(['\n', '\u{c}'])
        .any(|line| line.len() > MAX_CHUNK_BYTES)
}

pub(crate) fn claim_jobs(
    c: &mut Connection,
    ids: &[String],
    limit: Option<u32>,
) -> Result<Vec<(String, String, String, String, i64, String, String)>> {
    if let Some(value) = limit {
        if value == 0 || value > MAX_PROCESS_LIMIT {
            bail!("limit must be between 1 and {MAX_PROCESS_LIMIT}");
        }
    }
    for value in ids {
        Uuid::parse_str(value).with_context(|| format!("invalid job id: {value}"))?;
    }
    let now_value = now();
    let lease = (Utc::now() + chrono::Duration::seconds(PROCESS_LEASE_SECONDS)).to_rfc3339();
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute("UPDATE inbox_jobs SET status='pending',lease_owner=NULL,lease_until=NULL,updated_at=? WHERE status='processing' AND lease_until < ?", params![now_value, now_value])?;
    let selected = if ids.is_empty() {
        let count = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_PROCESS_LIMIT);
        let mut statement = tx.prepare(
            "SELECT id FROM inbox_jobs WHERE status='pending' ORDER BY created_at,id LIMIT ?",
        )?;
        let values = statement
            .query_map(params![count], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        values
    } else {
        ids.iter()
            .take(limit.unwrap_or(MAX_PROCESS_LIMIT) as usize)
            .cloned()
            .collect()
    };
    let mut jobs = Vec::new();
    for job_id in selected {
        let owner = id();
        let changed = tx.execute("UPDATE inbox_jobs SET status='processing',lease_owner=?,lease_until=?,updated_at=? WHERE id=? AND status='pending'", params![owner, lease, now_value, job_id])?;
        if changed == 0 {
            if !ids.is_empty() {
                bail!("job is not pending or is leased: {job_id}");
            }
            continue;
        }
        jobs.push(tx.query_row("SELECT id,sha256,original_basename,published_path,byte_count,sensitivity,lease_owner FROM inbox_jobs WHERE id=?", params![job_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get::<_, Option<String>>(3)?.unwrap_or_default(), row.get(4)?, row.get(5)?, row.get(6)?)))?);
    }
    tx.commit()?;
    Ok(jobs)
}

pub(crate) fn select_job_ids(
    c: &mut Connection,
    ids: &[String],
    limit: Option<u32>,
) -> Result<Vec<String>> {
    if let Some(value) = limit {
        if value == 0 || value > MAX_PROCESS_LIMIT {
            bail!("limit must be between 1 and {MAX_PROCESS_LIMIT}");
        }
    }
    for value in ids {
        Uuid::parse_str(value).with_context(|| format!("invalid job id: {value}"))?;
    }
    let now_value = now();
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "UPDATE inbox_jobs SET status='pending',lease_owner=NULL,lease_until=NULL,updated_at=? WHERE status='processing' AND lease_until < ?",
        params![now_value, now_value],
    )?;
    let selected = if ids.is_empty() {
        let count = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_PROCESS_LIMIT);
        let mut statement = tx.prepare(
            "SELECT id FROM inbox_jobs WHERE status='pending' ORDER BY created_at,id LIMIT ?",
        )?;
        let values = statement
            .query_map(params![count], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        values
    } else {
        ids.iter()
            .take(limit.unwrap_or(MAX_PROCESS_LIMIT) as usize)
            .cloned()
            .collect()
    };
    tx.commit()?;
    Ok(selected)
}

async fn quarantine(
    c: &mut Connection,
    root: &Path,
    job: &str,
    path: &Path,
    digest: &str,
    lease_owner: &str,
    status: &str,
    detail: &str,
) -> Result<serde_json::Value> {
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let owns: bool = tx.query_row(
        "SELECT status='processing' AND lease_owner=? AND lease_until > ? FROM inbox_jobs WHERE id=?",
        params![lease_owner, now(), job],
        |row| row.get(0),
    )?;
    if !owns {
        bail!("processing lease was lost")
    }
    let destination = root
        .join("quarantine")
        .join(format!("job-{job}-{digest}.bin"));
    if path.exists() {
        fs::rename(path, destination).context("failed to preserve quarantined bytes")?;
    }
    let changed = tx.execute("UPDATE inbox_jobs SET status=?,error_detail=?,lease_owner=NULL,lease_until=NULL,updated_at=? WHERE id=? AND status='processing' AND lease_owner=? AND lease_until > ?", params![status, detail, now(), job, lease_owner, now()])?;
    if changed != 1 {
        bail!("processing lease was lost")
    }
    event(
        &tx,
        "inbox_job",
        job,
        "quarantine",
        serde_json::json!({"status":status,"detail":detail}),
    )?;
    tx.commit()?;
    Ok(serde_json::json!({"id":job,"status":status,"detail":detail}))
}

async fn process_one(
    c: &mut Connection,
    root: &Path,
    job: (String, String, String, String, i64, String, String),
    extractor: &dyn PdfExtractor,
) -> Result<serde_json::Value> {
    let (job_id, expected, basename, published, byte_count, sensitivity, lease_owner) = job;
    if !c.query_row("SELECT status='processing' AND lease_owner=? AND lease_until > ? FROM inbox_jobs WHERE id=?", params![lease_owner, now(), job_id], |row| row.get::<_, bool>(0))? {
        bail!("processing lease was lost")
    }
    let path = valid_job_path(&root.join("inbox"), &published)?;
    let bytes = fs::read(&path)?;
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if actual != expected {
        return quarantine(
            c,
            root,
            &job_id,
            &path,
            &expected,
            &lease_owner,
            "quarantined",
            "inbox bytes do not match recorded SHA-256",
        )
        .await;
    }
    let (media_type, extension) = if bytes.starts_with(b"%PDF-") {
        ("application/pdf", "pdf")
    } else if let Ok(text) = std::str::from_utf8(&bytes) {
        if text.trim().is_empty() {
            return quarantine(
                c,
                root,
                &job_id,
                &path,
                &expected,
                &lease_owner,
                "quarantined",
                "unsupported media type",
            )
            .await;
        }
        ("text/plain", "txt")
    } else {
        return quarantine(
            c,
            root,
            &job_id,
            &path,
            &expected,
            &lease_owner,
            "quarantined",
            "unsupported media type",
        )
        .await;
    };
    let archive = match archive_object(root, &expected, extension, &path) {
        Ok(value) => value,
        Err(error) => {
            return quarantine(
                c,
                root,
                &job_id,
                &path,
                &expected,
                &lease_owner,
                "quarantined",
                &error.to_string(),
            )
            .await
        }
    };
    let extracted = if media_type == "application/pdf" {
        extractor.extract(&archive).await
    } else {
        normalize_text(&bytes).map(|text| Extracted { text, pages: None })
    };
    let extracted = match extracted {
        Ok(value) => value,
        Err(error) => {
            return quarantine(
                c,
                root,
                &job_id,
                &path,
                &expected,
                &lease_owner,
                "quarantined",
                &error.to_string(),
            )
            .await
        }
    };
    if media_type == "application/pdf" && extracted.text.replace('\u{c}', "").trim().is_empty() {
        return quarantine(
            c,
            root,
            &job_id,
            &path,
            &expected,
            &lease_owner,
            "quarantined",
            "needs OCR: PDF has negligible extractable text",
        )
        .await;
    }
    if has_oversized_line(&extracted.text) {
        return quarantine(
            c,
            root,
            &job_id,
            &path,
            &expected,
            &lease_owner,
            "quarantined",
            "line exceeds maximum chunk size",
        )
        .await;
    }
    let text_path = root
        .join("archive/text")
        .join(&expected[..2])
        .join(format!("{}.txt", &expected[2..]));
    atomic_write(&text_path, extracted.text.as_bytes())?;
    let archive_rel = format!(
        "archive/objects/{}/{}.{}",
        &expected[..2],
        &expected[2..],
        extension
    );
    let text_rel = format!("archive/text/{}/{}.txt", &expected[..2], &expected[2..]);
    let tx = c.transaction()?;
    let timestamp = now();
    let source_id = id();
    let revision_id = id();
    let document_id = id();
    tx.execute("INSERT INTO sources(id,kind,external_id,uri,title,sensitivity,metadata_json,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)", params![source_id,"local",format!("sha256:{expected}"),archive_rel,basename,sensitivity,"{}",timestamp,timestamp])?;
    tx.execute("INSERT INTO source_revisions(id,source_id,version,content_hash,observed_at) VALUES(?,?,?,?,?)", params![revision_id,source_id,expected,expected,timestamp])?;
    tx.execute(
        "UPDATE sources SET current_revision_id=? WHERE id=?",
        params![revision_id, source_id],
    )?;
    tx.execute(
        "INSERT INTO documents VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        params![
            document_id,
            expected,
            media_type,
            extension,
            basename,
            basename,
            byte_count,
            sensitivity,
            archive_rel,
            text_rel,
            "filed",
            extracted.pages,
            source_id,
            revision_id,
            timestamp,
            timestamp
        ],
    )?;
    for (ordinal, (page, start, end, locator, text)) in
        split_chunks(&extracted.text, extracted.pages)
            .into_iter()
            .enumerate()
    {
        let chunk_id = id();
        tx.execute(
            "INSERT INTO document_chunks VALUES(?,?,?,?,?,?,?,?,?)",
            params![
                chunk_id,
                document_id,
                ordinal as i64,
                page,
                start as i64,
                end as i64,
                locator,
                text,
                hash(&text)
            ],
        )?;
        tx.execute(
            "INSERT INTO document_fts VALUES(?,?,?)",
            params![chunk_id, document_id, text],
        )?;
    }
    let changed = tx.execute("UPDATE inbox_jobs SET status='filed',error_detail=NULL,lease_owner=NULL,lease_until=NULL,updated_at=? WHERE id=? AND status='processing' AND lease_owner=? AND lease_until > ?", params![timestamp, job_id, lease_owner, timestamp])?;
    if changed != 1 {
        bail!("processing lease was lost")
    }
    event(
        &tx,
        "document",
        &document_id,
        "file",
        serde_json::json!({"job_id":job_id,"sha256":expected}),
    )?;
    tx.commit()?;
    let warning = fs::remove_file(path).err().map(|error| error.to_string());
    Ok(
        serde_json::json!({"id":job_id,"status":"filed","document_id":document_id,"warning":warning}),
    )
}

pub(crate) async fn process(
    c: &mut Connection,
    args: InboxProcess,
    root: &Path,
) -> Result<serde_json::Value> {
    process_with_extractor(c, args, root, &SystemdPdfExtractor).await
}

pub(crate) async fn process_with_extractor(
    c: &mut Connection,
    args: InboxProcess,
    root: &Path,
    extractor: &dyn PdfExtractor,
) -> Result<serde_json::Value> {
    let job_ids = select_job_ids(c, &args.job_ids, args.limit)?;
    let mut results = Vec::new();
    for job_id in job_ids {
        let Some(job) = claim_jobs(c, std::slice::from_ref(&job_id), None)?.pop() else {
            continue;
        };
        let job_id = job.0.clone();
        let lease_owner = job.6.clone();
        match process_one(c, root, job, extractor).await {
            Ok(value) => results.push(value),
            Err(error) => {
                let detail = error.to_string();
                let tx = c.transaction()?;
                let changed = tx.execute("UPDATE inbox_jobs SET status='failed',error_detail=?,lease_owner=NULL,lease_until=NULL,updated_at=? WHERE id=? AND status='processing' AND lease_owner=? AND lease_until > ?", params![detail, now(), job_id, lease_owner, now()])?;
                if changed == 0 {
                    tx.rollback()?;
                    continue;
                }
                event(
                    &tx,
                    "inbox_job",
                    &job_id,
                    "processing_failed",
                    serde_json::json!({"detail":detail}),
                )?;
                tx.commit()?;
                results.push(serde_json::json!({"id":job_id,"status":"failed","detail":detail}));
            }
        }
    }
    let count = results.len();
    Ok(serde_json::json!({"jobs":results,"summary":{"processed":count}}))
}

fn validate_document_limit(limit: u32) -> Result<()> {
    if limit == 0 || limit > MAX_SEARCH_LIMIT {
        bail!("limit must be between 1 and {MAX_SEARCH_LIMIT}");
    }
    Ok(())
}
fn truncate_utf8(value: String, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}
fn document_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<serde_json::Value> {
    Ok(
        serde_json::json!({"id":row.get::<_,String>(0)?,"byte_hash":row.get::<_,String>(1)?,"media_type":row.get::<_,String>(2)?,"title":row.get::<_,String>(3)?,"original_basename":row.get::<_,String>(4)?,"byte_count":row.get::<_,i64>(5)?,"sensitivity":row.get::<_,String>(6)?,"archive_path":row.get::<_,String>(7)?,"derived_text_path":row.get::<_,String>(8)?,"extraction_status":row.get::<_,String>(9)?,"page_count":row.get::<_,Option<i64>>(10)?,"source_id":row.get::<_,String>(11)?,"source_revision_id":row.get::<_,String>(12)?,"created_at":row.get::<_,String>(13)?,"updated_at":row.get::<_,String>(14)?}),
    )
}

pub(crate) fn document_list(
    c: &Connection,
    status: Option<String>,
    limit: u32,
) -> Result<serde_json::Value> {
    validate_document_limit(limit)?;
    if status
        .as_deref()
        .is_some_and(|value| !["filed", "quarantined"].contains(&value))
    {
        bail!("invalid document status");
    }
    let mut sql = "SELECT id,byte_hash,media_type,title,original_basename,byte_count,sensitivity,archive_path,derived_text_path,extraction_status,page_count,source_id,source_revision_id,created_at,updated_at FROM documents".to_string();
    if status.is_some() {
        sql.push_str(" WHERE extraction_status=?");
    }
    sql.push_str(" ORDER BY created_at,id LIMIT ?");
    let mut statement = c.prepare(&sql)?;
    let rows = if let Some(value) = status {
        statement.query_map(params![value, limit], document_row)?
    } else {
        statement.query_map(params![limit], document_row)?
    };
    Ok(serde_json::json!({"documents":rows.collect::<rusqlite::Result<Vec<_>>>()?}))
}

pub(crate) fn document_show(c: &Connection, document_id: &str) -> Result<serde_json::Value> {
    document_show_with_limit(c, document_id, DEFAULT_LIMIT)
}

pub(crate) fn document_show_with_limit(
    c: &Connection,
    document_id: &str,
    limit: u32,
) -> Result<serde_json::Value> {
    Uuid::parse_str(document_id).context("invalid document id")?;
    validate_document_limit(limit)?;
    let document = c.query_row("SELECT id,byte_hash,media_type,title,original_basename,byte_count,sensitivity,archive_path,derived_text_path,extraction_status,page_count,source_id,source_revision_id,created_at,updated_at FROM documents WHERE id=?", params![document_id], document_row).optional()?.context("document not found")?;
    let mut statement = c.prepare("SELECT ordinal,page_number,start_line,end_line,locator,text,text_hash FROM document_chunks WHERE document_id=? ORDER BY ordinal LIMIT ?")?;
    let chunks = statement.query_map(params![document_id, limit], |row| {
        let text = truncate_utf8(row.get::<_, String>(5)?, MAX_CHUNK_BYTES);
        Ok(serde_json::json!({"ordinal":row.get::<_,i64>(0)?,"page_number":row.get::<_,Option<i64>>(1)?,"start_line":row.get::<_,i64>(2)?,"end_line":row.get::<_,i64>(3)?,"locator":row.get::<_,String>(4)?,"text":text,"text_hash":row.get::<_,String>(6)?}))
    })?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(serde_json::json!({"document":document,"chunks":chunks,"limit":limit}))
}

pub(crate) fn document_search(
    c: &Connection,
    query: &str,
    limit: u32,
) -> Result<serde_json::Value> {
    validate_document_limit(limit)?;
    let query = safe_fts_query(query);
    if query.is_empty() {
        return Ok(serde_json::json!({"documents":[]}));
    }
    let mut statement = c.prepare("SELECT f.document_id,d.byte_hash,d.media_type,d.title,d.original_basename,d.sensitivity,c.locator,substr(c.text,1,512) FROM document_fts f JOIN documents d ON d.id=f.document_id JOIN document_chunks c ON c.id=f.chunk_id WHERE document_fts MATCH ? ORDER BY bm25(document_fts) LIMIT ?")?;
    let rows = statement.query_map(params![query,limit], |row| Ok(serde_json::json!({"document_id":row.get::<_,String>(0)?,"byte_hash":row.get::<_,String>(1)?,"media_type":row.get::<_,String>(2)?,"title":row.get::<_,String>(3)?,"original_basename":row.get::<_,String>(4)?,"sensitivity":row.get::<_,String>(5)?,"locator":row.get::<_,String>(6)?,"snippet":row.get::<_,String>(7)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(serde_json::json!({"documents":rows}))
}
