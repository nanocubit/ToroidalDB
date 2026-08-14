use lazy_static::lazy_static;
use prometheus::{Encoder, IntCounterVec, Opts, Registry, TextEncoder};

lazy_static! {
    // Создаем реестр ОДИН раз
    pub static ref REGISTRY: Registry = Registry::new();

    // Создаем и РЕГИСТРИРУЕМ счетчик в реестре
    pub static ref HTTP_COUNTER: IntCounterVec = {
        let counter = IntCounterVec::new(
            Opts::new("http_requests_total", "HTTP requests by method and status"),
            &["method", "status"]
        ).unwrap();
        // КРИТИЧЕСКИ ВАЖНО: регистрируем счетчик в реестре
        REGISTRY.register(Box::new(counter.clone())).unwrap();
        counter
    };
}

pub fn metrics_handler() -> String {
    let encoder = TextEncoder::new();
    let mut buffer = Vec::new();
    encoder.encode(&REGISTRY.gather(), &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}

pub fn increment(method: &str, status: &str) {
    HTTP_COUNTER.with_label_values(&[method, status]).inc();
}
