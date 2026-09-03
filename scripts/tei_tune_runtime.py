"""Validation and Docker snapshot reconstruction for the TEI tuning tool."""

from __future__ import annotations

import re

PRESETS = {
    "rtx4070-axon": {"max-concurrent-requests": 1024, "max-batch-tokens": 163840, "max-batch-requests": 16, "max-client-batch-size": 128, "tokenization-workers": 16},
    "stable": {"max-concurrent-requests": 1024, "max-batch-tokens": 163840, "max-batch-requests": 16, "max-client-batch-size": 128, "tokenization-workers": 16},
    "admission": {"max-concurrent-requests": 1024, "max-batch-tokens": 196608, "max-batch-requests": 1024, "max-client-batch-size": 256, "tokenization-workers": 32},
    "probe-212k": {"max-concurrent-requests": 1024, "max-batch-tokens": 212992, "max-batch-requests": 1024, "max-client-batch-size": 256, "tokenization-workers": 32},
}
KNOBS = frozenset(next(iter(PRESETS.values())))


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed <= 0:
        raise ValueError("must be a positive integer")
    return parsed


def resolve_config(preset: str, overrides: list[str], allow_unsafe: bool) -> dict[str, int]:
    config = dict(PRESETS[preset])
    for item in overrides:
        if "=" not in item:
            raise ValueError(f"override must be KEY=VALUE: {item}")
        key, value = item.split("=", 1)
        key = key.strip().replace("_", "-")
        if key not in KNOBS:
            raise ValueError(f"unknown knob {key!r}; choose from {', '.join(sorted(KNOBS))}")
        config[key] = positive_int(value)
    if config["max-batch-tokens"] > 212992 and not allow_unsafe:
        raise ValueError("max-batch-tokens above 212992 requires --allow-unsafe; 262144 caused CUDA OOM on the RTX 4070")
    return config


SSH_HOST_PATTERN = re.compile(
    r"^(?:[A-Za-z0-9_][A-Za-z0-9_.-]*@)?(?:[A-Za-z0-9](?:[A-Za-z0-9.-]*[A-Za-z0-9])?|\[[0-9A-Fa-f:]+\])$"
)
CONTAINER_PATTERN = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.-]*$")


def validate_ssh_host(host: str) -> str:
    if not SSH_HOST_PATTERN.fullmatch(host):
        raise ValueError(f"invalid SSH host: {host!r}")
    return host


def validate_container_name(container: str) -> str:
    if not CONTAINER_PATTERN.fullmatch(container):
        raise ValueError(f"invalid Docker container name: {container!r}")
    return container


def validate_snapshot_host_config(host: dict) -> None:
    unsupported = {
        "Privileged": host.get("Privileged") is True,
        "ReadonlyRootfs": host.get("ReadonlyRootfs") is True,
        "AutoRemove": host.get("AutoRemove") is True,
        "CapAdd": bool(host.get("CapAdd")),
        "CapDrop": bool(host.get("CapDrop")),
        "SecurityOpt": bool(host.get("SecurityOpt")),
        "Devices": bool(host.get("Devices")),
        "Tmpfs": bool(host.get("Tmpfs")),
        "ExtraHosts": bool(host.get("ExtraHosts")),
        "Dns": bool(host.get("Dns")),
        "DnsSearch": bool(host.get("DnsSearch")),
        "Ulimits": bool(host.get("Ulimits")),
        "PidMode": bool(host.get("PidMode")),
        "IpcMode": host.get("IpcMode") not in (None, "", "private"),
    }
    present = sorted(key for key, enabled in unsupported.items() if enabled)
    if present:
        raise ValueError(
            f"unsupported material Docker settings in rollback snapshot: {', '.join(present)}"
        )


