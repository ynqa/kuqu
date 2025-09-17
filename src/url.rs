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

use std::fmt;

use kubex::k8s_openapi::apimachinery::pkg::apis::meta::v1::APIResource;

/// Structure representing a Kubernetes resource URL
#[derive(Debug, Clone, PartialEq)]
pub struct KubernetesUrl {
    /// Resource type (e.g., "pod", "node", "service")
    pub resource: APIResource,
    /// Namespace (if specified)
    pub namespace: String,
}

impl KubernetesUrl {
    /// Parse URL string to create KubernetesUrl
    ///
    /// Supported formats:
    /// - `pod` => pod in default namespace
    /// - `pod/something` => Pod in "something" namespace
    /// - `node/something` => For non-namespaced resources, namespace is ignored
    pub fn parse(
        url: &str,
        context: &str,
        api_resources: &[APIResource],
    ) -> Result<Self, ParseError> {
        if url.is_empty() {
            return Err(ParseError::EmptyUrl);
        }

        let parts: Vec<&str> = url.split('/').collect();

        let (resource, namespace) = match parts.len() {
            1 => {
                let resource = parts[0].to_string();
                (resource, kubex::determine_namespace(None, context))
            }
            2 => {
                // Format like "pod/something"
                let resource = parts[0].to_string();
                let namespace = parts[1].to_string();

                (resource, namespace)
            }
            _ => return Err(ParseError::InvalidFormat(url.to_string())),
        };

        // Check if resource exists and retrieve it
        let api_resource = match kubex::find_resource(&resource, api_resources) {
            Some(res) => res,
            None => return Err(ParseError::ResourceNotFound(resource)),
        };

        Ok(KubernetesUrl {
            resource: api_resource,
            namespace,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    EmptyUrl,
    InvalidFormat(String),
    ResourceNotFound(String),
}

const SUPPORTED_FORMATS: &str = "Supported formats:
- `pod` => pod in default namespace
- `pod/namespace` => Pod in `something` namespace
- `node/something` => For non-namespaced resources, namespace is ignored";

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::EmptyUrl => write!(f, "URL is empty"),
            ParseError::InvalidFormat(url) => {
                write!(f, "Invalid URL format: {}\n\n{}", url, SUPPORTED_FORMATS)
            }
            ParseError::ResourceNotFound(resource) => {
                write!(f, "Resource '{}' not found", resource)
            }
        }
    }
}
