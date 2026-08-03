use std::cell::RefCell;
use std::path::Path;

use sha2::{Digest, Sha256};

use super::{GeneratedContractsCommand, refresh_fixture, run_with};

fn write(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().expect("fixture parent")).unwrap();
    std::fs::write(path, content).unwrap();
}

fn checksum(root: &Path, path: &str) -> String {
    let bytes = std::fs::read(root.join(path)).unwrap();
    format!("sha256:{:x}", Sha256::digest(bytes))
}

#[test]
fn generated_contracts_runs_schemas_before_dependent_docs() {
    for (command, check) in [
        (GeneratedContractsCommand::Refresh, false),
        (GeneratedContractsCommand::Check, true),
    ] {
        let calls = RefCell::new(Vec::new());
        run_with(
            command,
            |actual_check| {
                calls.borrow_mut().push(("schemas", actual_check));
                Ok(())
            },
            |actual_check| {
                calls.borrow_mut().push(("docs", actual_check));
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(*calls.borrow(), [("schemas", check), ("docs", check)]);
    }
}

#[test]
fn refresh_keeps_split_api_source_schema_and_dependent_docs_coupled() {
    let tmp = crate::schemas::test_fixture_repo();
    write(
        tmp.path(),
        "crates/axon-api/src/source.rs",
        "pub mod enums;\n",
    );
    write(
        tmp.path(),
        "crates/axon-api/src/source/enums.rs",
        "include!(\"enums/runtime.rs\");\n",
    );
    write(
        tmp.path(),
        "crates/axon-api/src/source/enums/runtime.rs",
        "pub enum RuntimeState { Ready }\n",
    );
    write(
        tmp.path(),
        "docs/reference/presentation/tokens.schema.json",
        "{}\n",
    );

    refresh_fixture(tmp.path()).unwrap();
    let before = checksum(tmp.path(), "docs/reference/api/schemas.json");

    write(
        tmp.path(),
        "crates/axon-api/src/source/enums/runtime.rs",
        "pub enum RuntimeState { Ready, Running }\n",
    );
    refresh_fixture(tmp.path()).unwrap();
    let after = checksum(tmp.path(), "docs/reference/api/schemas.json");

    assert_ne!(after, before, "split source must change the API artifact");
    for path in [
        "docs/reference/api/dto.md",
        "docs/reference/api/enums.md",
        "docs/reference/generated/schemas.md",
    ] {
        let content = std::fs::read_to_string(tmp.path().join(path)).unwrap();
        assert!(
            content.contains(&after),
            "{path} must carry the refreshed API artifact digest"
        );
        assert!(
            !content.contains(&before),
            "{path} must not retain the stale API artifact digest"
        );
    }
}
