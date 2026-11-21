<!-- Generated with Stardoc: http://skydoc.bazel.build -->

`cloudformation_up_interactive` — `bazel run` wrapper that drives
the cfn_console binary against a specific cloudformation_stack
target.

Usage:

```python
load("@rules_aws_workflows//aws/cloudformation:defs.bzl",
     "cloudformation_up_interactive")

cloudformation_up_interactive(
    name = "my_stack_up_i",
    stack = "//deploy/cloudformation:my_stack",   # any label producing a CFN JSON file
    stack_name = "my-stack",
    region = "us-east-1",
    capabilities = ["CAPABILITY_NAMED_IAM"],
)
```

`bazel run //deploy/cloudformation:my_stack_up_i` launches the
console: prompts for each Parameter via meridian, creates the
change set, shows the resource-change diff, asks for confirmation,
then executes and tails events.

No region, no parameter overrides, no static credentials in the
BUILD file — cfn_console reads creds from the standard AWS chain
at runtime and the region from --region (this rule's attr) or the
ambient AWS_REGION env var.

<a id="cloudformation_up_interactive"></a>

## cloudformation_up_interactive

<pre>
load("@rules_aws_workflows//aws/cloudformation:defs.bzl", "cloudformation_up_interactive")

cloudformation_up_interactive(<a href="#cloudformation_up_interactive-name">name</a>, <a href="#cloudformation_up_interactive-capabilities">capabilities</a>, <a href="#cloudformation_up_interactive-region">region</a>, <a href="#cloudformation_up_interactive-stack">stack</a>, <a href="#cloudformation_up_interactive-stack_name">stack_name</a>)
</pre>

Interactive CFN deploy driver. Wraps //cfn_console:cfn_console with a target's rendered template + a stack name. Prompts for Parameter values via meridian, runs CreateChangeSet → ExecuteChangeSet, tails events.

**ATTRIBUTES**


| Name  | Description | Type | Mandatory | Default |
| :------------- | :------------- | :------------- | :------------- | :------------- |
| <a id="cloudformation_up_interactive-name"></a>name |  A unique name for this target.   | <a href="https://bazel.build/concepts/labels#target-names">Name</a> | required |  |
| <a id="cloudformation_up_interactive-capabilities"></a>capabilities |  CFN capabilities (`CAPABILITY_IAM`, `CAPABILITY_NAMED_IAM`, `CAPABILITY_AUTO_EXPAND`).   | List of strings | optional |  `[]`  |
| <a id="cloudformation_up_interactive-region"></a>region |  AWS region. If unset, cfn_console resolves it from the AWS config / AWS_REGION env var.   | String | optional |  `""`  |
| <a id="cloudformation_up_interactive-stack"></a>stack |  Any label whose DefaultInfo emits a single CFN JSON template (e.g. a `cloudformation_stack` target from rules_cloudformation).   | <a href="https://bazel.build/concepts/labels">Label</a> | required |  |
| <a id="cloudformation_up_interactive-stack_name"></a>stack_name |  CFN stack name. Defaults to the rule's label name.   | String | optional |  `""`  |