def docker_run_from_snapshot(container: str, snapshot: dict) -> list[str]:
    config = snapshot.get("config", {})
    host = snapshot.get("host_config", {})
    run = ["docker", "run", "-d", "--name", container]
    restart = host.get("RestartPolicy", {}).get("Name")
    if restart and restart != "no":
        retry_count = host.get("RestartPolicy", {}).get("MaximumRetryCount", 0)
        restart_value = (
            f"{restart}:{retry_count}"
            if restart == "on-failure" and retry_count
            else restart
        )
        run.extend(("--restart", restart_value))
    validate_snapshot_host_config(host)
    network = host.get("NetworkMode") or snapshot.get("network_mode")
    if network:
        run.extend(("--network", network))
        aliases = (snapshot.get("networks") or {}).get(network, {}).get("Aliases") or []
        for alias in aliases:
            if alias and alias != container:
                run.extend(("--network-alias", alias))
    runtime = host.get("Runtime")
    if runtime:
        run.extend(("--runtime", runtime))
    requests = host.get("DeviceRequests") or []
    if requests and requests[0].get("Driver") == "nvidia":
        device_ids = requests[0].get("DeviceIDs") or []
        run.extend(("--gpus", f"device={','.join(device_ids)}" if device_ids else "all"))
    resource_flags = (
        ("Memory", "--memory"),
        ("MemorySwap", "--memory-swap"),
        ("CpuShares", "--cpu-shares"),
        ("CpuPeriod", "--cpu-period"),
        ("CpuQuota", "--cpu-quota"),
        ("CpusetCpus", "--cpuset-cpus"),
        ("CpusetMems", "--cpuset-mems"),
        ("ShmSize", "--shm-size"),
        ("PidsLimit", "--pids-limit"),
    )
    for key, flag in resource_flags:
        value = host.get(key)
        if value not in (None, "", 0) and not (key != "MemorySwap" and value == -1):
            run.extend((flag, str(value)))
    if host.get("NanoCpus") not in (None, 0):
        run.extend(("--cpus", str(host["NanoCpus"] / 1_000_000_000)))
    if host.get("OomKillDisable") is True:
        run.append("--oom-kill-disable")
    log_config = host.get("LogConfig") or {}
    if log_config.get("Type"):
        run.extend(("--log-driver", log_config["Type"]))
    for key, value in (log_config.get("Config") or {}).items():
        run.extend(("--log-opt", f"{key}={value}"))
    for container_port, bindings in (host.get("PortBindings") or {}).items():
        for binding in bindings or []:
            published = (
                f"{binding.get('HostIp')}:{binding['HostPort']}:{container_port}"
                if binding.get("HostIp")
                else f"{binding['HostPort']}:{container_port}"
            )
            run.extend(("-p", published))
    for bind in host.get("Binds") or []:
        run.extend(("-v", bind))
    if config.get("Env"):
        run.extend(("--env-file", "/dev/stdin"))
    for key, value in (config.get("Labels") or {}).items():
        run.extend(("--label", f"{key}={value}"))
    if config.get("User"):
        run.extend(("--user", config["User"]))
    if config.get("WorkingDir"):
        run.extend(("--workdir", config["WorkingDir"]))
    entrypoint = snapshot.get("entrypoint") or []
    if entrypoint:
        run.extend(("--entrypoint", entrypoint[0]))
    run.append(snapshot["image"])
    run.extend(entrypoint[1:])
    run.extend(snapshot.get("cmd") or [])
    return run


def secondary_network_commands(container: str, snapshot: dict) -> list[list[str]]:
    networks = snapshot.get("networks") or {}
    primary = snapshot.get("host_config", {}).get("NetworkMode") or snapshot.get(
        "network_mode"
    )
    commands = []
    for network, settings in networks.items():
        unsupported = [
            key
            for key in ("IPAMConfig", "Links", "MacAddress", "DriverOpts")
            if settings.get(key)
        ]
        if unsupported:
            raise ValueError(
                f"unsupported material Docker network settings for {network}: "
                f"{', '.join(unsupported)}"
            )
        if network == primary:
            continue
        command = ["docker", "network", "connect"]
        for alias in settings.get("Aliases") or []:
            if alias and alias != container:
                command.extend(("--alias", alias))
        command.extend((network, container))
        commands.append(command)
    return commands
