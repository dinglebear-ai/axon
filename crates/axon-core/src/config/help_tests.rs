use super::{COMMAND_SECTIONS, command_options, command_rows, global_options};
use crate::config::cli::Cli;
use clap::CommandFactory;
use std::collections::HashSet;

#[test]
fn top_level_help_commands_come_from_clap_surface() {
    let names: Vec<String> = command_rows().into_iter().map(|(name, _)| name).collect();

    for expected in [
        "watch",
        "monitor",
        "scrape",
        "crawl",
        "embed",
        "ingest",
        "code-search",
        "map",
        "extract",
        "search",
        "research",
        "debug",
        "doctor",
        "query",
        "retrieve",
        "ask",
        "evaluate",
        "train",
        "suggest",
        "sources",
        "domains",
        "stats",
        "status",
        "memory",
        "sessions",
        "sync",
        "screenshot",
        "completions",
        "serve",
        "setup",
        "mcp",
        "migrate",
        "config",
    ] {
        assert!(names.iter().any(|name| name == expected), "{expected}");
    }

    for removed in ["dedupe", "purge", "fresh", "refresh"] {
        assert!(
            !names.iter().any(|name| name == removed),
            "removed command still present in clap surface: {removed}"
        );
    }
}

#[test]
fn config_target_help_does_not_claim_schema_bypass() {
    let command = Cli::command();
    let config = command.find_subcommand("config").expect("config command");
    for action in ["get", "set", "unset"] {
        let subcommand = config.find_subcommand(action).expect("config action");
        for flag in ["env", "toml"] {
            let help = subcommand
                .get_arguments()
                .find(|arg| arg.get_id().as_str() == flag)
                .and_then(|arg| arg.get_help())
                .map(ToString::to_string)
                .expect("target flag help");
            assert!(
                help.contains("key must satisfy"),
                "{action} --{flag}: {help}"
            );
            assert!(
                !help.contains("regardless of key shape"),
                "{action} --{flag}: {help}"
            );
        }
    }
}

#[test]
fn curated_command_sections_cover_current_clap_surface() {
    let names: HashSet<String> = command_rows().into_iter().map(|(name, _)| name).collect();
    let categorized: HashSet<&str> = COMMAND_SECTIONS
        .iter()
        .flat_map(|(_, commands)| commands.iter().copied())
        .collect();

    for name in names {
        assert!(categorized.contains(name.as_str()), "{name}");
    }
}

#[test]
fn generated_global_help_covers_every_visible_global_clap_option() {
    let command = Cli::command();
    let labels = global_options(&command)
        .into_iter()
        .map(|(label, _)| label)
        .collect::<Vec<_>>();

    for expected in [
        "--automation-script <PATH>",
        "--batch-concurrency <BATCH_CONCURRENCY>",
        "--cache <CACHE>",
        "--query <QUERY>",
        "--screenshot-full-page <SCREENSHOT_FULL_PAGE>",
        "--yes",
    ] {
        assert!(
            labels.iter().any(|label| label == expected),
            "missing global option from generated help: {expected}"
        );
    }
}

#[test]
fn extract_help_includes_the_prompt_query_option() {
    let command = Cli::command();
    let extract = command
        .get_subcommands()
        .find(|subcommand| subcommand.get_name() == "extract")
        .expect("extract command");
    let labels = command_options(extract, &["extract"])
        .into_iter()
        .map(|(label, _)| label)
        .collect::<Vec<_>>();

    assert!(labels.iter().any(|label| label == "--query <QUERY>"));
}

#[test]
fn specialized_commands_omit_unimplemented_source_options() {
    let command = Cli::command();
    for name in ["endpoints", "extract", "brand", "diff", "screenshot"] {
        let subcommand = command
            .get_subcommands()
            .find(|subcommand| subcommand.get_name() == name)
            .unwrap_or_else(|| panic!("{name} command"));
        let labels = command_options(subcommand, &[name])
            .into_iter()
            .map(|(label, _)| label)
            .collect::<Vec<_>>();

        for unsupported in ["--warc", "--skip-embed", "--cron-every-seconds"] {
            assert!(
                !labels.iter().any(|label| label.starts_with(unsupported)),
                "{name} advertises unsupported {unsupported}: {labels:?}"
            );
        }
    }
}

