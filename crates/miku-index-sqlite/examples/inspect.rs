//! Inspect durable SQLite projection metrics without opening the Miku server.

use sqlx::{sqlite::SqliteConnectOptions, ConnectOptions};
use std::{env, process::ExitCode};

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: cargo run -p miku-index-sqlite --example inspect -- PATH");
        return ExitCode::FAILURE;
    };

    let mut connection = match SqliteConnectOptions::new()
        .filename(&path)
        .read_only(true)
        .connect()
        .await
    {
        Ok(conn) => conn,
        Err(error) => {
            eprintln!("open SQLite database: {error}");
            return ExitCode::FAILURE;
        }
    };

    let row: (i64, i64) = match sqlx::query_as(
        "SELECT count(*), coalesce(sum(length(path) + length(title) + length(frontmatter)), 0) FROM tb_pages"
    )
    .fetch_one(&mut connection)
    .await
    {
        Ok(row) => row,
        Err(error) => {
            eprintln!("query SQLite database: {error}");
            return ExitCode::FAILURE;
        }
    };

    println!("pages={} payload_bytes={}", row.0, row.1);
    ExitCode::SUCCESS
}
