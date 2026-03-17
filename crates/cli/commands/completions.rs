use crate::crates::core::config::{Config, build_cli_command};
use clap_complete::generate;
use clap_complete::shells::{Bash, Fish, Zsh};
use std::error::Error;

pub async fn run_completions(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let shell = cfg
        .positional
        .first()
        .map(String::as_str)
        .ok_or("completions requires a shell: bash, zsh, or fish")?;
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
        other => return Err(anyhow::anyhow!("unsupported shell for completions: {other}").into()),
    }
    let script = String::from_utf8(out)
        .map_err(|e| format!("failed to decode generated completion script as UTF-8: {e}"))?;
    Ok(script)
}

#[cfg(test)]
mod tests {
    use super::generate_completion_script;

    #[test]
    fn bash_completion_includes_commands_flags_and_enum_values() {
        let script = generate_completion_script("bash").expect("bash completion");
        assert!(script.contains("complete -F _axon"));
        assert!(script.contains("completions"));
        assert!(script.contains("--wait"));
        assert!(script.contains("--render-mode"));
        assert!(script.contains("--performance-profile"));
        assert!(script.contains("auto-switch"));
        assert!(script.contains("high-stable"));
    }

    #[test]
    fn zsh_completion_includes_compdef_and_enum_values() {
        let script = generate_completion_script("zsh").expect("zsh completion");
        assert!(script.contains("#compdef axon"));
        assert!(script.contains("compdef _axon axon"));
        assert!(script.contains("render-mode"));
        assert!(script.contains("auto-switch"));
        assert!(script.contains("performance-profile"));
        assert!(script.contains("high-stable"));
    }

    #[test]
    fn fish_completion_emits_subcommands_and_enum_values() {
        let script = generate_completion_script("fish").expect("fish completion");
        assert!(script.contains("complete -c axon"));
        assert!(script.contains("completions"));
        assert!(script.contains("completion"));
        assert!(script.contains("render-mode"));
        assert!(script.contains("auto-switch"));
        assert!(script.contains("performance-profile"));
        assert!(script.contains("high-stable"));
    }
}
