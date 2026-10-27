use lift_db::api::*;
use std::time::{Duration, Instant};

fn contract(driver: &str, url: &str) {
    let pool = create_pool(driver, url, 1, 1, 1000).unwrap();
    execute(&pool, "DROP TABLE IF EXISTS lift_adapter_contract", &[]).unwrap();
    execute(&pool,"CREATE TABLE lift_adapter_contract (id BIGINT PRIMARY KEY, text_value TEXT, flag BOOLEAN, amount DOUBLE PRECISION, absent TEXT)",&[]).unwrap();
    let mut tx = begin(&pool).unwrap();
    let start = Instant::now();
    assert!(query(&pool, "SELECT 1", &[]).is_err());
    assert!(start.elapsed() >= Duration::from_millis(800));
    let parameters = [
        value_int(1),
        value_text("' ; DROP TABLE lift_adapter_contract; -- 🙂"),
        value_bool(true),
        value_number(2.5),
        value_null(),
    ];
    assert_eq!(
        tx_execute(
            &mut tx,
            "INSERT INTO lift_adapter_contract VALUES ($1,$2,$3,$4,$5)",
            &parameters
        )
        .unwrap(),
        1
    );
    commit(tx).unwrap();
    let rows = query(
        &pool,
        "SELECT id,text_value,flag,amount,absent FROM lift_adapter_contract",
        &[],
    )
    .unwrap();
    assert_eq!(row_count(&rows), 1);
    assert_eq!(column_count(&rows), 5);
    assert_eq!(get_int(&rows, 0, 0).unwrap(), 1);
    assert_eq!(
        get_text(&rows, 0, 1).unwrap(),
        "' ; DROP TABLE lift_adapter_contract; -- 🙂"
    );
    assert!(get_bool(&rows, 0, 2).unwrap());
    assert_eq!(get_number(&rows, 0, 3).unwrap(), 2.5);
    assert!(is_null(&rows, 0, 4).unwrap());
    assert!(get_text(&rows, 0, 4).is_err());
    assert!(get_int(&rows, -1, 0).is_err());
    assert!(get_int(&rows, 1, 0).is_err());
    // Materialized rows do not keep a lease: max_size=1 still permits begin.
    let mut tx = begin(&pool).unwrap();
    tx_execute(
        &mut tx,
        "INSERT INTO lift_adapter_contract (id) VALUES ($1)",
        &[value_int(2)],
    )
    .unwrap();
    rollback(tx).unwrap();
    {
        let mut tx = begin(&pool).unwrap();
        tx_execute(
            &mut tx,
            "INSERT INTO lift_adapter_contract (id) VALUES ($1)",
            &[value_int(3)],
        )
        .unwrap();
    }
    let mut failed = begin(&pool).unwrap();
    tx_execute(
        &mut failed,
        "INSERT INTO lift_adapter_contract (id) VALUES ($1)",
        &[value_int(4)],
    )
    .unwrap();
    assert!(tx_execute(
        &mut failed,
        "INSERT INTO lift_adapter_contract (id) VALUES ($1)",
        &[value_int(1)]
    )
    .is_err());
    assert!(commit(failed).is_err());
    assert_eq!(
        row_count(&query(&pool, "SELECT id FROM lift_adapter_contract", &[]).unwrap()),
        1
    );
    assert!(execute(&pool, "BEGIN", &[]).is_err());
    assert!(execute(&pool, "SELECT 1; DELETE FROM lift_adapter_contract", &[]).is_err());
    let order = query(
        &pool,
        "SELECT CAST($2 AS TEXT), CAST($1 AS TEXT)",
        &[value_text("first"), value_text("second")],
    )
    .unwrap();
    assert_eq!(get_text(&order, 0, 0).unwrap(), "second");
    assert_eq!(get_text(&order, 0, 1).unwrap(), "first");
    assert!(query(
        &pool,
        "SELECT CAST($1 AS DOUBLE PRECISION)",
        &[value_number(f64::INFINITY)]
    )
    .is_err());
    execute(&pool, "DROP TABLE lift_adapter_contract", &[]).unwrap();
}

#[test]
fn sqlite_general_api_contract() {
    contract("sqlite", "sqlite::memory:");
}

#[test]
fn pool_configuration_and_tls_policy_are_explicit() {
    assert!(create_pool("sqlite", "sqlite::memory:", 1, 2, 100).is_err());
    assert!(create_pool("sqlite", "sqlite::memory:", 2, 1, 100).is_err());
    assert!(create_pool("sqlite", "sqlite::memory:", 0, 0, 100).is_err());
    assert!(create_pool("sqlite", "sqlite::memory:", 0, 1, 0).is_err());
    assert!(create_pool(
        "postgres",
        "postgres://user:secret@127.0.0.1/database",
        0,
        1,
        100
    )
    .is_err());
}

#[test]
#[ignore = "requires a real PostgreSQL server and LIFT_TEST_POSTGRES_URL; CI runs it explicitly"]
fn postgres_general_api_contract() {
    let url = std::env::var("LIFT_TEST_POSTGRES_URL")
        .expect("LIFT_TEST_POSTGRES_URL must point to a dedicated PostgreSQL test database");
    contract("postgres", &url);
    // Kill this transaction's backend: rollback must fail and its connection
    // must be discarded; a new acquisition must establish a usable connection.
    let pool = create_pool("postgres", &url, 1, 1, 3000).unwrap();
    let mut tx = begin(&pool).unwrap();
    assert!(tx_query(
        &mut tx,
        "SELECT pg_terminate_backend(pg_backend_pid())",
        &[]
    )
    .is_err());
    drop(tx);
    let rows = query(&pool, "SELECT 1::bigint", &[]).unwrap();
    assert_eq!(get_int(&rows, 0, 0).unwrap(), 1);
}
