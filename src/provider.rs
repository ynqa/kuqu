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

use std::{any::Any, fmt::Debug, io::Cursor, sync::Arc};

use async_trait::async_trait;
use datafusion::{
    arrow::{
        compute::concat_batches,
        datatypes::SchemaRef,
        json::{ReaderBuilder, reader::infer_json_schema},
        record_batch::RecordBatch,
    },
    catalog::{Session, UrlTableFactory},
    common::{DataFusionError, Result as DataFusionResult},
    datasource::{TableProvider, TableType},
    execution::context::TaskContext,
    logical_expr::Expr,
    physical_expr::EquivalenceProperties,
    physical_plan::{
        DisplayAs, DisplayFormatType, ExecutionPlan, Partitioning, PlanProperties,
        SendableRecordBatchStream,
        execution_plan::{Boundedness, EmissionType},
        memory::MemoryStream,
    },
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::APIResource;
use kube::{Api, Client, api::ObjectList};

use crate::{dynamic::DynamicObject, url::KubernetesUrl};

/// Infer schema from NDJSON
async fn infer_schema(ndjson: &str) -> DataFusionResult<SchemaRef> {
    // TODO: make it configurable to adjust the number of records used for schema inference
    infer_json_schema(&mut Cursor::new(ndjson.as_bytes()), None)
        .map(|(schema, _)| Arc::new(schema))
        .map_err(|e| DataFusionError::External(Box::new(e)))
}

/// Factory for creating Kubernetes table providers
pub struct KubernetesTableProviderFactory {
    client: Client,
    context: String,
    api_resources: Vec<APIResource>,
}

impl Debug for KubernetesTableProviderFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "KubernetesTableProviderFactory")
    }
}

impl KubernetesTableProviderFactory {
    pub fn new(client: Client, context: String, api_resources: Vec<APIResource>) -> Self {
        Self {
            client,
            context,
            api_resources,
        }
    }

    /// List API resources for a given resource type and namespace
    async fn list_api_resources(
        &self,
        api_resource: &APIResource,
        namespace: &str,
    ) -> DataFusionResult<ObjectList<DynamicObject>> {
        let api: Api<DynamicObject> = if api_resource.namespaced {
            Api::namespaced_with(self.client.clone(), namespace, api_resource)
        } else {
            Api::all_with(self.client.clone(), api_resource)
        };

        api.list(&Default::default())
            .await
            .map(|mut list| {
                list.items.iter_mut().for_each(|item| {
                    // TODO: re-consider whether to remove managedFields or not?
                    item.metadata.managed_fields = None;
                });
                list
            })
            .map_err(|e| DataFusionError::External(Box::new(e)))
    }
}

#[async_trait]
impl UrlTableFactory for KubernetesTableProviderFactory {
    /// Try to create a table provider from a Kubernetes URL
    async fn try_new(&self, url: &str) -> DataFusionResult<Option<Arc<dyn TableProvider>>> {
        let kubeurl =
            KubernetesUrl::parse(url, &self.context, &self.api_resources).map_err(|e| {
                DataFusionError::Plan(format!("Invalid Kubernetes URL '{}': {}", url, e))
            })?;

        let object_list = self
            .list_api_resources(&kubeurl.resource, &kubeurl.namespace)
            .await?;

        if object_list.items.is_empty() {
            return Err(DataFusionError::Plan(format!(
                "No items found for resource '{}' in namespace '{}'",
                kubeurl.resource.kind, kubeurl.namespace
            )));
        }

        let ndjson = object_list
            .items
            .iter()
            .map(|item| serde_json::json!(item).to_string())
            .collect::<Vec<_>>()
            .join("\n");

        let schema = infer_schema(&ndjson).await?;

        Ok(Some(Arc::new(KubernetesTableProvider::new(
            schema,
            Arc::new(ndjson),
        ))))
    }
}

#[derive(Clone, Debug)]
pub struct KubernetesTableProvider {
    schema: SchemaRef,
    ndjson: Arc<String>,
}

impl KubernetesTableProvider {
    pub fn new(schema: SchemaRef, ndjson: Arc<String>) -> Self {
        Self { schema, ndjson }
    }
}

#[async_trait]
impl TableProvider for KubernetesTableProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    async fn scan(
        &self,
        _state: &dyn Session,
        projection: Option<&Vec<usize>>,
        _filters: &[Expr],
        _limit: Option<usize>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        let projected_schema = if let Some(proj) = projection {
            match self.schema.project(proj) {
                Ok(projected_schema) => Arc::new(projected_schema),
                Err(_) => self.schema.clone(),
            }
        } else {
            self.schema.clone()
        };

        Ok(Arc::new(KubernetesExec::new(
            projected_schema,
            self.ndjson.clone(),
        )))
    }
}

/// Convert NDJSON to a DataFusion RecordBatch
fn record_batch_from_ndjson(ndjson: &str, schema: SchemaRef) -> DataFusionResult<RecordBatch> {
    let reader = ReaderBuilder::new(schema.clone())
        // TODO: make it configurable?
        .with_batch_size(4096)
        .with_coerce_primitive(true)
        .build(Cursor::new(ndjson.as_bytes()))?;

    let mut batches: Vec<RecordBatch> = Vec::new();

    for batch_result in reader {
        match batch_result {
            Ok(batch) => {
                batches.push(batch);
            }
            Err(e) => {
                if !batches.is_empty() {
                    break;
                } else {
                    return Err(DataFusionError::External(Box::new(e)));
                }
            }
        }
    }

    if batches.is_empty() {
        Ok(RecordBatch::new_empty(schema))
    } else {
        concat_batches(&schema, &batches).map_err(DataFusionError::from)
    }
}

#[derive(Debug)]
struct KubernetesExec {
    properties: PlanProperties,
    schema: SchemaRef,
    ndjson: Arc<String>,
}

impl KubernetesExec {
    fn new(schema: SchemaRef, ndjson: Arc<String>) -> Self {
        // TODO: properties set here are not refined. There is room for optimization.
        let properties = PlanProperties::new(
            EquivalenceProperties::new(schema.clone()),
            Partitioning::UnknownPartitioning(1),
            EmissionType::Final,
            Boundedness::Bounded,
        );
        Self {
            properties,
            schema,
            ndjson,
        }
    }
}

impl DisplayAs for KubernetesExec {
    fn fmt_as(&self, _t: DisplayFormatType, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "KubernetesExec")
    }
}

impl ExecutionPlan for KubernetesExec {
    fn name(&self) -> &str {
        "KubernetesExec"
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn properties(&self) -> &PlanProperties {
        &self.properties
    }

    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        Vec::new()
    }

    fn with_new_children(
        self: Arc<Self>,
        _: Vec<Arc<dyn ExecutionPlan>>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        Ok(self)
    }

    fn execute(
        &self,
        _partition: usize,
        _context: Arc<TaskContext>,
    ) -> DataFusionResult<SendableRecordBatchStream> {
        Ok(Box::pin(MemoryStream::try_new(
            vec![record_batch_from_ndjson(&self.ndjson, self.schema.clone())?],
            self.schema.clone(),
            None,
        )?))
    }
}
