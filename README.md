# rules_aws_workflows

Hermetic Rust binaries and Bazel rules for interactive AWS workflows.
Typed CloudFormation codegen lives in
[rules_cloudformation](https://github.com/tomato-bazel/rules_cloudformation);
this module is the runtime layer on top of it.

It ships:

- **`//cfn_console`**: a Rust binary that drives the CloudFormation
  change-set lifecycle interactively. It prompts for each template
  Parameter in a meridian `PromptPanel`, then
  runs create → describe → confirm → execute and tails stack events until
  the stack settles. API calls go through `aws-sdk-cloudformation`, so there
  is no shelling out to the `aws` CLI.
- **`cloudformation_up_interactive`**: a rule that binds `cfn_console` to
  one rendered template and stack name as a `bazel run`-able launcher.

## Install

`.bazelrc`:

```
common --registry=https://registry.tbzl.dev/
common --registry=https://bcr.bazel.build/

# rules_aws_workflows and meridian each declare an isolated crate universe.
common --experimental_isolated_extension_usages
```

`MODULE.bazel`:

```starlark
bazel_dep(name = "rules_aws_workflows", version = "0.2.0")
bazel_dep(name = "rules_cloudformation", version = "0.10.1")
```

## Usage

```starlark
load("@rules_aws_workflows//aws/cloudformation:defs.bzl", "cloudformation_up_interactive")

cloudformation_up_interactive(
    name = "my_stack_up_i",
    stack = "//deploy/cloudformation:my_stack",  # any label producing one CFN JSON template
    stack_name = "my-stack",
    region = "us-east-1",
    capabilities = ["CAPABILITY_NAMED_IAM"],
)
```

```sh
bazel run //deploy/cloudformation:my_stack_up_i
```

Credentials come from the standard AWS provider chain at run time. The
region comes from the `region` attribute, or from the AWS config and
`AWS_REGION` when it is unset. Nothing secret goes in the BUILD file.

See [`examples/cfn_console_smoke`](examples/cfn_console_smoke/BUILD.bazel)
for a complete stack (a `cloudformation_stack` from rules_cloudformation, an
interactive deploy, and a `cloudformation_down` teardown), and
[`docs/defs.md`](docs/defs.md) for the full rule reference.

## Development

```sh
bazel build //...
bazel test //...
```

The Bazel version is pinned in `.bazelversion`. Regenerate the API docs
after changing a docstring with `bazel run //docs:update`.

## License

[MIT](LICENSE)
