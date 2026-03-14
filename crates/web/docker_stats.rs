use bollard::Docker;
use bollard::query_parameters::{ListContainersOptions, StatsOptions};
use futures_util::StreamExt;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use tokio::sync::{broadcast, mpsc};
use tracing::warn;

const CONTAINER_PREFIX: &str = "axon-";
const POLL_INTERVAL_MS: u64 = 500;

#[derive(Clone)]
struct ContainerMetrics {
    name: String,
    cpu_percent: f64,
    memory_percent: f64,
    memory_usage_mb: f64,
    memory_limit_mb: f64,
    net_rx_rate: f64,
    net_tx_rate: f64,
    block_read_rate: f64,
    block_write_rate: f64,
    status: String,
}

pub(super) async fn run_stats_loop(tx: broadcast::Sender<String>) {
    let docker = match Docker::connect_with_local_defaults() {
        Ok(d) => d,
        Err(e) => {
            warn!("Docker not available for stats: {e} — stats broadcasting disabled");
            return;
        }
    };

    let (metrics_tx, mut metrics_rx) = mpsc::channel::<ContainerMetrics>(128);
    let mut streams = HashMap::new();
    let mut latest_metrics = HashMap::new();

    let mut interval = tokio::time::interval(std::time::Duration::from_millis(POLL_INTERVAL_MS));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let filters: HashMap<String, Vec<String>> = HashMap::from([
                    ("name".to_string(), vec![CONTAINER_PREFIX.to_string()]),
                    ("status".to_string(), vec!["running".to_string()]),
                ]);

                let containers = match docker
                    .list_containers(Some(ListContainersOptions {
                        all: false,
                        filters: Some(filters),
                        ..Default::default()
                    }))
                    .await
                {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::debug!("Failed to list containers: {e}");
                        continue;
                    }
                };

                let mut current_ids = HashSet::new();
                let mut current_names = HashSet::new();

                for container in containers {
                    let id = match container.id {
                        Some(ref id) => id.clone(),
                        None => continue,
                    };
                    current_ids.insert(id.clone());

                    let name = container
                        .names
                        .as_ref()
                        .and_then(|n| n.first())
                        .map(|n| n.trim_start_matches('/').to_string())
                        .unwrap_or_else(|| "unknown".to_string());

                    current_names.insert(name.clone());

                    streams.entry(id.clone()).or_insert_with(|| {
                        let docker_clone = docker.clone();
                        let tx_clone = metrics_tx.clone();
                        tokio::spawn(stream_container_stats(
                            docker_clone,
                            id,
                            name,
                            tx_clone,
                        ))
                    });
                }

                streams.retain(|id, handle| {
                    if !current_ids.contains(id) {
                        handle.abort();
                        false
                    } else {
                        true
                    }
                });

                latest_metrics.retain(|name, _| current_names.contains(name));

                if !latest_metrics.is_empty() {
                    let message = build_stats_message(&latest_metrics);
                    let _ = tx.send(message);
                }
            }
            Some(metric) = metrics_rx.recv() => {
                latest_metrics.insert(metric.name.clone(), metric);
            }
        }
    }
}

