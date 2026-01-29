use color_eyre::eyre::Result;
use tracing_subscriber::EnvFilter;

use contenant::Contenant;

fn main() -> Result<std::process::ExitCode> {
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cwd = std::env::current_dir()?;
    let exit_code = Contenant::new(&cwd)?.run()?;

    Ok(std::process::ExitCode::from(exit_code as u8))
}
