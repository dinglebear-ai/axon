use super::*;

#[test]
fn redirected_json_and_quiet_never_show_progress() {
    let mut config = Config::default();
    assert!(!ConsolePolicy::for_stream(&config, false).progress_enabled());

    config.json_output = true;
    assert!(!ConsolePolicy::for_stream(&config, true).progress_enabled());

    config.json_output = false;
    config.quiet = true;
    assert!(!ConsolePolicy::for_stream(&config, true).progress_enabled());
}

#[test]
fn verbosity_selects_console_threshold() {
    let mut config = Config::default();
    assert_eq!(
        ConsolePolicy::for_stream(&config, true).console_log_level(),
        ConsoleLogLevel::Warn
    );
    config.verbosity = 1;
    assert_eq!(
        ConsolePolicy::for_stream(&config, true).console_log_level(),
        ConsoleLogLevel::Info
    );
    config.verbosity = 2;
    assert_eq!(
        ConsolePolicy::for_stream(&config, true).console_log_level(),
        ConsoleLogLevel::Debug
    );
    config.quiet = true;
    assert_eq!(
        ConsolePolicy::for_stream(&config, true).console_log_level(),
        ConsoleLogLevel::Error
    );
}

#[test]
fn explicit_motion_choice_controls_interactive_progress() {
    let mut config = Config {
        motion_choice: MotionChoice::Always,
        ..Config::default()
    };
    assert!(ConsolePolicy::for_stream(&config, true).motion_enabled());

    config.motion_choice = MotionChoice::Never;
    assert!(!ConsolePolicy::for_stream(&config, true).motion_enabled());

    config.motion_choice = MotionChoice::Always;
    assert!(!ConsolePolicy::for_stream(&config, false).motion_enabled());
}
