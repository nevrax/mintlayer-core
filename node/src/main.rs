// Copyright (c) 2022 RBB S.r.l
// opensource@mintlayer.org
// SPDX-License-Identifier: MIT
// Licensed under the MIT License;
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://spdx.org/licenses/MIT
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Top-level node binary

use std::fs;

use anyhow::{anyhow, Context, Result};

use common::chain::{config::ChainType, ChainConfig};
use logging::log;
use node::{Command, Config, Options};

async fn run() -> Result<()> {
    let opts = Options::from_args(std::env::args_os());
    logging::init_logging(opts.log_path.as_ref());
    log::trace!("Command line options: {opts:?}");

    match opts.command {
        Command::CreateConfig { path, net } => {
            let config = Config::new(net)?;
            println!("{config:#?}");
            let config = toml::to_string(&config).context("Failed to serialize config")?;
            log::trace!("Saving config to {path:?}\n: {config:#?}");
            fs::write(path, config).context("Failed to write config")?;
            Ok(())
        }
        Command::Run(options) => {
            let config = Config::read(options).context("Failed to initialize config")?;
            if config.chain_type != ChainType::Mainnet && config.chain_type != ChainType::Regtest {
                return Err(anyhow!(
                    "Chain type '{:?}' not yet supported",
                    config.chain_type
                ));
            }
            let chain_config = ChainConfig::new(config.chain_type);
            log::trace!("Starting with the following config\n: {config:#?}");
            node::run(chain_config, config).await
        }
    }
}

#[tokio::main]
async fn main() {
    run().await.unwrap_or_else(|err| {
        eprintln!("ERROR: {:?}", err);
        std::process::exit(1)
    })
}