async fn stream_container_stats(
    docker: Docker,
    id: String,
    name: String,
    tx: mpsc::Sender<ContainerMetrics>,
) {
    let mut stream = docker.stats(
        &id,
        Some(StatsOptions {
            stream: true,
            one_shot: false,
        }),
    );
    let mut prev_timestamp = std::time::Instant::now();
    let mut prev_net_rx = 0u64;
    let mut prev_net_tx = 0u64;
    let mut prev_blk_read = 0u64;
    let mut prev_blk_write = 0u64;

    while let Some(Ok(stats)) = stream.next().await {
        let now = std::time::Instant::now();
        let dt = now.duration_since(prev_timestamp).as_secs_f64().max(0.1);
        prev_timestamp = now;

        let cpu_percent = cpu_percent_from_stats(&stats);
        let (mem_usage_mb, mem_limit_mb, mem_percent) = memory_metrics_from_stats(&stats);
        let (net_rx, net_tx) = network_totals_from_stats(&stats);
        let (blk_read, blk_write) = block_io_totals_from_stats(&stats);
        let (net_rx_rate, net_tx_rate, blk_read_rate, blk_write_rate) = compute_rates(
            dt,
            (net_rx, net_tx, blk_read, blk_write),
            (prev_net_rx, prev_net_tx, prev_blk_read, prev_blk_write),
        );

        prev_net_rx = net_rx;
        prev_net_tx = net_tx;
        prev_blk_read = blk_read;
        prev_blk_write = blk_write;

        let metric = ContainerMetrics {
            name: name.clone(),
            cpu_percent: round2(cpu_percent),
            memory_percent: round2(mem_percent),
            memory_usage_mb: round1(mem_usage_mb),
            memory_limit_mb: round1(mem_limit_mb),
            net_rx_rate: round1(net_rx_rate),
            net_tx_rate: round1(net_tx_rate),
            block_read_rate: round1(blk_read_rate),
            block_write_rate: round1(blk_write_rate),
            status: "running".to_string(),
        };

        if tx.send(metric).await.is_err() {
            break;
        }
    }
}

fn cpu_percent_from_stats(stats: &bollard::models::ContainerStatsResponse) -> f64 {
    let cpu_delta = stats
        .cpu_stats
        .as_ref()
        .and_then(|c| c.cpu_usage.as_ref())
        .and_then(|u| u.total_usage)
        .unwrap_or(0)
        .saturating_sub(
            stats
                .precpu_stats
                .as_ref()
                .and_then(|c| c.cpu_usage.as_ref())
                .and_then(|u| u.total_usage)
                .unwrap_or(0),
        );
    let system_delta = stats
        .cpu_stats
        .as_ref()
        .and_then(|c| c.system_cpu_usage)
        .unwrap_or(0)
        .saturating_sub(
            stats
                .precpu_stats
                .as_ref()
                .and_then(|c| c.system_cpu_usage)
                .unwrap_or(0),
        );
    let online_cpus = stats
        .cpu_stats
        .as_ref()
        .and_then(|c| c.online_cpus)
        .unwrap_or(1)
        .max(1);
    if system_delta > 0 {
        (cpu_delta as f64 / system_delta as f64) * online_cpus as f64 * 100.0
    } else {
        0.0
    }
}

fn memory_metrics_from_stats(stats: &bollard::models::ContainerStatsResponse) -> (f64, f64, f64) {
    let mem_usage = stats
        .memory_stats
        .as_ref()
        .and_then(|m| m.usage)
        .unwrap_or(0);
    // P2-7: Subtract page cache from raw usage.  In cgroup v1, `usage` includes
    // the kernel page cache, inflating reported RSS by 2-4x.  The reclaimable
    // memory field varies by cgroup version:
    //   - cgroup v2: `inactive_file`
    //   - cgroup v1 (modern): `total_inactive_file`
    //   - cgroup v1 (older): `cache`
    // Priority order matters — `cache` includes active file pages on modern
    // kernels, so prefer `inactive_file` / `total_inactive_file` first.
    let cache = stats
        .memory_stats
        .as_ref()
        .and_then(|m| m.stats.as_ref())
        .and_then(|s| {
            s.get("inactive_file")
                .or_else(|| s.get("total_inactive_file"))
                .or_else(|| s.get("cache"))
                .copied()
        })
        .unwrap_or(0);
    let mem_actual = mem_usage.saturating_sub(cache);
    let mem_limit = stats
        .memory_stats
        .as_ref()
        .and_then(|m| m.limit)
        .unwrap_or(1)
        .max(1);
    let mem_usage_mb = mem_actual as f64 / (1024.0 * 1024.0);
    let mem_limit_mb = mem_limit as f64 / (1024.0 * 1024.0);
    let mem_percent = (mem_actual as f64 / mem_limit as f64) * 100.0;
    (mem_usage_mb, mem_limit_mb, mem_percent)
}