#[test]
fn setup_install_help_omits_unrelated_crawler_options() {
    let command = Cli::command();
    let setup = command
        .get_subcommands()
        .find(|subcommand| subcommand.get_name() == "setup")
        .expect("setup command");
    let install = setup
        .get_subcommands()
        .find(|subcommand| subcommand.get_name() == "install")
        .expect("install command");
    let labels = command_options(install, &["setup", "install"])
        .into_iter()
        .map(|(label, _)| label)
        .collect::<Vec<_>>();

    assert!(!labels.iter().any(|label| label.starts_with("--max-pages")));
    assert!(labels.iter().any(|label| label == "--json"));
}

#[test]
fn generated_help_includes_clap_possible_values() {
    let command = Cli::command();
    let evaluate = command
        .get_subcommands()
        .find(|subcommand| subcommand.get_name() == "evaluate")
        .expect("evaluate command");
    let (_, description) = command_options(evaluate, &["evaluate"])
        .into_iter()
        .find(|(label, _)| label == "--responses-mode <RESPONSES_MODE>")
        .expect("responses mode option");

    assert!(description.contains("Possible values: inline, side-by-side, events"));
}

#[test]
fn generated_help_does_not_claim_set_true_flags_take_boolean_values() {
    let command = Cli::command();
    let source = command
        .get_subcommands()
        .find(|subcommand| subcommand.get_name() == "source")
        .expect("source command");
    let (label, description) = command_options(source, &["source"])
        .into_iter()
        .find(|(label, _)| label == "--skip-embed")
        .expect("skip embed option");

    assert_eq!(label, "--skip-embed");
    assert!(!description.contains("Possible values"));
}

#[test]
fn monitor_jobs_help_advertises_watch_once() {
    let command = Cli::command();
    let monitor = command
        .get_subcommands()
        .find(|subcommand| subcommand.get_name() == "monitor")
        .expect("monitor command");
    let jobs = monitor
        .get_subcommands()
        .find(|subcommand| subcommand.get_name() == "jobs")
        .expect("monitor jobs command");
    let labels = command_options(jobs, &["monitor", "jobs"])
        .into_iter()
        .map(|(label, _)| label)
        .collect::<Vec<_>>();

    assert_eq!(
        labels
            .iter()
            .filter(|label| label.as_str() == "--watch")
            .count(),
        1
    );
}

#[test]
fn watch_mutation_help_does_not_advertise_status_watch_mode() {
    let command = Cli::command();
    let watch = command
        .get_subcommands()
        .find(|subcommand| subcommand.get_name() == "watch")
        .expect("watch command");
    let update = watch
        .get_subcommands()
        .find(|subcommand| subcommand.get_name() == "update")
        .expect("watch update command");
    let labels = command_options(update, &["watch", "update"])
        .into_iter()
        .map(|(label, _)| label)
        .collect::<Vec<_>>();

    assert!(!labels.iter().any(|label| label == "--watch"));
}

#[test]
fn source_help_is_the_only_command_surface_that_advertises_cron() {
    let command = Cli::command();
    let source = command
        .get_subcommands()
        .find(|subcommand| subcommand.get_name() == "source")
        .expect("source command");
    let source_labels = command_options(source, &["source"])
        .into_iter()
        .map(|(label, _)| label)
        .collect::<Vec<_>>();
    assert!(
        source_labels
            .iter()
            .any(|label| label.starts_with("--cron-every-seconds"))
    );
    assert!(
        source_labels
            .iter()
            .any(|label| label.starts_with("--cron-max-runs"))
    );

    for name in ["preflight", "map", "scrape", "ask", "doctor"] {
        let subcommand = command
            .get_subcommands()
            .find(|subcommand| subcommand.get_name() == name)
            .unwrap_or_else(|| panic!("{name} command"));
        let labels = command_options(subcommand, &[name])
            .into_iter()
            .map(|(label, _)| label)
            .collect::<Vec<_>>();
        assert!(
            !labels.iter().any(|label| label.starts_with("--cron-")),
            "{name} must not advertise unproven cron execution: {labels:?}"
        );
    }
}
