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

use std::borrow::Cow;

use k8s_openapi::apimachinery::pkg::apis::meta::v1::APIResource;
use kube::{
    Resource,
    api::{ObjectMeta, TypeMeta},
    core::DynamicResourceScope,
};

/// Note about own `DynamicObject` instead of `kube::api::DynamicObject`.
/// The original `kube::api::DynamicObject` uses `kube::api::ApiResource`
/// which do not have `short_names` field compared to
/// `k8s_openapi::apimachinery::pkg::apis::meta::v1::APIResource`.
///
/// See https://github.com/kube-rs/kube/issues/1002
///
/// For this reason, even if specifying `po` -- the abbreviation for Pod -- to *eyeon*,
/// like `kubectl get po`, it cannot retrieve Pod API.
///
/// As a workaround, define own `DynamicObject` that is associated with `APIResource`.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct DynamicObject {
    /// The type fields, not always present
    #[serde(flatten, default)]
    pub types: Option<TypeMeta>,
    /// Object metadata
    #[serde(default)]
    pub metadata: ObjectMeta,

    /// All other keys
    #[serde(flatten)]
    pub data: serde_json::Value,
}

impl Resource for DynamicObject {
    type DynamicType = APIResource;
    type Scope = DynamicResourceScope;

    fn group(dt: &APIResource) -> Cow<'_, str> {
        // NOTE: If the group is "core", return empty string.
        let group = dt.group.as_deref().unwrap();
        if group == "core" {
            "".into()
        } else {
            group.into()
        }
    }

    fn version(dt: &APIResource) -> Cow<'_, str> {
        dt.version.as_deref().unwrap().into()
    }

    fn kind(dt: &APIResource) -> Cow<'_, str> {
        dt.kind.as_str().into()
    }

    fn api_version(dt: &APIResource) -> Cow<'_, str> {
        // NOTE: If the group is "core", trim the group from the apiVersion.
        if dt.group.as_deref().unwrap() == "core" {
            dt.version.as_deref().unwrap().into()
        } else {
            format!(
                "{}/{}",
                dt.group.as_deref().unwrap(),
                dt.version.as_deref().unwrap()
            )
            .into()
        }
    }

    fn plural(dt: &APIResource) -> Cow<'_, str> {
        dt.name.as_str().into()
    }

    fn meta(&self) -> &ObjectMeta {
        &self.metadata
    }

    fn meta_mut(&mut self) -> &mut ObjectMeta {
        &mut self.metadata
    }
}
