// Copyright 2025 kuqu Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{collections::HashMap, sync::Arc};

use clap::Parser;
use datafusion::{
    catalog::{DynamicFileCatalog, UrlTableFactory},
    execution::context::SessionContext,
    prelude::SessionConfig,
};
use kubex::{
    discover::DiscoverClient,
    kube::{
        Client, Config,
        config::{KubeConfigOptions, Kubeconfig},
    },
};

mod provider;
mod url;

use crate::provider::KubernetesTableProviderFactory;

/// Query Kubernetes resources using SQL-like syntax.
#[derive(Parser)]
#[command(name = "kuqu", version)]
pub struct Args {
    #[arg(long = "context", help = "Kubernetes context.")]
    pub context: Option<String>,

    /// The SQL-like query to execute against Kubernetes resources.
    /// See https://datafusion.apache.org/user-guide/sql/index.html
    /// for more details on the query syntax.
    pub query: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let context = kubex::determine_context(&args.context)?;

    let kubeconfig = Kubeconfig::read()?;
    let options = KubeConfigOptions {
        context: Some(context.clone()),
        ..Default::default()
    };
    let config = Config::from_custom_kubeconfig(kubeconfig, &options).await?;
    let client = Client::try_from(config)?;

    let discover_client = DiscoverClient::new(client.clone());
    let api_resources = discover_client.list_api_resources().await?;

    let factory = Arc::new(KubernetesTableProviderFactory::new(
        client,
        context,
        api_resources,
    ));
    let ctx = SessionContext::new();
    let catalog_list = Arc::new(DynamicFileCatalog::new(
        Arc::clone(ctx.state().catalog_list()),
        factory as Arc<dyn UrlTableFactory>,
    ));
    let ctx: SessionContext = ctx
        .into_state_builder()
        .with_config(SessionConfig::from_string_hash_map(&HashMap::from([(
            // To avoid e.g. spec.nodeName => spec.nodename normalization in DataFusion SQL parser
            "datafusion.sql_parser.enable_ident_normalization".to_owned(),
            "false".to_owned(),
        )]))?)
        .with_catalog_list(catalog_list)
        .build()
        .into();

    let df = ctx.sql(&args.query).await?;
    df.show().await?;
    Ok(())
}
