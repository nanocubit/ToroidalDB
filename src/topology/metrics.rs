use lazy_static::lazy_static;
use prometheus::{opts, Gauge, Histogram, HistogramOpts, IntCounterVec, Registry, TextEncoder};

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();
    pub static ref REQUEST_COUNT: IntCounterVec = IntCounterVec::new(
        opts!("request_count_total", "Number of requests processed"),
        &["method", "endpoint", "status"]
    )
    .unwrap();
    pub static ref REQUEST_DURATION: Histogram = Histogram::with_opts(HistogramOpts::new(
        "request_duration",
        "Request duration in seconds"
    ))
    .unwrap();
    pub static ref NODE_COUNT: Gauge =
        Gauge::new("node_count", "Current number of nodes in the database").unwrap();
    pub static ref SHARD_COUNT: Gauge =
        Gauge::new("shard_count", "Current number of shards").unwrap();
    pub static ref CACHE_HIT_RATE: Gauge =
        Gauge::new("cache_hit_rate", "Cache hit rate as a percentage").unwrap();
}

pub fn increment_request_count(method: &str, endpoint: &str, status: &str) {
    REQUEST_COUNT
        .with_label_values(&[method, endpoint, status])
        .inc();
}

// Регистрируем метрики в реестре
pub fn init_metrics() {
    REGISTRY.register(Box::new(REQUEST_COUNT.clone())).unwrap();
    REGISTRY
        .register(Box::new(REQUEST_DURATION.clone()))
        .unwrap();
    REGISTRY.register(Box::new(NODE_COUNT.clone())).unwrap();
    REGISTRY.register(Box::new(SHARD_COUNT.clone())).unwrap();
    REGISTRY.register(Box::new(CACHE_HIT_RATE.clone())).unwrap();
}

pub fn observe_request_duration(duration: f64) {
    REQUEST_DURATION.observe(duration);
}

pub fn set_node_count(count: f64) {
    NODE_COUNT.set(count);
}

pub fn set_shard_count(count: f64) {
    SHARD_COUNT.set(count);
}

pub fn set_cache_hit_rate(rate: f64) {
    CACHE_HIT_RATE.set(rate);
}

pub fn metrics_handler() -> String {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    encoder
        .encode_to_string(&metric_families)
        .unwrap_or_else(|_| "# Error gathering metrics".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collection() {
        init_metrics(); // Инициализируем метрики
        increment_request_count("GET", "/health", "200");
        observe_request_duration(0.123);
        set_node_count(100.0);
        set_shard_count(3.0);
        set_cache_hit_rate(0.85);

        let metrics_output = metrics_handler();
        assert!(metrics_output.contains("request_count_total"));
        assert!(metrics_output.contains("node_count"));
        assert!(metrics_output.contains("shard_count"));

        println!("✅ Metrics collection test passed");
    }
}
