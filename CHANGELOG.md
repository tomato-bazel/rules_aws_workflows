# Changelog

All notable changes to rules_aws_workflows. The format is loosely
[Keep a Changelog](https://keepachangelog.com/); version headers mirror
the published registry entries.

## 0.2.0

### Changed

- Resolve dependencies from the public registry at
  `https://registry.tbzl.dev/`, then the Bazel Central Registry.
- `meridian` 0.2.0 → 0.2.1. 0.2.0 was never published, so 0.1.1 could not
  resolve.
- `rules_cloudformation` 0.7.0 → 0.10.1.
- The smoke example loads `cloudformation_up_interactive` from this module
  (`//aws/cloudformation:defs.bzl`). It no longer comes from
  rules_cloudformation's `interactive.bzl`, which that module does not ship.
- CI runs `bazel build //...` plus the docs freshness test. There are no
  other test targets yet.

### Added

- `.bazelversion` pinning Bazel 9.1.0.
- `docs/defs.md`: stardoc reference for `aws/cloudformation:defs.bzl`, with
  a `diff_test` that fails when it goes stale.
- Dev-only hermetic C/C++ toolchain (`hermetic_cc_toolchain`, Zig) for root
  builds on Linux hosts without a system compiler.

### Removed

- `tools/credhelper/gh-cred-helper.sh`. Every dependency now resolves from
  public registries, so no credential helper is needed.

## 0.1.1

- Pin `meridian` 0.2.0 and drop the dev-only `local_path_override`s.

## 0.1.0

- `cfn_console`: interactive CloudFormation change-set driver.
- `cloudformation_up_interactive` rule.
