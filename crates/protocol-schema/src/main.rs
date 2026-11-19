use std::fs;
use std::io;
use std::path::PathBuf;

use protocol_schema::{invalid_fixtures, schema_json, valid_fixtures};
use serde_json::Value;

fn write_set(dir: &PathBuf, fixtures: &[(String, Value)]) -> io::Result<()> {
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    fs::create_dir_all(dir)?;
    for (name, value) in fixtures {
        let text = format!("{}\n", serde_json::to_string_pretty(value)?);
        fs::write(dir.join(format!("{name}.json")), text)?;
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../protocol/v1");
    fs::create_dir_all(&root)?;
    fs::write(root.join("schema.json"), schema_json())?;
    write_set(&root.join("fixtures/valid"), &valid_fixtures())?;
    write_set(&root.join("fixtures/invalid"), &invalid_fixtures())
}
