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
