use std::io::ErrorKind;
use std::time::Duration;

use tokio::process::Command;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CommandStatus {
    Ok,
    Failed,
    NotFound,
    TimedOut,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CommandCheck {
    pub status: CommandStatus,
    pub detail: String,
}

pub async fn check_command<const N: usize>(
    program: &str,
    args: [&str; N],
    timeout: Duration,
) -> CommandCheck {
    let mut command = Command::new(program);
    command.args(args);
    match tokio::time::timeout(timeout, command.output()).await {
        Ok(Ok(output)) if output.status.success() => CommandCheck {
            status: CommandStatus::Ok,
            detail: String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("available")
                .to_string(),
        },
        Ok(Ok(output)) => CommandCheck {
            status: CommandStatus::Failed,
            detail: String::from_utf8_lossy(&output.stderr)
                .lines()
                .next()
                .unwrap_or("command failed")
                .to_string(),
        },
        Ok(Err(err)) if err.kind() == ErrorKind::NotFound => CommandCheck {
            status: CommandStatus::NotFound,
            detail: "not found on PATH".to_string(),
        },
        Ok(Err(err)) => CommandCheck {
            status: CommandStatus::Failed,
            detail: err.to_string(),
        },
        Err(_) => CommandCheck {
            status: CommandStatus::TimedOut,
            detail: "timed out".to_string(),
        },
    }
}
