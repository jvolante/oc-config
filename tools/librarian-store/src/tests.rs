use super::*;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};
use tempfile::TempDir;

struct FakeRunner {
    outcomes: Mutex<Vec<i32>>,
    calls: AtomicUsize,
    fail: bool,
    mutate: Option<PathBuf>,
}
#[async_trait]
impl ReviewRunner for FakeRunner {
    async fn run(&self, _script: &str, _capabilities: &[String]) -> Result<(i32, String)> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(anyhow::anyhow!("fake runner failure"));
        }
        if let Some(path) = &self.mutate {
            let db = Connection::open(path).unwrap();
            db.execute(
                "UPDATE claims SET review_after='changed' WHERE review_after='exit 0'",
                [],
            )
            .unwrap();
        }
        Ok((
            self.outcomes.lock().unwrap().pop().unwrap_or(1),
            "fake".into(),
        ))
    }
}
fn db() -> (TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let c = open(&dir.path().join("knowledge.db")).unwrap();
    (dir, c)
}

fn source(c: &mut Connection, version: Option<&str>, hash_value: Option<&str>) -> String {
    let out = source_upsert(
        c,
        SourceUpsert {
            kind: "web".into(),
            external_id: "doc".into(),
            title: "Source Title".into(),
            uri: None,
            version: version.map(str::to_owned),
            content_hash: hash_value.map(str::to_owned),
            make_current: false,
            authority: None,
            sensitivity: None,
            metadata_json: "{}".into(),
        },
    )
    .unwrap();
    out["id"].as_str().unwrap().into()
}
fn claim(c: &mut Connection, text: &str, review_kind: Option<&str>, after: Option<&str>) -> String {
    claim_add(
        c,
        ClaimAdd {
            text: text.into(),
            topics: vec!["topic-name".into()],
            confidence: "medium".into(),
            temporal_kind: "current".into(),
            valid_from: None,
            valid_until: None,
            review_kind: review_kind.map(str::to_owned),
            review_after: after.map(str::to_owned),
        },
    )
    .unwrap()["id"]
        .as_str()
        .unwrap()
        .into()
}
fn link(c: &mut Connection, cid: &str, sid: &str) {
    evidence_add(
        c,
        EvidenceArgs {
            claim: cid.into(),
            source: sid.into(),
            locator: "p1".into(),
            excerpt: "distinct excerpt".into(),
            stance: "supports".into(),
        },
    )
    .unwrap();
}

