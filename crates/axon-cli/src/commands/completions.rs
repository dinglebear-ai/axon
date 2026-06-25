use axon_core::config::{Config, build_cli_command};
use clap_complete::generate;
use clap_complete::shells::{Bash, Fish, Zsh};
use std::error::Error;

pub async fn run_completions(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let shell =
        cfg.positional.first().map(String::as_str).ok_or(
            "completions requires a shell argument.\nUsage: axon completions <bash|zsh|fish>",
        )?;
    let script = generate_completion_script(shell)?;
    print!("{script}");
    Ok(())
}

fn generate_completion_script(shell: &str) -> Result<String, Box<dyn Error>> {
    let mut command = build_cli_command();
    let mut out = Vec::<u8>::new();
    match shell {
        "bash" => generate(Bash, &mut command, "axon", &mut out),
        "zsh" => generate(Zsh, &mut command, "axon", &mut out),
        "fish" => generate(Fish, &mut command, "axon", &mut out),
        other => {
            return Err(format!(
                "unsupported shell '{other}' for completions.\n\
                 Supported shells: bash, zsh, fish\n\
                 Usage: axon completions <bash|zsh|fish>"
            )
            .into());
        }
    }
    let script = String::from_utf8(out)
        .map_err(|e| format!("failed to decode generated completion script as UTF-8: {e}"))?;
    Ok(script)
}

#[cfg(test)]
#[path = "completions_tests.rs"]
mod tests;
