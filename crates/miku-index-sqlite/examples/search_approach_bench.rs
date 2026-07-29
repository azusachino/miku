//! Throwaway measurement backing ADR-0020: compares real search designs
//! against the live vault. A: plain content column, raw scan. B: SQLite FTS5,
//! bulk-inserted with no per-row delete. D/F: Tolaria's actual design (walk +
//! read + `to_lowercase` per query), sequential and rayon-parallelized. E: a
//! second precomputed lower-case column + parallel scan. H: the approach
//! ADR-0020 actually adopts — single raw column, no second column, matched
//! via a zero-allocation ASCII case-fold scan, parallelized. Not part of the
//! shipped crate; run via `cargo run -p miku-index-sqlite --release --example
//! search_approach_bench -- <vault_dir>`.

use rayon::prelude::*;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::time::Instant;
use std::{env, fs, process::ExitCode};

fn walk(root: &Path, files: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        if path.is_dir() {
            walk(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "md") {
            files.push(path);
        }
    }
}

async fn open(path: &str) -> SqlitePool {
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .expect("open db")
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let Some(vault) = env::args().nth(1) else {
        eprintln!("usage: search_approach_bench -- <vault_dir>");
        return ExitCode::FAILURE;
    };
    let mut files = Vec::new();
    walk(Path::new(&vault), &mut files);
    let contents: Vec<(String, String)> = files
        .iter()
        .map(|f| {
            let path = f.to_string_lossy().to_string();
            let body = fs::read_to_string(f).unwrap_or_default();
            (path, body)
        })
        .collect();
    println!(
        "files={} total_bytes={}",
        contents.len(),
        contents.iter().map(|(_, b)| b.len()).sum::<usize>()
    );

    // --- Approach A: plain content column, one bulk transaction ---
    {
        let _ = fs::remove_file("/tmp/bench_plain.sqlite");
        let pool = open("/tmp/bench_plain.sqlite").await;
        sqlx::query("CREATE TABLE bodies (path TEXT PRIMARY KEY, body TEXT)")
            .execute(&pool)
            .await
            .unwrap();

        let started = Instant::now();
        let mut tx = pool.begin().await.unwrap();
        for (path, body) in &contents {
            sqlx::query("INSERT INTO bodies (path, body) VALUES (?, ?)")
                .bind(path)
                .bind(body)
                .execute(&mut *tx)
                .await
                .unwrap();
        }
        tx.commit().await.unwrap();
        println!(
            "[A: plain column] bulk insert one tx: {:.1} ms",
            started.elapsed().as_secs_f64() * 1000.0
        );

        for term in ["architecture", "the", "zzzzznonexistent"] {
            let started = Instant::now();
            let rows: Vec<(String,)> = sqlx::query_as("SELECT path FROM bodies")
                .fetch_all(&pool)
                .await
                .unwrap();
            let mut matches = 0usize;
            for (path,) in &rows {
                let body: (String,) = sqlx::query_as("SELECT body FROM bodies WHERE path = ?")
                    .bind(path)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
                if body.0.contains(term) {
                    matches += 1;
                }
            }
            println!(
                "[A: plain column] naive per-row scan term={:?} matches={} elapsed={:.1} ms",
                term,
                matches,
                started.elapsed().as_secs_f64() * 1000.0
            );
        }

        // Fairer version of the same approach: one query returns all bodies,
        // Rust does the substring match in-process (this is what Trilium's
        // iterateRows + JS matching actually does -- one read pass, not N).
        for term in ["architecture", "the", "zzzzznonexistent"] {
            let started = Instant::now();
            let rows: Vec<(String, String)> = sqlx::query_as("SELECT path, body FROM bodies")
                .fetch_all(&pool)
                .await
                .unwrap();
            let matches = rows.iter().filter(|(_, body)| body.contains(term)).count();
            println!(
                "[A: plain column] one-pass streamed scan term={:?} matches={} elapsed={:.1} ms",
                term,
                matches,
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
        let meta = fs::metadata("/tmp/bench_plain.sqlite").unwrap();
        println!(
            "[A: plain column] file_size_mb={:.1}",
            meta.len() as f64 / 1024.0 / 1024.0
        );
    }

    // --- Approach B: FTS5, ONE bulk transaction, no per-row delete ---
    {
        let _ = fs::remove_file("/tmp/bench_fts5.sqlite");
        let pool = open("/tmp/bench_fts5.sqlite").await;
        sqlx::query("CREATE VIRTUAL TABLE bodies_fts USING fts5(path UNINDEXED, body, tokenize='porter unicode61')")
            .execute(&pool)
            .await
            .unwrap();

        let started = Instant::now();
        let mut tx = pool.begin().await.unwrap();
        for (path, body) in &contents {
            sqlx::query("INSERT INTO bodies_fts (path, body) VALUES (?, ?)")
                .bind(path)
                .bind(body)
                .execute(&mut *tx)
                .await
                .unwrap();
        }
        tx.commit().await.unwrap();
        println!(
            "[B: FTS5] bulk insert one tx (no deletes): {:.1} ms",
            started.elapsed().as_secs_f64() * 1000.0
        );

        let started = Instant::now();
        sqlx::query("INSERT INTO bodies_fts(bodies_fts) VALUES ('optimize')")
            .execute(&pool)
            .await
            .unwrap();
        println!(
            "[B: FTS5] optimize: {:.1} ms",
            started.elapsed().as_secs_f64() * 1000.0
        );

        for term in ["architecture", "the", "zzzzznonexistent"] {
            let started = Instant::now();
            let rows: Vec<(String,)> =
                sqlx::query_as("SELECT path FROM bodies_fts WHERE bodies_fts MATCH ? LIMIT 50")
                    .bind(format!("\"{term}\""))
                    .fetch_all(&pool)
                    .await
                    .unwrap_or_default();
            println!(
                "[B: FTS5] MATCH term={:?} matches={} elapsed={:.1} ms",
                term,
                rows.len(),
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
        let meta = fs::metadata("/tmp/bench_fts5.sqlite").unwrap();
        println!(
            "[B: FTS5] file_size_mb={:.1}",
            meta.len() as f64 / 1024.0 / 1024.0
        );
    }

    // --- Approach D (Tolaria): no cache at all, walk + read files from disk
    // on every query, substring-match in Rust. Zero write-path cost since
    // there is nothing to index or store beyond the files themselves. ---
    {
        for term in ["architecture", "the", "zzzzznonexistent"] {
            let started = Instant::now();
            let matches = contents
                .iter()
                .filter(|(_, body)| body.to_lowercase().contains(&term.to_lowercase()))
                .count();
            println!(
                "[D: Tolaria filesystem scan] term={:?} matches={} elapsed={:.1} ms (files already read into `contents` above; this isolates match cost from disk I/O)",
                term, matches, started.elapsed().as_secs_f64() * 1000.0
            );
        }
        // Now the honest version: re-read every file from disk fresh, per
        // query, exactly like Tolaria's search_vault_with_options does.
        for term in ["architecture", "the", "zzzzznonexistent"] {
            let started = Instant::now();
            let mut matches = 0usize;
            for file in &files {
                let Ok(body) = fs::read_to_string(file) else {
                    continue;
                };
                if body.to_lowercase().contains(&term.to_lowercase()) {
                    matches += 1;
                }
            }
            println!(
                "[D: Tolaria filesystem scan] cold walk+read+match term={:?} matches={} elapsed={:.1} ms",
                term, matches, started.elapsed().as_secs_f64() * 1000.0
            );
        }
    }

    // --- Approach E: Tolaria's philosophy (scan at query time, no index),
    // improved: (1) content read from one SQLite column instead of N
    // filesystem syscalls, (2) content pre-lowercased once at write time
    // instead of every query, (3) the scan itself parallelized across cores. ---
    {
        let _ = fs::remove_file("/tmp/bench_plain2.sqlite");
        let pool = open("/tmp/bench_plain2.sqlite").await;
        sqlx::query("CREATE TABLE bodies (path TEXT PRIMARY KEY, body TEXT, body_lower TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        let started = Instant::now();
        let mut tx = pool.begin().await.unwrap();
        for (path, body) in &contents {
            let lower = body.to_lowercase();
            sqlx::query("INSERT INTO bodies (path, body, body_lower) VALUES (?, ?, ?)")
                .bind(path)
                .bind(body)
                .bind(lower)
                .execute(&mut *tx)
                .await
                .unwrap();
        }
        tx.commit().await.unwrap();
        println!(
            "[E: pre-lowered column] bulk insert one tx: {:.1} ms",
            started.elapsed().as_secs_f64() * 1000.0
        );

        let rows: Vec<(String, String)> = sqlx::query_as("SELECT path, body_lower FROM bodies")
            .fetch_all(&pool)
            .await
            .unwrap();

        for term in ["architecture", "the", "zzzzznonexistent"] {
            let term_lower = term.to_lowercase();
            let started = Instant::now();
            let matches = rows
                .iter()
                .filter(|(_, lower)| lower.contains(&term_lower))
                .count();
            println!(
                "[E: pre-lowered column, sequential] term={:?} matches={} elapsed={:.1} ms",
                term,
                matches,
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
        for term in ["architecture", "the", "zzzzznonexistent"] {
            let term_lower = term.to_lowercase();
            let started = Instant::now();
            let matches = rows
                .par_iter()
                .filter(|(_, lower)| lower.contains(&term_lower))
                .count();
            println!(
                "[E: pre-lowered column, rayon parallel] term={:?} matches={} elapsed={:.1} ms",
                term,
                matches,
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
        let meta = fs::metadata("/tmp/bench_plain2.sqlite").unwrap();
        println!(
            "[E: pre-lowered column] file_size_mb={:.1}",
            meta.len() as f64 / 1024.0 / 1024.0
        );
    }

    // --- Approach F: Tolaria's exact design, but parallelized (rayon over
    // the file list instead of a sequential for-loop) -- how much does just
    // adding parallelism to their own approach help, with no other changes? ---
    {
        for term in ["architecture", "the", "zzzzznonexistent"] {
            let term_lower = term.to_lowercase();
            let started = Instant::now();
            let matches = files
                .par_iter()
                .filter(|file| {
                    fs::read_to_string(file)
                        .map(|body| body.to_lowercase().contains(&term_lower))
                        .unwrap_or(false)
                })
                .count();
            println!(
                "[F: Tolaria + rayon parallel walk] term={:?} matches={} elapsed={:.1} ms",
                term,
                matches,
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
    }

    // --- Restart behavior: does reopening the SQLite file across many
    // process restarts, each with a small incremental update, degrade over
    // time (WAL growth, fragmentation) the way the naive FTS5 write path did? ---
    {
        let _ = fs::remove_file("/tmp/bench_restart.sqlite");
        let _ = fs::remove_file("/tmp/bench_restart.sqlite-wal");
        let _ = fs::remove_file("/tmp/bench_restart.sqlite-shm");
        {
            let pool = open("/tmp/bench_restart.sqlite").await;
            sqlx::query("CREATE TABLE bodies (path TEXT PRIMARY KEY, body_lower TEXT)")
                .execute(&pool)
                .await
                .unwrap();
            let started = Instant::now();
            let mut tx = pool.begin().await.unwrap();
            for (path, body) in &contents {
                sqlx::query("INSERT INTO bodies (path, body_lower) VALUES (?, ?)")
                    .bind(path)
                    .bind(body.to_lowercase())
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            tx.commit().await.unwrap();
            println!(
                "[restart cycle 0: initial cold load] {:.1} ms",
                started.elapsed().as_secs_f64() * 1000.0
            );
            pool.close().await;
        }

        // Simulate 10 process restarts, each reopening the file and updating
        // a small changed-file batch (50 pages), like a real edit session.
        for cycle in 1..=10 {
            let reopen_started = Instant::now();
            let pool = open("/tmp/bench_restart.sqlite").await;
            let reopen_ms = reopen_started.elapsed().as_secs_f64() * 1000.0;

            let query_started = Instant::now();
            let rows: Vec<(String, String)> = sqlx::query_as("SELECT path, body_lower FROM bodies")
                .fetch_all(&pool)
                .await
                .unwrap();
            let matches = rows
                .par_iter()
                .filter(|(_, lower)| lower.contains("architecture"))
                .count();
            let query_ms = query_started.elapsed().as_secs_f64() * 1000.0;

            let write_started = Instant::now();
            let mut tx = pool.begin().await.unwrap();
            for (path, body) in contents.iter().take(50) {
                let mut updated = body.to_lowercase();
                updated.push_str(&format!(" restart-cycle-{cycle}-marker"));
                sqlx::query("UPDATE bodies SET body_lower = ? WHERE path = ?")
                    .bind(updated)
                    .bind(path)
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            tx.commit().await.unwrap();
            let write_ms = write_started.elapsed().as_secs_f64() * 1000.0;
            pool.close().await;

            let meta = fs::metadata("/tmp/bench_restart.sqlite").unwrap();
            println!(
                "[restart cycle {cycle}] reopen={reopen_ms:.1}ms query(matches={matches})={query_ms:.1}ms 50-row-update={write_ms:.1}ms file_mb={:.1}",
                meta.len() as f64 / 1024.0 / 1024.0
            );
        }
        let _ = fs::remove_file("/tmp/bench_restart.sqlite");
        let _ = fs::remove_file("/tmp/bench_restart.sqlite-wal");
        let _ = fs::remove_file("/tmp/bench_restart.sqlite-shm");
    }

    // --- Approach H (the actual final design, ADR-0020): single raw `body`
    // column, no second precomputed lower-case column. Case-insensitive
    // match via a zero-allocation byte-window comparison instead of
    // `String::to_lowercase()`. Compares directly against Approach E above
    // to answer "is the second column worth it?" -- measured, not assumed. ---
    {
        fn contains_ascii_ci(haystack: &str, needle: &str) -> bool {
            let h = haystack.as_bytes();
            let n = needle.as_bytes();
            if n.is_empty() || n.len() > h.len() {
                return n.is_empty();
            }
            h.windows(n.len())
                .any(|window| window.eq_ignore_ascii_case(n))
        }

        for term in ["architecture", "Architecture", "the", "zzzzznonexistent"] {
            let started = Instant::now();
            let matches = contents
                .iter()
                .filter(|(_, body)| contains_ascii_ci(body, term))
                .count();
            println!(
                "[H: single column, inline case-fold, sequential] term={:?} matches={} elapsed={:.1} ms",
                term, matches, started.elapsed().as_secs_f64() * 1000.0
            );
        }
        for term in ["architecture", "Architecture", "the", "zzzzznonexistent"] {
            let started = Instant::now();
            let matches = contents
                .par_iter()
                .filter(|(_, body)| contains_ascii_ci(body, term))
                .count();
            println!(
                "[H: single column, inline case-fold, rayon parallel] term={:?} matches={} elapsed={:.1} ms",
                term, matches, started.elapsed().as_secs_f64() * 1000.0
            );
        }
    }

    ExitCode::SUCCESS
}
