use rutable_server::App;
fn arg(name: &str) -> Option<String> {
    let args: Vec<_> = std::env::args().collect();
    args.iter().position(|v| v == name).and_then(|i| args.get(i + 1).cloned())
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = arg("--db").or_else(|| std::env::var("RUTABLE_DB").ok()).unwrap_or("rutable.sqlite3".into());
    let token = std::env::var("RUTABLE_API_TOKEN").map_err(|_| "请设置 RUTABLE_API_TOKEN（至少32个字符）")?;
    let state = App::open(&path, token).map_err(|e| e.1)?;
    if let Some(file) = arg("--import") {
        let base: rutable_model::Base = serde_json::from_slice(&std::fs::read(file)?)?;
        state.import(&arg("--document").unwrap_or("default".into()), base).map_err(|e| e.1)?;
        println!("Imported document into SQLite");
        return Ok(());
    }
    let bind = arg("--bind").or_else(|| std::env::var("RUTABLE_BIND").ok()).unwrap_or("127.0.0.1:47810".into());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    println!("Rutable API listening on {bind}; SQLite: {path}");
    axum::serve(listener, rutable_server::router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
