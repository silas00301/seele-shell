fn main() {
    if let Ok(cancel) = seele_runtime::process::termination_signal() {
        seele_broker::health::publish(&cancel);
    }
}
