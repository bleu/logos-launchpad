use std::{env, fs::File};

use eyre::{Result, WrapErr, ensure, eyre};
use wallet::{HOME_DIR_ENV_VAR, config::WalletConfig};

pub fn required(name: &str) -> Result<String> {
    let value =
        env::var(name).wrap_err_with(|| format!("set {name} for the disposable LEZ test"))?;
    ensure!(!value.is_empty(), "set {name} for the disposable LEZ test");
    Ok(value)
}

pub fn local_wallet_config() -> Result<WalletConfig> {
    required(HOME_DIR_ENV_VAR)?;
    let path =
        wallet::utils::fetch_config_path().map_err(|err| eyre!(err.into_boxed_dyn_error()))?;
    let config: WalletConfig = serde_json::from_reader(File::open(path)?)?;
    let endpoint = &config.sequencer.sequencer_addr;
    ensure!(
        endpoint.scheme() == "http" && matches!(endpoint.host_str(), Some("127.0.0.1" | "[::1]")),
        "use a disposable wallet with a loopback HTTP sequencer"
    );
    Ok(config)
}
