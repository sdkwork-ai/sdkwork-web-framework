use std::fs;
use std::path::PathBuf;

#[test]
fn sqlx_migrations_contain_no_iam_tables() {
    let migrations_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("crates")
        .join("sdkwork-web-store-sqlx")
        .join("migrations");
    for entry in fs::read_dir(&migrations_dir).expect("read migrations dir") {
        let entry = entry.expect("migration entry");
        let sql = fs::read_to_string(entry.path()).expect("read migration");
        let lowered = sql.to_ascii_lowercase();
        assert!(
            !lowered.contains("iam_"),
            "migration {:?} must not reference iam_* tables",
            entry.file_name()
        );
        assert!(
            !lowered.contains("create table") || lowered.contains("web_"),
            "migration {:?} must only create web_* framework tables",
            entry.file_name()
        );
    }
}