fn network_totals_from_stats(stats: &bollard::models::ContainerStatsResponse) -> (u64, u64) {
    stats
        .networks
        .as_ref()
        .map(|nets| {
            nets.values().fold((0u64, 0u64), |(rx, tx), iface| {
                (
                    rx + iface.rx_bytes.unwrap_or(0),
                    tx + iface.tx_bytes.unwrap_or(0),
                )
            })
        })
        .unwrap_or((0, 0))
}

fn block_io_totals_from_stats(stats: &bollard::models::ContainerStatsResponse) -> (u64, u64) {
    stats
        .blkio_stats
        .as_ref()
        .and_then(|b| b.io_service_bytes_recursive.as_ref())
        .map(|entries| {
            entries.iter().fold((0u64, 0u64), |(r, w), e| {
                let op = e.op.as_deref().unwrap_or("").to_lowercase();
                let val = e.value.unwrap_or(0);
                match op.as_str() {
                    "read" => (r + val, w),
                    "write" => (r, w + val),
                    _ => (r, w),
                }
            })
        })
        .unwrap_or((0, 0))
}

fn compute_rates(
    dt: f64,
    curr: (u64, u64, u64, u64),
    prev: (u64, u64, u64, u64),
) -> (f64, f64, f64, f64) {
    let (net_rx, net_tx, blk_read, blk_write) = curr;
    let (prev_net_rx, prev_net_tx, prev_blk_read, prev_blk_write) = prev;
    let net_rx_rate = (net_rx.saturating_sub(prev_net_rx) as f64 / dt).max(0.0);
    let net_tx_rate = (net_tx.saturating_sub(prev_net_tx) as f64 / dt).max(0.0);
    let blk_read_rate = (blk_read.saturating_sub(prev_blk_read) as f64 / dt).max(0.0);
    let blk_write_rate = (blk_write.saturating_sub(prev_blk_write) as f64 / dt).max(0.0);
    (net_rx_rate, net_tx_rate, blk_read_rate, blk_write_rate)
}

