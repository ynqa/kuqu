# kuqu

SQL for Kubernetes resources.

## Concept

*kuqu* is a tool that allows you to query Kubernetes resources using SQL-like syntax.
By leveraging [Apache DataFusion](https://datafusion.apache.org/)
(hereinafter referred to as DataFusion),
it treats resources within a Kubernetes cluster as tabular data,
enabling you to perform operations such as filtering, aggregation, and joins
using familiar SQL syntax.

While traditional kubectl commands make it difficult to search for resources
with complex conditions or perform aggregations,
*kuqu* allows you to analyze Kubernetes resources with the same intuitive approach
as working with databases.
For example, you can aggregate the number of Pods that meet specific conditions
or join and display information across multiple resource types.

## Features

- [x] High-performance query execution with DataFusion
    - [x] Querying Kubernetes resources using SQL syntax
    - [x] Automatic resource structure recognition through dynamic schema inference
    - [x] Direct access to JSON fields (e.g., `spec.nodeName`)
    - [x] JOIN operations between multiple resource types
    - [ ] Query result export functionality
    - [ ] User-defined function (UDF) definition and registration
          (is this even possible?)
- [x] Support for namespace-scoped and cluster-scoped resources
    - [ ] AllNamespace support
- [x] Custom Resource Definition (CRD) support
- [ ] Change detection (i.e., watch) support for query results
- [ ] Provide as a Rust library
- [ ] Provide as a kubectl plugin
- [ ] Query for manifest files
    - e.g. For comparison with the actual resource state
- [ ] REPL
    - [ ] Query result caching mechanism
    - [ ] Query history and favorites functionality
- [ ] Visualization (is this even possible?)
    - [ ] Colored output
    - [ ] Automatic table width adjustment
    - [ ] Table scrolling functionality
- [ ] Configuration file
    - [ ] Alias

## Installation

### Homebrew

```bash
brew install ynqa/tap/kuqu
```

### Cargo

```bash
cargo install kuqu

# Or from source (at kuqu root)
cargo install --path .
```

### Shell

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/ynqa/kuqu/releases/download/v0.1.0/kuqu-installer.sh | sh
```

## Examples

```bash
# List all running pods
kuqu "SELECT metadata.name, metadata.namespace FROM pods WHERE status.phase = 'Running'"

# List all services like redis
kuqu "SELECT metadata.name FROM services WHERE metadata.name LIKE '%redis%'"

# Example for a CRD
kuqu "SELECT metadata.name FROM 'envoyproxies.gateway.envoyproxy.io'"

# List deployments with ready replicas
kuqu "SELECT metadata.name, spec.replicas, status.readyReplicas FROM deployments 
     WHERE spec.replicas == status.readyReplicas"

# List all pods with their node names and instance types
kuqu "SELECT pod.metadata.name, pod.spec.nodeName, 
            node.metadata.labels.'node.kubernetes.io/instance-type' 
     FROM pod JOIN node ON pod.spec.nodeName == node.metadata.name"
```

## SQL Syntax

SQL syntax available in *kuqu* conforms to DataFusion.
DataFusion is a high-performance query engine that provides many extended features
in addition to standard SQL functionality.

For detailed information about supported SQL syntax,
please refer to the official DataFusion documentation:

**[DataFusion SQL Reference](https://datafusion.apache.org/user-guide/sql/index.html)**

This documentation provides detailed explanations of all available SQL features,
including SELECT statements, WHERE clauses, JOIN operations, aggregate functions,
window functions, and more.

> [!NOTE]
> *kuqu* does not comprehensively test all DataFusion SQL features,
> so some queries or syntax may not work as expected.
> If you encounter issues, please use more basic SQL syntax
> or report them as GitHub Issues.

## Schema Inference

Instead of using Kubernetes' `/openapi/v3` endpoint,
*kuqu* dynamically infers schemas from actual resource data.

The reason for this design decision is that the OpenAPI specification
cannot deterministically resolve schemas for dynamically determined keys
in fields with `additionalProperties=true`,
such as `metadata.labels` and `metadata.annotations`.
These fields allow users to add arbitrary key-value pairs at runtime,
making it difficult to define schemas in advance.

By adopting dynamic schema inference,
*kuqu* enables SQL queries against all fields that exist in actual resources
(including custom labels and annotations),
providing a more flexible and practical query experience.

However, since schemas are inferred at query execution time,
queries may take longer when there are many resources
or when resources with complex structures exist
(we are considering mechanisms to limit the number of resources
used for schema inference).

## Table Specification

In *kuqu*, Kubernetes resources are treated as SQL tables.
Table name (resource name) specification supports flexible formats.

### Table Format

```sql
-- Basic format: resource name only (uses default namespace)
SELECT * FROM pods;

-- Namespace specification: resource_name/namespace_name
SELECT * FROM 'pods/kube-system';

-- Non-namespaced resources (namespace specification is ignored)
SELECT * FROM nodes;
```

### Resource Name Formats

*kuqu* allows you to specify Kubernetes resources in multiple ways:

1. **Basic resource names**: `pods`, `deployments`, `services`
2. **Singular names**: `pod`, `deployment`, `service`
3. **Short names**: `po`, `deploy`, `svc`
4. **Group-qualified resource names**: `deployments.apps`,
   `rolebindings.rbac.authorization.k8s.io`