#[test]
fn revision_dedup_and_linked_staleness() {
    let (_d, mut c) = db();
    let sid = source(&mut c, Some("1"), None);
    let cid = claim(&mut c, "one", None, None);
    link(&mut c, &cid, &sid);
    source(&mut c, Some("1"), None);
    assert_eq!(
        c.query_row("SELECT count(*) FROM source_revisions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        c.query_row("SELECT count(*) FROM review_reasons", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    source(&mut c, Some("2"), None);
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM review_reasons WHERE claim_id=?",
            params![cid],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    let _other = claim(&mut c, "two", None, None);
    assert_eq!(
        c.query_row("SELECT count(*) FROM review_reasons", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
#[test]
fn revisions_reject_empty_and_numeric_regression() {
    let (_d, mut c) = db();
    assert!(source_upsert(
        &mut c,
        SourceUpsert {
            kind: "web".into(),
            external_id: "empty".into(),
            title: "Empty".into(),
            uri: None,
            version: Some("  ".into()),
            content_hash: Some(String::new()),
            make_current: false,
            authority: None,
            sensitivity: None,
            metadata_json: "{}".into(),
        }
    )
    .is_err());
    source(&mut c, Some("2"), None);
    assert!(source_upsert(
        &mut c,
        SourceUpsert {
            kind: "web".into(),
            external_id: "doc".into(),
            title: "Source Title".into(),
            uri: None,
            version: Some("1".into()),
            content_hash: None,
            make_current: false,
            authority: None,
            sensitivity: None,
            metadata_json: "{}".into(),
        }
    )
    .is_err());
    assert_eq!(
        c.query_row(
            "SELECT sr.version FROM sources s JOIN source_revisions sr ON sr.id=s.current_revision_id",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "2"
    );
}
#[test]
fn unordered_revision_requires_explicit_current_assertion() {
    let (_d, mut c) = db();
    let make = |version: &str, make_current: bool| SourceUpsert {
        kind: "web".into(),
        external_id: "tagged".into(),
        title: "Tagged Source".into(),
        uri: None,
        version: Some(version.into()),
        content_hash: None,
        make_current,
        authority: None,
        sensitivity: None,
        metadata_json: "{}".into(),
    };
    source_upsert(&mut c, make("v2", false)).unwrap();
    assert!(source_upsert(&mut c, make("v1", false)).is_err());
    source_upsert(&mut c, make("v1", true)).unwrap();
    assert_eq!(
        c.query_row(
            "SELECT sr.version FROM sources s JOIN source_revisions sr ON sr.id=s.current_revision_id WHERE s.external_id='tagged'",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "v1"
    );
}
#[test]
fn claim_and_review_boundaries() {
    let (_d, mut c) = db();
    claim(&mut c, " Hello   WORLD ", None, None);
    assert!(claim_add(
        &mut c,
        ClaimAdd {
            text: "hello world".into(),
            topics: vec![],
            confidence: "medium".into(),
            temporal_kind: "current".into(),
            valid_from: None,
            valid_until: None,
            review_kind: None,
            review_after: None
        }
    )
    .is_err());
    assert!(claim_add(
        &mut c,
        ClaimAdd {
            text: "empty".into(),
            topics: vec![],
            confidence: "medium".into(),
            temporal_kind: "current".into(),
            valid_from: None,
            valid_until: None,
            review_kind: Some("script".into()),
            review_after: Some(String::new())
        }
    )
    .is_err());
    let exact = "λ".repeat(512);
    assert!(claim_add(
        &mut c,
        ClaimAdd {
            text: "unicode".into(),
            topics: vec![],
            confidence: "medium".into(),
            temporal_kind: "current".into(),
            valid_from: None,
            valid_until: None,
            review_kind: Some("script".into()),
            review_after: Some(exact)
        }
    )
    .is_ok());
    assert!(claim_add(
        &mut c,
        ClaimAdd {
            text: "too long".into(),
            topics: vec![],
            confidence: "medium".into(),
            temporal_kind: "current".into(),
            valid_from: None,
            valid_until: None,
            review_kind: Some("script".into()),
            review_after: Some("λ".repeat(513))
        }
    )
    .is_err());
}
#[test]
fn evidence_needs_exact_source_revision() {
    let (_d, mut c) = db();
    let sid = source(&mut c, None, Some("h"));
    let cid = claim(&mut c, "claim", None, None);
    assert!(evidence_add(
        &mut c,
        EvidenceArgs {
            claim: cid.clone(),
            source: "not-the-uuid".into(),
            locator: "x".into(),
            excerpt: "e".into(),
            stance: "supports".into()
        }
    )
    .is_err());
    let revisionless = id();
    let timestamp = now();
    c.execute(
        "INSERT INTO sources(id,kind,external_id,title,metadata_json,created_at,updated_at) VALUES(?,?,?,?,?,?,?)",
        params![revisionless, "web", "no-revision", "No Revision", "{}", timestamp, timestamp],
    )
    .unwrap();
    assert!(evidence_add(
        &mut c,
        EvidenceArgs {
            claim: cid.clone(),
            source: revisionless,
            locator: "x".into(),
            excerpt: "e".into(),
            stance: "supports".into()
        }
    )
    .is_err());
    link(&mut c, &cid, &sid);
    assert_eq!(
        c.query_row("SELECT count(*) FROM claim_evidence", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn approval_preview_canonicalizes_and_binds_capabilities() {
    let (_d, mut c) = db();
    let cid = claim(&mut c, "approval", Some("script"), Some("exit 1"));
    let preview = review_approval(
        &c,
        ReviewApprovalArgs {
            id: cid.clone(),
            capabilities: vec!["gh-auth".into(), "gh-auth".into()],
        },
    )
    .unwrap();
    assert_eq!(
        preview["capabilities"],
        serde_json::json!(["gh-auth", "network"])
    );
    assert_eq!(
        preview["approval_hash"],
        approval_identity("exit 1", &["gh-auth".into(), "network".into()])
    );
    assert!(review_approval(
        &c,
        ReviewApprovalArgs {
            id: cid,
            capabilities: vec!["home".into()],
        },
    )
    .is_err());
}

#[test]
fn current_supporting_evidence_revalidates_source_reason() {
    let (_d, mut c) = db();
    let sid = source(&mut c, Some("1"), None);
    let cid = claim(&mut c, "revalidate", None, None);
    link(&mut c, &cid, &sid);
    source(&mut c, Some("2"), None);
    assert_eq!(reasons(&c, &cid).unwrap().len(), 1);
    link(&mut c, &cid, &sid);
    assert!(reasons(&c, &cid).unwrap().is_empty());
    assert_eq!(
        c.query_row(
            "SELECT status FROM claims WHERE id=?",
            params![cid],
            |row| { row.get::<_, String>(0) }
        )
        .unwrap(),
        "verified"
    );
}
#[tokio::test]
async fn fts_retrieves_claim_from_topic_source_and_excerpt() {
    let (_d, mut c) = db();
    let sid = source(&mut c, Some("1"), None);
    let cid = claim(&mut c, "claim text", None, None);
    link(&mut c, &cid, &sid);
    for query in ["claim", "topic-name", "Source Title", "distinct excerpt"] {
        let result = present(&mut c, query, 10, "search").await.unwrap();
        assert_eq!(result["claims"][0]["id"], cid, "{query}: {result}");
    }
}
#[tokio::test]
async fn review_states_are_safe_and_latched() {
    let (_d, mut c) = db();
    let cid = claim(&mut c, "scripted", Some("script"), Some("exit 1"));
    let caps = vec!["network".into()];
    let identity = approval_identity("exit 1", &caps);
    approve(
        &mut c,
        ApproveArgs {
            id: cid.clone(),
            hash: identity,
            capabilities: caps,
        },
    )
    .unwrap();
    let fake = Arc::new(FakeRunner {
        fail: false,
        mutate: None,
        outcomes: Mutex::new(vec![1]),
        calls: AtomicUsize::new(0),
    });
    assert_eq!(
        present_id_with_runner(&mut c, &cid, fake.clone())
            .await
            .unwrap()["freshness"],
        "current"
    );
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    let cid2 = claim(&mut c, "fires", Some("script"), Some("exit 0"));
    let caps = vec![];
    let identity = approval_identity("exit 0", &caps);
    approve(
        &mut c,
        ApproveArgs {
            id: cid2.clone(),
            hash: identity,
            capabilities: caps,
        },
    )
    .unwrap();
    let fired_runner = Arc::new(FakeRunner {
        fail: false,
        mutate: None,
        outcomes: Mutex::new(vec![0]),
        calls: AtomicUsize::new(0),
    });
    assert_eq!(
        present_id_with_runner(&mut c, &cid2, fired_runner.clone())
            .await
            .unwrap()["freshness"],
        "needs_review"
    );
    assert_eq!(fired_runner.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        present_id_with_runner(&mut c, &cid2, fired_runner.clone())
            .await
            .unwrap()["freshness"],
        "needs_review"
    );
    assert_eq!(fired_runner.calls.load(Ordering::SeqCst), 1);
    review(&mut c, IdArg { id: cid2.clone() }).unwrap();
    assert_eq!(
        present_id_with_runner(&mut c, &cid2, fired_runner.clone())
            .await
            .unwrap()["freshness"],
        "current"
    );
    assert_eq!(fired_runner.calls.load(Ordering::SeqCst), 1);
    assert!(review(
        &mut c,
        IdArg {
            id: "missing".into()
        }
    )
    .is_err());
}
#[tokio::test]
async fn unapproved_and_condition_are_unknown_and_retired_is_not_run() {
    let (_d, mut c) = db();
    let cid = claim(&mut c, "unapproved", Some("script"), Some("exit 0"));
    let fake = Arc::new(FakeRunner {
        fail: false,
        mutate: None,
        outcomes: Mutex::new(vec![0]),
        calls: AtomicUsize::new(0),
    });
    run_reviews_with_runner(&mut c, &[cid.clone()], fake.clone())
        .await
        .unwrap();
    assert_eq!(
        present_id(&mut c, &cid).await.unwrap()["freshness"],
        "unknown"
    );
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    let condition = claim(&mut c, "condition", Some("condition"), Some("later"));
    assert_eq!(
        present_id(&mut c, &condition).await.unwrap()["freshness"],
        "unknown"
    );
    assert!(review(
        &mut c,
        IdArg {
            id: condition.clone()
        }
    )
    .is_err());
    assert_eq!(
        present_id(&mut c, &condition).await.unwrap()["freshness"],
        "unknown"
    );
}
#[tokio::test]
async fn runner_errors_do_not_hide_other_claims_and_false_keeps_reasons() {
    let (_d, mut c) = db();
    let sid = source(&mut c, Some("1"), None);
    let first = claim(&mut c, "first", Some("script"), Some("exit 1"));
    let second = claim(&mut c, "second", Some("script"), Some("exit 1"));
    link(&mut c, &first, &sid);
    source(&mut c, Some("2"), None);
    let caps = vec![];
    for cid in [&first, &second] {
        let script = "exit 1";
        approve(
            &mut c,
            ApproveArgs {
                id: cid.clone(),
                hash: approval_identity(script, &caps),
                capabilities: caps.clone(),
            },
        )
        .unwrap();
    }
    let runner = Arc::new(FakeRunner {
        fail: true,
        mutate: None,
        outcomes: Mutex::new(vec![]),
        calls: AtomicUsize::new(0),
    });
    assert_eq!(
        present_id_with_runner(&mut c, &first, runner.clone())
            .await
            .unwrap()["freshness"],
        "needs_review"
    );
    assert_eq!(
        present_id_with_runner(&mut c, &second, runner)
            .await
            .unwrap()["freshness"],
        "unknown"
    );
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM review_reasons WHERE claim_id=? AND kind='source_changed'",
            params![first],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    let expired = claim_add(
        &mut c,
        ClaimAdd {
            text: "expired".into(),
            topics: vec![],
            confidence: "high".into(),
            temporal_kind: "current".into(),
            valid_from: None,
            valid_until: Some("2000-01-01T00:00:00Z".into()),
            review_kind: None,
            review_after: None,
        },
    )
    .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        present_id(&mut c, &expired).await.unwrap()["freshness"],
        "needs_review"
    );
    assert!(c
        .query_row(
            "SELECT 1 FROM review_reasons WHERE claim_id=? AND kind='expiration'",
            params![expired],
            |r| r.get::<_, i64>(0)
        )
        .is_ok());
}
#[tokio::test]
async fn post_await_identity_change_is_unknown() {
    let (dir, mut c) = db();
    let cid = claim(&mut c, "identity", Some("script"), Some("exit 0"));
    approve(
        &mut c,
        ApproveArgs {
            id: cid.clone(),
            hash: approval_identity("exit 0", &[]),
            capabilities: vec![],
        },
    )
    .unwrap();
    let runner = Arc::new(FakeRunner {
        fail: false,
        mutate: Some(dir.path().join("knowledge.db")),
        outcomes: Mutex::new(vec![0]),
        calls: AtomicUsize::new(0),
    });
    run_reviews_with_runner(&mut c, &[cid.clone()], runner)
        .await
        .unwrap();
    assert_eq!(
        c.query_row(
            "SELECT last_review_result FROM claims WHERE id=?",
            params![cid],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "unknown"
    );
}
#[test]
fn review_job_is_atomically_claimed() {
    let (dir, mut first) = db();
    let cid = claim(&mut first, "leased", Some("script"), Some("exit 1"));
    approve(
        &mut first,
        ApproveArgs {
            id: cid.clone(),
            hash: approval_identity("exit 1", &[]),
            capabilities: vec![],
        },
    )
    .unwrap();
    let first_jobs = claim_review_jobs(&mut first, std::slice::from_ref(&cid)).unwrap();
    assert_eq!(first_jobs.len(), 1);
    let mut second = open(&dir.path().join("knowledge.db")).unwrap();
    let second_jobs = claim_review_jobs(&mut second, std::slice::from_ref(&cid)).unwrap();
    assert!(second_jobs.is_empty());
    assert_eq!(
        second
            .query_row(
                "SELECT trigger_state FROM claims WHERE id=?",
                params![cid],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "executing"
    );
}
#[test]
fn relations_and_expiration_reasons() {
    let (_d, mut c) = db();
    let a = claim(&mut c, "a", None, None);
    let b = claim(&mut c, "b", None, None);
    relate(
        &mut c,
        RelateArgs {
            from: a.clone(),
            relation: "contradicts".into(),
            to: b.clone(),
        },
    )
    .unwrap();
    assert_eq!(
        c.query_row("SELECT status FROM claims WHERE id=?", params![a], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "disputed"
    );
    assert!(c
        .query_row(
            "SELECT reason_key FROM review_reasons WHERE claim_id=?",
            params![a],
            |r| r.get::<_, String>(0)
        )
        .is_ok_and(|reason| reason == b));
    relate(
        &mut c,
        RelateArgs {
            from: a.clone(),
            relation: "supersedes".into(),
            to: b.clone(),
        },
    )
    .unwrap();
    assert_eq!(
        c.query_row("SELECT status FROM claims WHERE id=?", params![b], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "superseded"
    );
    assert!(relate(
        &mut c,
        RelateArgs {
            from: "x".into(),
            relation: "supersedes".into(),
            to: b
        }
    )
    .is_err());
}
#[tokio::test]
async fn export_and_backup_contain_claim() {
    let (d, mut c) = db();
    let cid = claim(&mut c, "exported", None, None);
    let out = PathBuf::from("out.jsonl");
    futures_export(&mut c, &out).await;
    assert!(fs::read_to_string(d.path().join("exports/out.jsonl"))
        .unwrap()
        .contains(&cid));
    let backup_path = PathBuf::from("backup.db");
    backup(&c, backup_path.clone()).unwrap();
    let copy = Connection::open(d.path().join("backups/backup.db")).unwrap();
    assert_eq!(
        copy.query_row("SELECT count(*) FROM claims", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn managed_outputs_reject_external_paths_and_symlinks() {
    let (d, c) = db();
    assert!(managed_output_path(&c, Path::new("/tmp/out.jsonl"), "exports").is_err());
    let exports = d.path().join("exports");
    fs::create_dir_all(&exports).unwrap();
    std::os::unix::fs::symlink("/tmp/outside", exports.join("link")).unwrap();
    assert!(managed_output_path(&c, Path::new("link"), "exports").is_err());
    assert!(managed_output_path(&c, Path::new("nested/out"), "exports").is_err());
}
async fn futures_export(c: &mut Connection, path: &Path) {
    export(
        c,
        ExportArgs {
            format: ExportFormat::Jsonl,
            output: path.to_path_buf(),
        },
    )
    .await
    .unwrap();
}

#[test]
fn init_creates_private_inbox_layout() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested/knowledge.db");
    let c = open(&path).unwrap();
    ensure_layout(&c).unwrap();
    for relative in ["inbox", "archive/objects", "archive/text", "quarantine"] {
        assert!(path.parent().unwrap().join(relative).is_dir());
    }
}

#[test]
fn inbox_add_is_copy_only_and_status_ignores_hidden_files() {
    let (dir, mut c) = db();
    ensure_layout(&c).unwrap();
    let source = dir.path().join("source.txt");
    fs::write(&source, b"hello").unwrap();
    let original = fs::read(&source).unwrap();
    let result = inbox_add(
        &mut c,
        InboxAdd {
            sensitivity: "internal".into(),
            files: vec![source.clone()],
        },
        &dir.path().join("inbox"),
    )
    .unwrap();
    assert_eq!(fs::read(&source).unwrap(), original);
    let published = dir.path().join(result["jobs"][0]["path"].as_str().unwrap());
    assert_eq!(fs::read(published).unwrap(), original);
    fs::write(dir.path().join("inbox/.orphan.tmp"), b"ignored").unwrap();
    let status = inbox_status(&c).unwrap();
    assert_eq!(status["jobs"].as_array().unwrap().len(), 1);
    assert_eq!(status["counts"]["pending"], 1);
}

#[test]
fn inbox_duplicate_reuses_job_and_published_file() {
    let (dir, mut c) = db();
    ensure_layout(&c).unwrap();
    let first = dir.path().join("one");
    let second = dir.path().join("two");
    fs::write(&first, b"same").unwrap();
    fs::write(&second, b"same").unwrap();
    let args = |file| InboxAdd {
        sensitivity: "internal".into(),
        files: vec![file],
    };
    let one = inbox_add(&mut c, args(first), &dir.path().join("inbox")).unwrap();
    let two = inbox_add(&mut c, args(second), &dir.path().join("inbox")).unwrap();
    assert_eq!(one["jobs"][0]["id"], two["jobs"][0]["id"]);
    assert_eq!(two["jobs"][0]["status"], "duplicate");
    assert_eq!(fs::read_dir(dir.path().join("inbox")).unwrap().count(), 1);
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM events WHERE operation='inbox_add'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
}

#[cfg(unix)]
#[test]
fn inbox_rejects_empty_and_symlink_and_non_file_inputs() {
    let (dir, mut c) = db();
    ensure_layout(&c).unwrap();
    let empty = dir.path().join("empty");
    let target = dir.path().join("target");
    let link = dir.path().join("link");
    fs::write(&empty, []).unwrap();
    fs::write(&target, b"target").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    for path in [empty, link, dir.path().to_path_buf()] {
        assert!(inbox_add(
            &mut c,
            InboxAdd {
                sensitivity: "internal".into(),
                files: vec![path]
            },
            &dir.path().join("inbox")
        )
        .is_err());
    }
}

#[test]
fn inbox_sanitizes_suspicious_basename_and_validates_sensitivity() {
    let (dir, mut c) = db();
    ensure_layout(&c).unwrap();
    let source = dir.path().join("..\n evil");
    fs::write(&source, b"safe").unwrap();
    assert!(inbox_add(
        &mut c,
        InboxAdd {
            sensitivity: "secret".into(),
            files: vec![source.clone()]
        },
        &dir.path().join("inbox")
    )
    .is_err());
    let result = inbox_add(
        &mut c,
        InboxAdd {
            sensitivity: "restricted".into(),
            files: vec![source],
        },
        &dir.path().join("inbox"),
    )
    .unwrap();
    let path = result["jobs"][0]["path"].as_str().unwrap();
    assert!(path.starts_with("inbox/job-"));
    assert!(!path.contains(".."));
}

#[test]
fn old_schema_errors_without_deleting_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("knowledge.db");
    let old = Connection::open(&path).unwrap();
    old.execute_batch("CREATE TABLE schema_version(version INTEGER NOT NULL); INSERT INTO schema_version VALUES(2); CREATE TABLE marker(value TEXT); INSERT INTO marker VALUES('keep');").unwrap();
    drop(old);
    let error = match open(&path) {
        Ok(_) => panic!("old schema unexpectedly opened"),
        Err(error) => error.to_string(),
    };
    assert!(error.contains("back up it, delete it, and run init"));
    let check = Connection::open(&path).unwrap();
    assert_eq!(
        check
            .query_row("SELECT value FROM marker", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "keep"
    );
}

#[test]
fn existing_database_without_schema_version_is_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("knowledge.db");
    let old = Connection::open(&path).unwrap();
    old.execute_batch("CREATE TABLE marker(value TEXT); INSERT INTO marker VALUES('keep');")
        .unwrap();
    drop(old);
    assert!(open(&path).is_err());
    let check = Connection::open(&path).unwrap();
    assert_eq!(
        check
            .query_row("SELECT value FROM marker", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "keep"
    );
    assert!(check
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE name='schema_version'",
            [],
            |_| Ok(())
        )
        .is_err());
}

#[tokio::test]
async fn zero_byte_database_is_initialized_by_init_and_first_inbox_add() {
    let init_dir = tempfile::tempdir().unwrap();
    let init_path = init_dir.path().join("knowledge.db");
    fs::File::create(&init_path).unwrap();
    execute_at(
        Cli::try_parse_from(["librarian-store", "init"]).unwrap(),
        init_path.clone(),
    )
    .await
    .unwrap();
    let initialized = Connection::open(&init_path).unwrap();
    assert!(initialized
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE name='schema_version'",
            [],
            |_| Ok(())
        )
        .is_ok());

    let inbox_dir = tempfile::tempdir().unwrap();
    let inbox_path = inbox_dir.path().join("knowledge.db");
    fs::File::create(&inbox_path).unwrap();
    let source = inbox_dir.path().join("first-use.txt");
    fs::write(&source, b"first use").unwrap();
    let result = execute_at(
        Cli::try_parse_from(["librarian-store", "inbox", "add", source.to_str().unwrap()]).unwrap(),
        inbox_path.clone(),
    )
    .await
    .unwrap();
    assert_eq!(result["jobs"][0]["status"], "pending");
    assert!(inbox_path.parent().unwrap().join("inbox").is_dir());
}

#[test]
fn data_home_accepts_only_nonempty_absolute_xdg_path() {
    assert_eq!(
        data_home(
            Some(std::ffi::OsStr::new("/xdg")),
            Some(std::ffi::OsStr::new("/home/user"))
        )
        .unwrap(),
        PathBuf::from("/xdg")
    );
    assert_eq!(
        data_home(
            Some(std::ffi::OsStr::new("relative")),
            Some(std::ffi::OsStr::new("/home/user"))
        )
        .unwrap(),
        PathBuf::from("/home/user/.local/share")
    );
    assert_eq!(
        data_home(
            Some(std::ffi::OsStr::new("")),
            Some(std::ffi::OsStr::new("/home/user"))
        )
        .unwrap(),
        PathBuf::from("/home/user/.local/share")
    );
    assert!(data_home(
        Some(std::ffi::OsStr::new("relative")),
        Some(std::ffi::OsStr::new("relative-home"))
    )
    .is_err());
}

#[test]
fn inbox_add_initializes_layout_before_copying() {
    let (dir, mut c) = db();
    let source = dir.path().join("first-use.txt");
    fs::write(&source, b"first use").unwrap();
    let result = inbox_add(
        &mut c,
        InboxAdd {
            sensitivity: "internal".into(),
            files: vec![source],
        },
        &dir.path().join("inbox"),
    )
    .unwrap();
    assert_eq!(result["jobs"][0]["status"], "pending");
    assert!(dir.path().join("inbox").is_dir());
}

#[test]
fn published_file_is_removed_when_database_write_fails() {
    let (dir, mut c) = db();
    let source = dir.path().join("failure.txt");
    fs::write(&source, b"failure").unwrap();
    let error = inbox_add_internal(
        &mut c,
        InboxAdd {
            sensitivity: "internal".into(),
            files: vec![source],
        },
        &dir.path().join("inbox"),
        true,
    )
    .unwrap_err();
    assert!(error.to_string().contains("forced inbox database failure"));
    assert_eq!(fs::read_dir(dir.path().join("inbox")).unwrap().count(), 0);
    assert_eq!(
        c.query_row("SELECT count(*) FROM inbox_jobs", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn invalid_duplicate_targets_are_failed_and_new_job_is_retained() {
    for corrupt in [false, true] {
        let (dir, mut c) = db();
        let first = dir.path().join("first");
        let second = dir.path().join("second");
        fs::write(&first, b"same").unwrap();
        fs::write(&second, b"same").unwrap();
        let first_result = inbox_add(
            &mut c,
            InboxAdd {
                sensitivity: "internal".into(),
                files: vec![first],
            },
            &dir.path().join("inbox"),
        )
        .unwrap();
        let published = dir
            .path()
            .join(first_result["jobs"][0]["path"].as_str().unwrap());
        if corrupt {
            fs::write(&published, b"corrupt").unwrap();
        } else {
            fs::remove_file(&published).unwrap();
        }
        let second_result = inbox_add(
            &mut c,
            InboxAdd {
                sensitivity: "internal".into(),
                files: vec![second],
            },
            &dir.path().join("inbox"),
        )
        .unwrap();
        assert_eq!(second_result["jobs"][0]["status"], "pending");
        assert_eq!(
            c.query_row(
                "SELECT status FROM inbox_jobs ORDER BY created_at LIMIT 1",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "failed"
        );
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM events WHERE operation='inbox_validation_failed'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }
}

#[test]
fn inbox_size_limit_includes_exact_boundary_only() {
    assert!(validate_inbox_size(MAX_INBOX_INPUT_BYTES).is_ok());
    assert!(validate_inbox_size(MAX_INBOX_INPUT_BYTES + 1).is_err());
}

#[tokio::test]
async fn cli_returns_json_envelope_and_rejects_invalid_input() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("knowledge.db");
    let success = Cli::try_parse_from(["librarian-store", "init"]).unwrap();
    let envelope = output_envelope(Ok(execute_at(success, db_path).await.unwrap()));
    assert_eq!(envelope["ok"], true);
    assert!(envelope["data"]["path"].is_string());

    let invalid = Cli::try_parse_from(["librarian-store", "inbox", "add"]);
    let error = match invalid {
        Ok(_) => panic!("invalid CLI input unexpectedly parsed"),
        Err(error) => error,
    };
    assert_eq!(error.exit_code(), 2);
}