fn build_stats_message(metrics: &HashMap<String, ContainerMetrics>) -> String {
    let mut containers = serde_json::Map::new();
    let mut total_cpu = 0.0f64;
    let mut total_mem_pct = 0.0f64;
    let mut total_net_rx_rate = 0.0f64;
    let mut total_net_tx_rate = 0.0f64;
    let mut total_blk_read_rate = 0.0f64;
    let mut total_blk_write_rate = 0.0f64;

    for (name, m) in metrics {
        containers.insert(
            name.clone(),
            json!({
                "cpu_percent": m.cpu_percent,
                "memory_percent": m.memory_percent,
                "memory_usage_mb": m.memory_usage_mb,
                "memory_limit_mb": m.memory_limit_mb,
                "net_rx_rate": m.net_rx_rate,
                "net_tx_rate": m.net_tx_rate,
                "block_read_rate": m.block_read_rate,
                "block_write_rate": m.block_write_rate,
                "status": m.status,
            }),
        );
        total_cpu += m.cpu_percent;
        total_mem_pct += m.memory_percent;
        total_net_rx_rate += m.net_rx_rate;
        total_net_tx_rate += m.net_tx_rate;
        total_blk_read_rate += m.block_read_rate;
        total_blk_write_rate += m.block_write_rate;
    }

    let count = metrics.len().max(1) as f64;

    json!({
        "type": "stats",
        "timestamp": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64(),
        "container_count": metrics.len(),
        "containers": containers,
        "aggregate": {
            "cpu_percent": round2(total_cpu),
            "avg_cpu_percent": round2(total_cpu / count),
            "avg_memory_percent": round2(total_mem_pct / count),
            "total_net_rx_rate": round1(total_net_rx_rate),
            "total_net_tx_rate": round1(total_net_tx_rate),
            "total_net_io_rate": round1(total_net_rx_rate + total_net_tx_rate),
            "total_block_read_rate": round1(total_blk_read_rate),
            "total_block_write_rate": round1(total_blk_write_rate),
        },
    })
    .to_string()
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)]
    fn make_metrics(
        name: &str,
        cpu_percent: f64,
        memory_percent: f64,
        memory_usage_mb: f64,
        memory_limit_mb: f64,
        net_rx_rate: f64,
        net_tx_rate: f64,
        block_read_rate: f64,
        block_write_rate: f64,
    ) -> ContainerMetrics {
        ContainerMetrics {
            name: name.to_string(),
            cpu_percent,
            memory_percent,
            memory_usage_mb,
            memory_limit_mb,
            net_rx_rate,
            net_tx_rate,
            block_read_rate,
            block_write_rate,
            status: "running".to_string(),
        }
    }

    #[test]
    fn stats_single_container_aggregate() {
        let mut map = HashMap::new();
        let m = make_metrics(
            "axon-redis",
            12.34,
            45.67,
            128.0,
            512.0,
            100.0,
            50.0,
            10.0,
            5.0,
        );
        map.insert("axon-redis".to_string(), m.clone());

        let json_str = build_stats_message(&map);
        let v: serde_json::Value = serde_json::from_str(&json_str).expect("valid json");

        assert_eq!(v["container_count"], 1);
        let expected_avg_cpu = round2(m.cpu_percent);
        assert_eq!(
            v["aggregate"]["avg_cpu_percent"].as_f64().unwrap(),
            expected_avg_cpu
        );
        let expected_net_io = round1(m.net_rx_rate + m.net_tx_rate);
        assert_eq!(
            v["aggregate"]["total_net_io_rate"].as_f64().unwrap(),
            expected_net_io
        );
        assert!(v["containers"]["axon-redis"].is_object());
    }

    #[test]
    fn stats_two_containers_aggregate() {
        let mut map = HashMap::new();
        let a = make_metrics(
            "axon-postgres",
            10.0,
            30.0,
            64.0,
            256.0,
            200.0,
            100.0,
            0.0,
            0.0,
        );
        let b = make_metrics(
            "axon-redis",
            20.0,
            40.0,
            32.0,
            128.0,
            300.0,
            150.0,
            0.0,
            0.0,
        );
        map.insert("axon-postgres".to_string(), a.clone());
        map.insert("axon-redis".to_string(), b.clone());

        let json_str = build_stats_message(&map);
        let v: serde_json::Value = serde_json::from_str(&json_str).expect("valid json");

        assert_eq!(v["container_count"], 2);

        let avg_cpu = v["aggregate"]["avg_cpu_percent"].as_f64().unwrap();
        let expected_avg_cpu = round2((a.cpu_percent + b.cpu_percent) / 2.0);
        assert_eq!(avg_cpu, expected_avg_cpu);

        let total_net_io = v["aggregate"]["total_net_io_rate"].as_f64().unwrap();
        let expected_net_io = round1(a.net_rx_rate + a.net_tx_rate + b.net_rx_rate + b.net_tx_rate);
        assert_eq!(total_net_io, expected_net_io);
    }

    #[test]
    fn stats_empty_map_no_divide_by_zero() {
        let map: HashMap<String, ContainerMetrics> = HashMap::new();
        // Must not panic; len().max(1) guards division
        let json_str = build_stats_message(&map);
        let v: serde_json::Value = serde_json::from_str(&json_str).expect("valid json");
        assert_eq!(v["container_count"], 0);
        // aggregate fields must be finite (no NaN/inf from divide-by-zero)
        let avg_cpu = v["aggregate"]["avg_cpu_percent"].as_f64().unwrap();
        assert!(avg_cpu.is_finite());
    }

    #[test]
    fn round1_basic() {
        assert_eq!(round1(2.55), 2.6);
        assert_eq!(round1(0.0), 0.0);
        assert_eq!(round1(-1.567), -1.6);
    }

    #[test]
    fn round2_basic() {
        assert_eq!(round2(1.23456), 1.23);
        assert_eq!(round2(0.0), 0.0);
    }

    #[test]
    fn round1_large_value() {
        // 99999.999 rounds to 100000.0 at 1 decimal place — must not panic
        let result = round1(99999.999);
        assert_eq!(result, 100000.0);
        assert!(result.is_finite());
    }
}
