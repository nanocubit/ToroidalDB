//! Phase 12.4 — Process-level durability runner.
//!
//! Proves: every operation for which `ToroidalStore::put/batch/delete` returns
//! `Ok` survives hard process termination (SIGKILL) and is correctly recovered
//! after a fresh `reopen`.
//!
//! Architecture:
//!   Parent process → spawn child → wait for ACK ledger → SIGKILL → reopen → verify.
//!   Child writes ACKed keys to a *separate* ledger file (not the database).
//!
//! Usage:
//!   cargo run --release --bin durability -- <dir> <group_size> <n_ops> child
//!   cargo run --release --bin durability -- <dir> <group_size> <n_ops> verify

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Instant;
use toroidal_store::{BatcherOptions, ToroidalStore, ToroidalStoreOptions, WalFrameKind};

// ---------------------------------------------------------------------------
// Ledger — records which ops were ACKed by the public Store API.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct AckRecord {
    op_id: u64,
    kind: String, // "put", "delete", "batch"
    key: Vec<u8>,
    value: Option<Vec<u8>>,
}

fn read_ledger(path: &Path) -> Vec<AckRecord> {
    let content = fs::read_to_string(path).unwrap_or_default();
    let mut records = Vec::new();
    for line in content.lines() {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() < 4 {
            continue;
        }
        let op_id: u64 = parts[0].parse().unwrap_or(0);
        let kind = parts[1].to_string();
        let key = parts[2].as_bytes().to_vec();
        let value = if parts[3] == "_null_" {
            None
        } else {
            Some(parts[3].as_bytes().to_vec())
        };
        records.push(AckRecord {
            op_id,
            kind,
            key,
            value,
        });
    }
    records
}

fn write_ledger(path: &Path, rec: &AckRecord) {
    let val_str = match &rec.value {
        Some(v) => String::from_utf8_lossy(v).to_string(),
        None => "_null_".to_string(),
    };
    let line = format!(
        "{}|{}|{}|{}\n",
        rec.op_id,
        rec.kind,
        String::from_utf8_lossy(&rec.key),
        val_str
    );
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    f.write_all(line.as_bytes()).unwrap();
    f.sync_all().unwrap();
}

// ---------------------------------------------------------------------------
// Child mode
// ---------------------------------------------------------------------------

fn child_mode(dir: &Path, group_size: usize, n_ops: usize) {
    let ledger_path = dir.join("ledger");
    let _ = fs::remove_file(&ledger_path);

    let opts = ToroidalStoreOptions {
        wal_options: BatcherOptions {
            max_batch_size: group_size,
            max_wait: std::time::Duration::from_millis(1),
        },
    };
    let store = ToroidalStore::open_with_options(dir, opts).expect("child: open store");
    let mut op_id = 0u64;

    // Sequential puts
    for i in 0..n_ops {
        let key = format!("put-{:08x}", i).into_bytes();
        let val = format!("val-{:08x}", i).into_bytes();
        store.put(key.clone(), val.clone()).expect("child: put");
        op_id += 1;
        write_ledger(
            &ledger_path,
            &AckRecord {
                op_id,
                kind: "put".into(),
                key,
                value: Some(val),
            },
        );
    }

    // Some deletes
    for i in 0..n_ops / 2 {
        let key = format!("put-{:08x}", i * 2).into_bytes();
        store.delete(&key).expect("child: delete");
        op_id += 1;
        write_ledger(
            &ledger_path,
            &AckRecord {
                op_id,
                kind: "delete".into(),
                key,
                value: None,
            },
        );
    }

    // Batches
    if group_size > 1 {
        let mut ops = Vec::with_capacity(group_size);
        for i in 0..group_size {
            let key = format!("batch-{:08x}", i).into_bytes();
            ops.push(WalFrameKind::Put {
                key,
                value: b"batch".to_vec(),
            });
        }
        for i in 0..n_ops / group_size {
            store.batch(&ops).expect("child: batch");
            op_id += 1;
            write_ledger(
                &ledger_path,
                &AckRecord {
                    op_id,
                    kind: "batch".into(),
                    key: format!("batch-group-{:08x}", i).into_bytes(),
                    value: Some(b"batch".to_vec()),
                },
            );
        }
    }

    // Write READY marker and block until killed.
    let ready_path = dir.join("ready");
    fs::write(&ready_path, b"ready").unwrap();
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}

// ---------------------------------------------------------------------------
// Verify mode
// ---------------------------------------------------------------------------

