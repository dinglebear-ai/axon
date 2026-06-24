use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha384};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const MIGRATIONS_DIR: &str = "crates/axon-jobs/src/migrations";
const CHECKSUMS_FILE: &str = "crates/axon-jobs/src/migration-checksums.txt";

#[derive(Debug, Clone, PartialEq, Eq)]
struct MigrationEntry {
    name: String,
    checksum: String,
}

pub fn check(root: &Path) -> Result<()> {
    let entries = check_inner(root)?;
    println!(
        "OK: SQLite job migrations are sequential and checksum-pinned ({} migrations).",
        entries
    );
    Ok(())
}

pub fn update(root: &Path) -> Result<()> {
    let migrations = list_migrations(&root.join(MIGRATIONS_DIR))?;
    validate_sequence(&migrations)?;
    let entries = checksum_entries(root, &migrations)?;
    fs::write(root.join(CHECKSUMS_FILE), render_manifest(&entries))
        .with_context(|| format!("failed to write {}", root.join(CHECKSUMS_FILE).display()))?;
    check_inner(root)?;
    println!(
        "Updated {CHECKSUMS_FILE} with {} migrations.",
        entries.len()
    );
    Ok(())
}

fn check_inner(root: &Path) -> Result<usize> {
    let migrations = list_migrations(&root.join(MIGRATIONS_DIR))?;
    validate_sequence(&migrations)?;
    let expected = read_checksum_manifest(&root.join(CHECKSUMS_FILE))?;
    validate_manifest_matches_files(&migrations, &expected)?;
    validate_checksums(root, &migrations, &expected)?;
    Ok(migrations.len())
}

fn list_migrations(dir: &Path) -> Result<Vec<String>> {
    let mut migrations = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("sql") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("migration filename is not UTF-8")?
            .to_owned();
        migrations.push(name);
    }
    migrations.sort();
    Ok(migrations)
}

fn validate_sequence(migrations: &[String]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for (index, name) in migrations.iter().enumerate() {
        let Some((prefix, _rest)) = name.split_once('_') else {
            bail!("migration {name} must start with a zero-padded numeric prefix");
        };
        let version: usize = prefix
            .parse()
            .with_context(|| format!("migration {name} has non-numeric prefix {prefix}"))?;
        let expected = index + 1;
        if version != expected {
            bail!(
                "migration sequence gap or reorder: expected {:04} at position {}, found {name}",
                expected,
                index + 1
            );
        }
        if !seen.insert(version) {
            bail!("duplicate migration version {version:04}");
        }
    }
    Ok(())
}

fn read_checksum_manifest(path: &Path) -> Result<BTreeMap<String, String>> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut entries = BTreeMap::new();
    for (line_no, raw_line) in contents.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let name = fields
            .next()
            .with_context(|| format!("missing migration filename on line {}", line_no + 1))?;
        let checksum = fields
            .next()
            .with_context(|| format!("missing checksum for {name} on line {}", line_no + 1))?;
        if fields.next().is_some() {
            bail!(
                "unexpected extra fields in {} on line {}",
                path.display(),
                line_no + 1
            );
        }
        if !name.ends_with(".sql") {
            bail!(
                "manifest entry {name} on line {} is not a SQL file",
                line_no + 1
            );
        }
        if checksum.len() != 96 || !checksum.chars().all(|c| c.is_ascii_hexdigit()) {
            bail!("manifest checksum for {name} must be 96 hex characters");
        }
        if entries
            .insert(name.to_owned(), checksum.to_ascii_lowercase())
            .is_some()
        {
            bail!("duplicate manifest entry for {name}");
        }
    }
    Ok(entries)
}

fn validate_manifest_matches_files(
    migrations: &[String],
    expected: &BTreeMap<String, String>,
) -> Result<()> {
    let migration_set: BTreeSet<_> = migrations.iter().cloned().collect();
    let manifest_set: BTreeSet<_> = expected.keys().cloned().collect();

    let missing: Vec<_> = migration_set.difference(&manifest_set).cloned().collect();
    let stale: Vec<_> = manifest_set.difference(&migration_set).cloned().collect();

    if !missing.is_empty() || !stale.is_empty() {
        if !missing.is_empty() {
            eprintln!("Missing checksum manifest entries:");
            for name in &missing {
                eprintln!("  {name}");
            }
        }
        if !stale.is_empty() {
            eprintln!("Stale checksum manifest entries:");
            for name in &stale {
                eprintln!("  {name}");
            }
        }
        bail!("SQLite migration checksum manifest is out of sync");
    }
    Ok(())
}

fn validate_checksums(
    root: &Path,
    migrations: &[String],
    expected: &BTreeMap<String, String>,
) -> Result<()> {
    let mut mismatches = Vec::new();
    for name in migrations {
        let path = root.join(MIGRATIONS_DIR).join(name);
        let actual = sha384_file(&path)?;
        let expected_checksum = expected
            .get(name)
            .with_context(|| format!("missing checksum manifest entry for {name}"))?;
        if &actual != expected_checksum {
            mismatches.push(MigrationEntry {
                name: name.clone(),
                checksum: actual,
            });
        }
    }

    if !mismatches.is_empty() {
        eprintln!("SQLite migration checksum drift detected:");
        for mismatch in &mismatches {
            if let Some(expected_checksum) = expected.get(&mismatch.name) {
                eprintln!(
                    "  {} expected {} actual {}",
                    mismatch.name, expected_checksum, mismatch.checksum
                );
            }
        }
        eprintln!();
        eprintln!(
            "Migrations are append-only after merge. Add a new migration instead of editing an applied one."
        );
        bail!("SQLite migration checksum drift");
    }
    Ok(())
}

fn checksum_entries(root: &Path, migrations: &[String]) -> Result<Vec<MigrationEntry>> {
    migrations
        .iter()
        .map(|name| {
            let path = root.join(MIGRATIONS_DIR).join(name);
            Ok(MigrationEntry {
                name: name.clone(),
                checksum: sha384_file(&path)?,
            })
        })
        .collect()
}

fn render_manifest(entries: &[MigrationEntry]) -> String {
    let mut output = String::from(
        "# SHA-384 checksums for SQLite job migrations.\n\
         #\n\
         # Migrations are append-only once merged because SQLx stores the checksum in\n\
         # each live jobs.db. To change an applied migration, add a new migration instead.\n",
    );
    for entry in entries {
        output.push_str(&format!("{} {}\n", entry.name, entry.checksum));
    }
    output
}

fn sha384_file(path: &PathBuf) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let digest = Sha384::digest(&bytes);
    Ok(format!("{digest:x}"))
}

#[cfg(test)]
#[path = "sqlite_migrations_tests.rs"]
mod tests;
