mod common;

use hecate_infrastructure::store;

type SchemaRow = (String, String, String, Option<String>);

fn python_schema() -> Vec<SchemaRow> {
    let text = std::fs::read_to_string(common::vectors_dir().join("schema.sql")).unwrap();
    text.split("-- object: ")
        .skip(1)
        .map(|block| {
            let (header, body) = block.split_once('\n').unwrap();
            let mut parts = header.split(' ');
            let kind = parts.next().unwrap().to_string();
            let name = parts.next().unwrap().to_string();
            let table = parts.next().unwrap().to_string();
            let body = body.trim_end();
            let sql = (body != "-- no sql").then(|| body.strip_suffix(';').unwrap().to_string());
            (kind, name, table, sql)
        })
        .collect()
}

const CA_MATERIAL: &str = "CREATE TABLE ca_material (
    generation INTEGER NOT NULL,
    intermediate_version INTEGER NOT NULL,
    root_certificate BLOB NOT NULL,
    intermediate_certificate BLOB NOT NULL,
    key_nonce BLOB NOT NULL,
    key_ciphertext BLOB NOT NULL,
    PRIMARY KEY (generation, intermediate_version)
)";

const HECATE_TLS: &str = "CREATE TABLE hecate_tls (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    serial TEXT NOT NULL,
    private_key BLOB NOT NULL
)";

const ENROLL_ATTEMPTS: &str = "CREATE TABLE enroll_attempts (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    consecutive_failures INTEGER NOT NULL,
    wait_until TEXT,
    rejected_during_wait INTEGER NOT NULL
)";

/// The schema of the change 005 (the vector, which does not change) with what the identities add
/// (research R10): two access log columns and three tables.
fn schema_with_identities() -> Vec<SchemaRow> {
    let mut rows = python_schema();
    let access_log = rows
        .iter_mut()
        .find(|row| row.0 == "table" && row.1 == "access_log")
        .unwrap();
    let sql = access_log.3.take().unwrap();
    assert!(sql.ends_with("    version INTEGER\n)"), "{sql}");
    access_log.3 = Some(sql.replace(
        "    version INTEGER\n)",
        "    version INTEGER,\n    identity TEXT,\n    certificate TEXT\n)",
    ));
    let row = |kind: &str, name: &str, table: &str, sql: Option<&str>| {
        (
            kind.to_string(),
            name.to_string(),
            table.to_string(),
            sql.map(str::to_string),
        )
    };
    rows.extend([
        row("table", "ca_material", "ca_material", Some(CA_MATERIAL)),
        row(
            "index",
            "sqlite_autoindex_ca_material_1",
            "ca_material",
            None,
        ),
        row("table", "hecate_tls", "hecate_tls", Some(HECATE_TLS)),
        row(
            "table",
            "enroll_attempts",
            "enroll_attempts",
            Some(ENROLL_ATTEMPTS),
        ),
    ]);
    rows
}

#[test]
fn a_new_database_has_the_python_schema_with_the_identities_and_the_pragmas() {
    let tmp = tempfile::tempdir().unwrap();
    let connection = store::open_database(&tmp.path().join("hecate.db")).unwrap();
    let mut statement = connection
        .prepare("SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY rowid")
        .unwrap();
    let rows: Vec<SchemaRow> = statement
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(rows, schema_with_identities());
    let synchronous: i64 = connection
        .query_row("PRAGMA synchronous", [], |row| row.get(0))
        .unwrap();
    let secure_delete: i64 = connection
        .query_row("PRAGMA secure_delete", [], |row| row.get(0))
        .unwrap();
    assert_eq!((synchronous, secure_delete), (2, 1));
}