fn verify_mode(dir: &Path, group_size: usize, n_ops: usize) -> i32 {
    let _ = (group_size, n_ops);

    // 1. Spawn child
    let mut child: Child = Command::new(std::env::current_exe().unwrap())
        .arg(dir)
        .arg(group_size.to_string())
        .arg(n_ops.to_string())
        .arg("child")
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn child");

    // 2. Wait for READY
    let ready_path = dir.join("ready");
    let mut waited = 0u64;
    loop {
        if ready_path.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        waited += 10;
        if waited > 60_000 {
            eprintln!("child did not become ready within 60s");
            let _ = child.kill();
            let _ = child.wait();
            return 2;
        }
    }

    // 3. Read ledger BEFORE kill
    let ledger_path = dir.join("ledger");
    let records = read_ledger(&ledger_path);
    let acked_count = records.len();

    // 4. SIGKILL (child.kill() sends SIGKILL on Unix)
    let _ = child.kill();
    let _ = child.wait();

    // 5. Fresh reopen
    let store = ToroidalStore::open(dir).expect("reopen store");

    // 6. Build expected final state from the ledger (last op per key wins).
    //    Exclude batch entries — they use synthetic keys; actual batch keys
    //    are checked separately below.
    use std::collections::BTreeMap;
    let mut expected: BTreeMap<Vec<u8>, Option<Vec<u8>>> = BTreeMap::new();
    for rec in &records {
        if rec.kind == "batch" {
            continue;
        }
        let v = match rec.kind.as_str() {
            "put" => Some(rec.value.clone().unwrap_or_default()),
            "delete" => None,
            _ => Some(rec.value.clone().unwrap_or_default()),
        };
        expected.insert(rec.key.clone(), v);
    }

    // Also verify batch keys exist (only when child wrote them).
    let mut batch_ok = 0u64;
    let mut batch_lost = 0u64;
    if group_size > 1 {
        for i in 0..group_size {
            let k = format!("batch-{:08x}", i);
            if store.get(k.as_bytes()).is_some() {
                batch_ok += 1;
            } else {
                batch_lost += 1;
                eprintln!("LOST batch key {:?}", k);
            }
        }
    }

    let mut lost = 0u64;
    let mut corrupt = 0u64;
    let mut recovered = 0u64;
    let t0 = Instant::now();

    for (key, want) in &expected {
        match store.get(key) {
            Some(found) => {
                if want.as_ref().map(|v| v.as_slice()) == Some(found.as_slice()) {
                    recovered += 1;
                } else {
                    corrupt += 1;
                    eprintln!(
                        "CORRUPT key {:?}: expected {:?}, got {:?}",
                        String::from_utf8_lossy(key),
                        want.as_ref().map(|v| String::from_utf8_lossy(v)),
                        Some(String::from_utf8_lossy(&found))
                    );
                }
            }
            None => {
                if want.is_none() {
                    recovered += 1;
                } else {
                    lost += 1;
                    eprintln!("LOST ACKed key {:?}", String::from_utf8_lossy(key));
                }
            }
        }
    }

    let recovery_ns = t0.elapsed().as_nanos() as u64;

    // 7. Post-recovery write
    store
        .put(b"post-recovery-key".to_vec(), b"post-recovery-val".to_vec())
        .unwrap();
    match store.get(b"post-recovery-key") {
        Some(v) if v == b"post-recovery-val" => {
            recovered += 1; // count as recovered
        }
        _ => {
            corrupt += 1;
            eprintln!("CORRUPT: post-recovery write failed");
        }
    }

    // 8. Summary
    let total_recovered = recovered + batch_ok;
    println!("group_size={group_size} acked={acked_count} lost={lost} corrupt={corrupt} recovered={total_recovered} batch_lost={batch_lost} recovery_ns={recovery_ns}");
    if lost > 0 || corrupt > 0 || batch_lost > 0 {
        eprintln!("FAIL: lost={lost} corrupt={corrupt} batch_lost={batch_lost}");
        1
    } else {
        println!("PASS: lost_acked=0 corrupt=0");
        0
    }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: durability <dir> <group_size> <n_ops> <child|verify>");
        std::process::exit(2);
    }
    let dir = PathBuf::from(&args[1]);
    let group_size: usize = args[2].parse().expect("group_size");
    let n_ops: usize = args[3].parse().expect("n_ops");
    match args[4].as_str() {
        "child" => child_mode(&dir, group_size, n_ops),
        "verify" => {
            let rc = verify_mode(&dir, group_size, n_ops);
            std::process::exit(rc);
        }
        _ => {
            eprintln!("mode must be child|verify");
            std::process::exit(2);
        }
    }
}
