use super::*;

struct RunEnvironment {
    qdrant_url: String,
    axon_bin: PathBuf,
    axon_version: String,
    git_sha: String,
    git_branch: String,
    work_root: PathBuf,
    stamp: String,
    output: PathBuf,
    client: reqwest::blocking::Client,
    tei_metrics_url: Option<String>,
}

impl RunEnvironment {
    fn new(root: &Path, args: &BenchSourceArgs) -> anyhow::Result<Self> {
        support::load_dotenv_if_present()?;
        let qdrant_url = support::resolve_url("QDRANT_URL", args.qdrant_url.clone())
            .or_else(|| std::env::var("AXON_QDRANT_URL").ok())
            .ok_or_else(|| anyhow!("QDRANT_URL or AXON_QDRANT_URL is required"))?;
        let tei_url = support::resolve_url("TEI_URL", args.tei_url.clone());
        let axon_bin = args.axon_bin.clone().unwrap_or_else(|| {
            support::default_axon_bin(root).unwrap_or_else(|| PathBuf::from("axon"))
        });
        let axon_version = command_text(Command::new(&axon_bin).arg("--version"))?;
        let git_sha = command_text(
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(root),
        )?;
        let git_branch = command_text(
            Command::new("git")
                .args(["branch", "--show-current"])
                .current_dir(root),
        )?;
        let work_root = root.join("target/bench-source");
        fs::create_dir_all(&work_root)?;
        let stamp = format!("{}-{}", support::unix_timestamp(), std::process::id());
        let output = args
            .output
            .clone()
            .unwrap_or_else(|| work_root.join(format!("results-{stamp}.json")));
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        let tei_metrics_url =
            support::select_tei_metrics_url(&client, tei_url.as_deref(), tei_url.is_some());
        Ok(Self {
            qdrant_url,
            axon_bin,
            axon_version,
            git_sha,
            git_branch,
            work_root,
            stamp,
            output,
            client,
            tei_metrics_url,
        })
    }
}

pub(super) fn run(root: &Path, args: BenchSourceArgs) -> anyhow::Result<()> {
    validate_args(&args)?;
    let environment = RunEnvironment::new(root, &args)?;
    let mut records = Vec::new();
    for warm in scenarios(args.scenario) {
        run_scenario(&environment, &args, warm, &mut records)?;
    }
    finish_report(
        args,
        environment.output,
        environment.git_sha,
        environment.git_branch,
        environment.axon_version,
        records,
    )
}

fn validate_args(args: &BenchSourceArgs) -> anyhow::Result<()> {
    if !args.allow_live_network {
        bail!(
            "live source benchmarking fetches and indexes an external site; rerun with --allow-live-network"
        );
    }
    if args.runs == 0 {
        bail!("--runs must be greater than zero");
    }
    let parsed_url = reqwest::Url::parse(&args.url).context("benchmark URL is invalid")?;
    if !matches!(parsed_url.scheme(), "http" | "https") {
        bail!("benchmark URL must use http or https");
    }
    Ok(())
}

fn scenarios(mode: ScenarioMode) -> Vec<bool> {
    match mode {
        ScenarioMode::Cold => vec![false],
        ScenarioMode::Warm => vec![true],
        ScenarioMode::Both => vec![false, true],
    }
}

fn run_scenario(
    environment: &RunEnvironment,
    args: &BenchSourceArgs,
    warm: bool,
    records: &mut Vec<RunRecord>,
) -> anyhow::Result<()> {
    let name = if warm { "warm" } else { "cold" };
    let shared_state = environment
        .work_root
        .join(format!("state-{}-{name}", environment.stamp));
    let shared_collection = format!(
        "axon_source_bench_{}_{name}",
        environment.stamp.replace('-', "_")
    );
    if warm {
        prime_warm_cache(environment, args, &shared_state, &shared_collection)?;
    }
    for iteration in 1..=args.runs {
        records.push(run_iteration(
            environment,
            args,
            warm,
            name,
            iteration,
            &shared_state,
            &shared_collection,
        )?);
    }
    if warm && !args.keep_state {
        cleanup_benchmark_resources(
            &environment.client,
            &environment.work_root,
            &environment.qdrant_url,
            &shared_state,
            &shared_collection,
        )?;
    }
    Ok(())
}

fn prime_warm_cache(
    environment: &RunEnvironment,
    args: &BenchSourceArgs,
    state: &Path,
    collection: &str,
) -> anyhow::Result<()> {
    fs::create_dir_all(state)?;
    eprintln!("priming warm benchmark cache for {}", args.url);
    let run = execute_run(
        &environment.client,
        &environment.axon_bin,
        &args.url,
        true,
        0,
        args.max_pages,
        state,
        collection,
        &environment.qdrant_url,
        environment.tei_metrics_url.as_deref(),
    );
    match run {
        Ok(_) => Ok(()),
        Err(error) if args.keep_state => Err(error),
        Err(error) => Err(combine_run_and_cleanup(
            error,
            cleanup_benchmark_resources(
                &environment.client,
                &environment.work_root,
                &environment.qdrant_url,
                state,
                collection,
            ),
        )),
    }
}

fn run_iteration(
    environment: &RunEnvironment,
    args: &BenchSourceArgs,
    warm: bool,
    scenario: &str,
    iteration: usize,
    shared_state: &Path,
    shared_collection: &str,
) -> anyhow::Result<RunRecord> {
    let state = if warm {
        shared_state.to_path_buf()
    } else {
        environment.work_root.join(format!(
            "state-{}-{scenario}-{iteration}",
            environment.stamp
        ))
    };
    let collection = if warm {
        shared_collection.to_string()
    } else {
        format!(
            "axon_source_bench_{}_{scenario}_{iteration}",
            environment.stamp.replace('-', "_")
        )
    };
    fs::create_dir_all(&state)?;
    eprintln!("running {scenario} benchmark {iteration}/{}", args.runs);
    let run = execute_run(
        &environment.client,
        &environment.axon_bin,
        &args.url,
        warm,
        iteration,
        args.max_pages,
        &state,
        &collection,
        &environment.qdrant_url,
        environment.tei_metrics_url.as_deref(),
    );
    if warm {
        return match run {
            Ok(record) => Ok(record),
            Err(error) if args.keep_state => Err(error),
            Err(error) => Err(combine_run_and_cleanup(
                error,
                cleanup_benchmark_resources(
                    &environment.client,
                    &environment.work_root,
                    &environment.qdrant_url,
                    shared_state,
                    shared_collection,
                ),
            )),
        };
    }
    let cleanup = if args.keep_state {
        Ok(())
    } else {
        cleanup_benchmark_resources(
            &environment.client,
            &environment.work_root,
            &environment.qdrant_url,
            &state,
            &collection,
        )
    };
    resolve_run_with_cleanup(run, cleanup)
}
