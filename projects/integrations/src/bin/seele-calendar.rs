use seele_integrations::calendar;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    calendar::run().await;
}
