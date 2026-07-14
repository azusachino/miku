//! Inspect durable Turso projection metrics without opening the Miku server.

use std::{env, process::ExitCode};
use turso::Builder;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: cargo run -p miku-index-turso --example inspect -- PATH");
        return ExitCode::FAILURE;
    };

    let database = match Builder::new_local(&path)
        .experimental_index_method(true)
        .build()
        .await
    {
        Ok(database) => database,
        Err(error) => {
            eprintln!("open Turso database: {error}");
            return ExitCode::FAILURE;
        }
    };
    let connection = match database.connect() {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("connect Turso database: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut rows = match connection
        .query(
            "SELECT count(*), coalesce(sum(length(payload)), 0) FROM miku_page_index",
            (),
        )
        .await
    {
        Ok(rows) => rows,
        Err(error) => {
            eprintln!("query Turso database: {error}");
            return ExitCode::FAILURE;
        }
    };
    let Some(row) = (match rows.next().await {
        Ok(row) => row,
        Err(error) => {
            eprintln!("read Turso result: {error}");
            return ExitCode::FAILURE;
        }
    }) else {
        eprintln!("Turso returned no aggregate row");
        return ExitCode::FAILURE;
    };
    let Some(page_count) = row
        .get_value(0)
        .ok()
        .and_then(|value| value.as_integer().copied())
    else {
        eprintln!("Turso returned an invalid page count");
        return ExitCode::FAILURE;
    };
    let Some(payload_bytes) = row
        .get_value(1)
        .ok()
        .and_then(|value| value.as_integer().copied())
    else {
        eprintln!("Turso returned an invalid payload size");
        return ExitCode::FAILURE;
    };
    println!("pages={page_count} payload_bytes={payload_bytes}");
    ExitCode::SUCCESS
}
