use seele_integrations::weather;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    weather::run().await;
}
