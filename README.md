# rules_aws_workflows

AWS workflow tools — hermetic Rust binaries + their Bazel rules. v0.1: cfn_console (interactive CloudFormation deploys via meridian PromptPanel + aws-sdk).

## Status: v0.0.1 — scaffold

No public surface yet. See `CHANGELOG.md` for what has shipped.

## Install

`.bazelrc`:

```
common --registry=https://raw.githubusercontent.com/fastverk/bazel-registry/main/
common --registry=https://bcr.bazel.build/
```

`MODULE.bazel`:

```python
bazel_dep(name = "rules_aws_workflows", version = "0.0.1")
```
