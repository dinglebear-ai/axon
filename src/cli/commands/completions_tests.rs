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
