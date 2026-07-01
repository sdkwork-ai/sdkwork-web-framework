use sdkwork_web_admin_server::assemble_control_plane;
use sdkwork_web_bootstrap::WebFrameworkEnv;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    sdkwork_web_bootstrap::init_tracing_from_env();

    let env = WebFrameworkEnv::from_process_env();
    let bind = env
        .admin_bind
        .clone()
        .unwrap_or_else(|| "127.0.0.1:3920".to_owned());
    let assembly = assemble_control_plane(&env).await?;
    let router = assembly.framework.mount_admin_routes(axum::Router::new());
    let addr: SocketAddr = bind.parse()?;
    tracing::info!(%bind, "sdkwork-web-admin-server listening");
    assembly.framework.run(addr, router).await?;
    Ok(())
}
