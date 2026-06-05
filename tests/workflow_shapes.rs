use std::collections::HashMap;

#[test]
fn release_checkout_sparse_paths_are_in_sparse_checkout_block() {
    let workflow = include_str!("../.github/workflows/release.yml");
    let blocks = checkout_sparse_blocks(workflow);
    assert_eq!(
        blocks.len(),
        4,
        "release workflow should have one sparse checkout block per release build job"
    );

    for (index, block) in blocks.iter().enumerate() {
        let paths = parse_sparse_checkout_paths(block);
        for required in ["tests", "scripts", "config", "vendor", ".cargo"] {
            assert!(
                paths.iter().any(|path| path == required),
                "checkout block #{index} is missing {required} from sparse-checkout paths; \
                 paths following sparse-checkout-cone-mode are ignored by actions/checkout"
            );
        }
        assert!(
            block
                .iter()
                .any(|line| line.trim() == "sparse-checkout-cone-mode: false"),
            "checkout block #{index} must explicitly disable cone mode"
        );
        let cone_index = block
            .iter()
            .position(|line| line.trim().starts_with("sparse-checkout-cone-mode:"))
            .expect("cone mode line is present");
        assert!(
            block[(cone_index + 1)..]
                .iter()
                .all(|line| !line.trim().starts_with(['.', '/'])
                    && !["tests", "scripts", "config", "vendor"].contains(&line.trim())),
            "checkout block #{index} has path-looking entries indented under sparse-checkout-cone-mode"
        );
    }
}

fn checkout_sparse_blocks(workflow: &str) -> Vec<Vec<&str>> {
    let lines: Vec<&str> = workflow.lines().collect();
    let mut blocks = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if !line.contains("uses: actions/checkout@") {
            continue;
        }
        let mut block = Vec::new();
        for candidate in lines.iter().skip(idx) {
            if candidate.trim_start().starts_with("- uses:") && !block.is_empty() {
                break;
            }
            if candidate.trim_start().starts_with("- name:") && !block.is_empty() {
                break;
            }
            block.push(*candidate);
        }
        if block.iter().any(|line| line.contains("sparse-checkout: |")) {
            blocks.push(block);
        }
    }
    blocks
}

fn parse_sparse_checkout_paths(block: &[&str]) -> Vec<String> {
    let mut paths = Vec::new();
    let mut in_sparse_checkout = false;
    let mut sparse_indent = None;
    for line in block {
        let trimmed = line.trim();
        if trimmed == "sparse-checkout: |" {
            in_sparse_checkout = true;
            sparse_indent = None;
            continue;
        }
        if !in_sparse_checkout {
            continue;
        }
        if trimmed.starts_with("sparse-checkout-cone-mode:") {
            break;
        }
        if trimmed.is_empty() {
            continue;
        }
        let indent = leading_spaces(line);
        let expected = *sparse_indent.get_or_insert(indent);
        if indent == expected {
            paths.push(trimmed.to_string());
        }
    }
    paths
}

fn leading_spaces(line: &str) -> usize {
    line.chars().take_while(|ch| *ch == ' ').count()
}

#[test]
fn ci_uses_guard_for_named_cargo_test_filters() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    let forbidden =
        "cargo test --locked server_mode_post_bodies_match_canonical_rest_contract_fields --lib";
    assert!(
        !workflow.contains(forbidden),
        "CI must not run stale cargo test filters that match zero tests"
    );

    let mut named_filters: HashMap<&str, &str> = HashMap::new();
    named_filters.insert(
        "rest_route_contracts_match_openapi_request_schemas",
        "scripts/cargo_test_filter_guard.py",
    );

    for (filter, guard) in named_filters {
        if workflow.contains(filter) {
            assert!(
                workflow.contains(&format!("python3 {guard} -- cargo test")),
                "named cargo test filter {filter} must be run through {guard}"
            );
        }
    }
}
