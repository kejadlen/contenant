use color_eyre::eyre::Result;
use tracing_subscriber::EnvFilter;

use contenant::Contenant;

fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    Contenant::new()?.run()
}
