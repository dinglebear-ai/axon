use super::*;

#[test]
fn worker_mode_json_contract_is_machine_readable() {
    let mode = WorkerMode::InProcess {
        pending_at_start: 3,
        elapsed_secs: 2,
    };

    let output = worker_mode_json(&mode);

    assert_eq!(output["status"], "queue_drained");
    assert_eq!(output["pending_at_start"], 3);
    assert_eq!(output["elapsed_secs"], 2);
    assert_eq!(worker_mode_json(&WorkerMode::Started)["status"], "started");
    assert_eq!(
        worker_mode_json(&WorkerMode::Unsupported("not available"))["status"],
        "unsupported"
    );
}
