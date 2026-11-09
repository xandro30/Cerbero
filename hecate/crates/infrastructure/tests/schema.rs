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

#[test]
fn a_new_database_has_exactly_the_python_schema_and_pragmas() {
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
    assert_eq!(rows, python_schema());
    let synchronous: i64 = connection
        .query_row("PRAGMA synchronous", [], |row| row.get(0))
        .unwrap();
    let secure_delete: i64 = connection
        .query_row("PRAGMA secure_delete", [], |row| row.get(0))
        .unwrap();
    assert_eq!((synchronous, secure_delete), (2, 1));
}
