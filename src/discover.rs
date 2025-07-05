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

use futures::{
    future::try_join_all,
    stream::{self, StreamExt},
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::APIResource;
use kube::Client;

pub struct DiscoverClient {
    client: Client,
}

impl DiscoverClient {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    pub async fn list_api_resources(&self) -> anyhow::Result<Vec<APIResource>> {
        Ok(self
            .list_api_groups_resources()
            .await?
            .into_iter()
            .chain(self.list_core_api_resources().await?)
            // Filter out subresources.
            .filter(|resource| !resource.name.contains("/"))
            .collect())
    }

    pub async fn list_api_groups_resources(&self) -> anyhow::Result<Vec<APIResource>> {
        let groups = self.client.list_api_groups().await?.groups;
        let resources = stream::iter(groups)
            .flat_map(|group| stream::iter(group.versions))
            .then(|version| async move {
                let mut resources = self
                    .client
                    .list_api_group_resources(&version.group_version)
                    .await?;
                // NOTE: For some reason, `version` and `group` are None, so we need to set them manually.
                for resource in &mut resources.resources {
                    if let Some((group, version)) = version.group_version.split_once('/') {
                        resource.group = Some(group.to_string());
                        resource.version = Some(version.to_string());
                    }
                }
                Ok::<_, anyhow::Error>(resources)
            })
            .flat_map(|api_resource_list| {
                stream::iter(api_resource_list.unwrap_or_default().resources)
            })
            .collect::<Vec<_>>()
            .await;
        Ok(resources)
    }

    pub async fn list_core_api_resources(&self) -> anyhow::Result<Vec<APIResource>> {
        let versions = self.client.list_core_api_versions().await?.versions;

        Ok(try_join_all(versions.into_iter().map(|version| async move {
            let mut resources = self.client.list_core_api_resources(&version).await?;
            // NOTE: For some reason, `version` is None, so we need to set them manually.
            for resource in &mut resources.resources {
                resource.group = Some("core".to_string());
                resource.version = Some(version.clone());
            }
            Ok::<_, anyhow::Error>(resources)
        }))
        .await?
        .into_iter()
        .flat_map(|api_resource_list| api_resource_list.resources)
        .collect())
    }
}
