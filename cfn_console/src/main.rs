// cfn_console — interactive CloudFormation deploy driver.
//
// Reads a rendered CFN template, prompts for Parameter values
// through meridian's PromptPanel, runs the change-set lifecycle
// (CreateChangeSet → DescribeChangeSet → ExecuteChangeSet), and
// tails stack events until the stack settles.
//
// Hermetic by design: aws-sdk-cloudformation handles the API
// calls directly (no shelling out to `aws`), and the prompt UI
// uses meridian + ratatui in-process. The only ambient
// requirement is AWS credentials resolvable via the standard
// provider chain.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use aws_sdk_cloudformation::types::{
    Capability, ChangeSetType, Parameter as CfnParameter, StackStatus,
};
use aws_sdk_cloudformation::Client;
use clap::Parser;
use meridian_tui::{render_prompt, FieldValue, PromptResponse};
use meridian_uiview::proto::{
    form_field::Kind, EnumSelection, FormField, IntegerSpinner, MaskedInput, PanelDescriptor,
    PromptPanel, TextInput,
};
use serde::Deserialize;

#[derive(Parser, Debug)]
#[command(
    about = "Interactive CloudFormation deploy. Reads a rendered template, prompts for Parameters, drives the change-set lifecycle."
)]
struct Args {
    /// Path to the rendered CFN template JSON.
    #[arg(long)]
    template: PathBuf,

    /// CFN stack name.
    #[arg(long)]
    stack_name: String,

    /// AWS region.
    #[arg(long)]
    region: Option<String>,

    /// CFN capabilities (CAPABILITY_IAM, CAPABILITY_NAMED_IAM,
    /// CAPABILITY_AUTO_EXPAND). Repeatable.
    #[arg(long = "capability")]
    capabilities: Vec<String>,
}

/// One entry from the CFN template's Parameters block. We only
/// model the fields the prompt builder cares about; unknown fields
/// are ignored.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct CfnTemplateParameter {
    #[serde(rename = "Type")]
    type_: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    default: Option<serde_json::Value>,
    #[serde(default)]
    allowed_values: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    allowed_pattern: Option<String>,
    #[serde(default)]
    constraint_description: Option<String>,
    #[serde(default)]
    min_length: Option<i32>,
    #[serde(default)]
    max_length: Option<i32>,
    #[serde(default)]
    min_value: Option<i32>,
    #[serde(default)]
    max_value: Option<i32>,
    #[serde(default)]
    no_echo: Option<bool>,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    let args = Args::parse();

    // 1. Load the template + extract Parameters.
    let raw = std::fs::read_to_string(&args.template)
        .with_context(|| format!("reading template {}", args.template.display()))?;
    // CFN templates use PascalCase keys; serde_json + rename_all
    // = "PascalCase" handles per-struct. But the top-level
    // `Parameters` key is also PascalCase — the outer struct's
    // default-camelCase derive expects "parameters". Patch with
    // explicit serde rename via the lowercased copy:
    let template_value: serde_json::Value =
        serde_json::from_str(&raw).context("parsing template JSON")?;
    let parameters = extract_parameters(&template_value)?;

    // 2. Prompt for values (if any).
    let parameter_values = if parameters.is_empty() {
        eprintln!("cfn_console: template has no Parameters; no prompt needed.");
        HashMap::new()
    } else {
        let panel = build_parameter_prompt(&parameters);
        match render_prompt(&panel)? {
            PromptResponse::Submitted(values) => values,
            PromptResponse::Cancelled => {
                eprintln!("cfn_console: cancelled.");
                std::process::exit(2);
            }
            PromptResponse::Confirmed(_) => unreachable!("form mode returns Submitted, not Confirmed"),
        }
    };

    // 3. AWS client.
    let mut config_loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
    if let Some(region) = &args.region {
        config_loader = config_loader.region(aws_config::Region::new(region.clone()));
    }
    let config = config_loader.load().await;
    let client = Client::new(&config);

    // 4. Decide CREATE vs UPDATE.
    let stack_exists = stack_exists(&client, &args.stack_name).await?;
    let change_set_type = if stack_exists {
        ChangeSetType::Update
    } else {
        ChangeSetType::Create
    };
    eprintln!(
        "cfn_console: stack '{}' {} — creating {} change set.",
        args.stack_name,
        if stack_exists { "exists" } else { "does not exist" },
        if stack_exists { "UPDATE" } else { "CREATE" },
    );

    // 5. Build + submit the change set.
    let change_set_name = format!("cfn-console-{}", chrono_like_timestamp());
    let cfn_parameters: Vec<CfnParameter> = parameter_values
        .iter()
        .map(|(k, v)| {
            CfnParameter::builder()
                .parameter_key(k)
                .parameter_value(v.as_string())
                .build()
        })
        .collect();

    let capabilities = parse_capabilities(&args.capabilities)?;

    let _ = client
        .create_change_set()
        .stack_name(&args.stack_name)
        .change_set_name(&change_set_name)
        .change_set_type(change_set_type.clone())
        .template_body(raw.clone())
        .set_parameters(Some(cfn_parameters))
        .set_capabilities(Some(capabilities))
        .send()
        .await
        .context("CreateChangeSet")?;

    // 6. Wait for change set to be ready.
    let described = wait_for_change_set(&client, &args.stack_name, &change_set_name).await?;

    // 7. Confirmation prompt.
    let detail = render_change_set_detail(&described);
    let confirm = PanelDescriptor {
        panel_id: "cfn-confirm".into(),
        title: format!("Apply change set to {}?", args.stack_name),
        body: Some(meridian_uiview::proto::panel_descriptor::Body::Prompt(
            PromptPanel {
                description: format!(
                    "Change set: {change_set_name}\nStack: {}\nType: {:?}",
                    args.stack_name, change_set_type
                ),
                fields: vec![],
                is_confirmation: true,
                accept_label: "Apply".into(),
                cancel_label: "Discard".into(),
                detail,
            },
        )),
    };
    // We only need the inner PromptPanel for render_prompt; the
    // wrapping PanelDescriptor is for future PanelBundle archiving.
    let confirm_panel = match &confirm.body {
        Some(meridian_uiview::proto::panel_descriptor::Body::Prompt(p)) => p.clone(),
        _ => unreachable!(),
    };

    match render_prompt(&confirm_panel)? {
        PromptResponse::Confirmed(true) => {}
        PromptResponse::Confirmed(false) | PromptResponse::Cancelled => {
            eprintln!("cfn_console: not applying. Deleting change set.");
            let _ = client
                .delete_change_set()
                .change_set_name(&change_set_name)
                .stack_name(&args.stack_name)
                .send()
                .await;
            std::process::exit(0);
        }
        PromptResponse::Submitted(_) => unreachable!("confirmation mode returns Confirmed"),
    }

    // 8. Execute.
    client
        .execute_change_set()
        .change_set_name(&change_set_name)
        .stack_name(&args.stack_name)
        .send()
        .await
        .context("ExecuteChangeSet")?;
    eprintln!("cfn_console: change set submitted. Tailing stack events…");

    // 9. Tail events until stack settles.
    tail_events_until_settled(&client, &args.stack_name).await?;

    Ok(())
}

/// Build a meridian PromptPanel from the template's Parameters.
fn build_parameter_prompt(params: &BTreeMap<String, CfnTemplateParameter>) -> PromptPanel {
    let fields: Vec<FormField> = params
        .iter()
        .map(|(name, p)| {
            let kind = if let Some(allowed) = &p.allowed_values {
                Kind::EnumSelection(EnumSelection {
                    allowed_values: allowed
                        .iter()
                        .map(|v| value_to_string(v))
                        .collect(),
                    default_value: p
                        .default
                        .as_ref()
                        .map(value_to_string)
                        .unwrap_or_default(),
                })
            } else if p.no_echo.unwrap_or(false) {
                Kind::Masked(MaskedInput {
                    default_value: p
                        .default
                        .as_ref()
                        .map(value_to_string)
                        .unwrap_or_default(),
                    pattern: p.allowed_pattern.clone().unwrap_or_default(),
                    pattern_error_msg: p.constraint_description.clone().unwrap_or_default(),
                    min_length: p.min_length.unwrap_or(0),
                    max_length: p.max_length.unwrap_or(0),
                })
            } else if p.type_ == "Number" {
                Kind::Integer(IntegerSpinner {
                    min: p.min_value.unwrap_or(0),
                    max: p.max_value.unwrap_or(0),
                    default_value: p
                        .default
                        .as_ref()
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0) as i32,
                    step: 1,
                })
            } else {
                Kind::Text(TextInput {
                    default_value: p
                        .default
                        .as_ref()
                        .map(value_to_string)
                        .unwrap_or_default(),
                    pattern: p.allowed_pattern.clone().unwrap_or_default(),
                    pattern_error_msg: p.constraint_description.clone().unwrap_or_default(),
                    min_length: p.min_length.unwrap_or(0),
                    max_length: p.max_length.unwrap_or(0),
                })
            };
            FormField {
                field_id: name.clone(),
                label: name.clone(),
                request_field: String::new(),
                description: p.description.clone().unwrap_or_default(),
                kind: Some(kind),
            }
        })
        .collect();

    PromptPanel {
        description: "Provide values for the stack's Parameters.".into(),
        fields,
        is_confirmation: false,
        accept_label: "Continue".into(),
        cancel_label: "Cancel".into(),
        detail: String::new(),
    }
}

fn value_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn extract_parameters(
    template: &serde_json::Value,
) -> Result<BTreeMap<String, CfnTemplateParameter>> {
    let Some(params) = template.get("Parameters") else {
        return Ok(BTreeMap::new());
    };
    let map: BTreeMap<String, CfnTemplateParameter> = serde_json::from_value(params.clone())
        .context("parsing Parameters block")?;
    Ok(map)
}

fn parse_capabilities(raw: &[String]) -> Result<Vec<Capability>> {
    raw.iter()
        .map(|c| match c.as_str() {
            "CAPABILITY_IAM" => Ok(Capability::CapabilityIam),
            "CAPABILITY_NAMED_IAM" => Ok(Capability::CapabilityNamedIam),
            "CAPABILITY_AUTO_EXPAND" => Ok(Capability::CapabilityAutoExpand),
            other => bail!("unknown capability {other:?}"),
        })
        .collect()
}

async fn stack_exists(client: &Client, name: &str) -> Result<bool> {
    let res = client.describe_stacks().stack_name(name).send().await;
    match res {
        Ok(_) => Ok(true),
        Err(e) => {
            // CFN returns ValidationError with "does not exist"
            // when the stack is missing — string-matching the
            // message is the documented detection technique.
            let msg = e.to_string();
            if msg.contains("does not exist") {
                Ok(false)
            } else {
                Err(e).context("DescribeStacks")?
            }
        }
    }
}

async fn wait_for_change_set(
    client: &Client,
    stack_name: &str,
    change_set_name: &str,
) -> Result<aws_sdk_cloudformation::operation::describe_change_set::DescribeChangeSetOutput> {
    for _ in 0..120 {
        let out = client
            .describe_change_set()
            .stack_name(stack_name)
            .change_set_name(change_set_name)
            .send()
            .await
            .context("DescribeChangeSet")?;
        match out.status() {
            Some(aws_sdk_cloudformation::types::ChangeSetStatus::CreateComplete) => return Ok(out),
            Some(aws_sdk_cloudformation::types::ChangeSetStatus::Failed) => {
                bail!(
                    "change set failed: {}",
                    out.status_reason().unwrap_or("(no reason)")
                );
            }
            _ => tokio::time::sleep(Duration::from_secs(2)).await,
        }
    }
    bail!("timed out waiting for change set")
}

fn render_change_set_detail(
    set: &aws_sdk_cloudformation::operation::describe_change_set::DescribeChangeSetOutput,
) -> String {
    let mut lines = Vec::new();
    lines.push("Resource changes:".to_string());
    for c in set.changes() {
        if let Some(rc) = c.resource_change() {
            let action = rc.action().map(|a| a.as_str()).unwrap_or("?");
            let kind = rc.resource_type().unwrap_or("?");
            let id = rc.logical_resource_id().unwrap_or("?");
            lines.push(format!("  {action:7} {kind:50} {id}"));
        }
    }
    if lines.len() == 1 {
        lines.push("  (no resource-level changes)".into());
    }
    lines.join("\n")
}

async fn tail_events_until_settled(client: &Client, stack_name: &str) -> Result<()> {
    let mut seen_event_ids: std::collections::HashSet<String> = Default::default();
    loop {
        let events = client
            .describe_stack_events()
            .stack_name(stack_name)
            .send()
            .await
            .context("DescribeStackEvents")?;
        // Events come back newest-first; iterate in reverse so
        // we print in chronological order.
        let mut new_events: Vec<_> = events
            .stack_events()
            .iter()
            .filter(|e| {
                e.event_id().is_some_and(|id| !seen_event_ids.contains(id))
            })
            .collect();
        new_events.reverse();
        for e in &new_events {
            if let Some(id) = e.event_id() {
                seen_event_ids.insert(id.to_string());
            }
            eprintln!(
                "  [{:24}] {:30} {:24} {}",
                e.timestamp()
                    .map(|t| t.to_string())
                    .unwrap_or_default(),
                e.logical_resource_id().unwrap_or("?"),
                e.resource_status()
                    .map(|s| s.as_str())
                    .unwrap_or("?"),
                e.resource_status_reason().unwrap_or(""),
            );
        }

        let stacks = client
            .describe_stacks()
            .stack_name(stack_name)
            .send()
            .await
            .context("DescribeStacks")?;
        if let Some(stack) = stacks.stacks().first() {
            if let Some(status) = stack.stack_status() {
                if is_terminal_status(status) {
                    eprintln!("cfn_console: stack settled — {:?}", status);
                    if is_failure_status(status) {
                        std::process::exit(1);
                    }
                    return Ok(());
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

fn is_terminal_status(s: &StackStatus) -> bool {
    matches!(
        s,
        StackStatus::CreateComplete
            | StackStatus::UpdateComplete
            | StackStatus::DeleteComplete
            | StackStatus::CreateFailed
            | StackStatus::UpdateFailed
            | StackStatus::UpdateRollbackComplete
            | StackStatus::UpdateRollbackFailed
            | StackStatus::RollbackComplete
            | StackStatus::RollbackFailed
            | StackStatus::DeleteFailed
    )
}

fn is_failure_status(s: &StackStatus) -> bool {
    matches!(
        s,
        StackStatus::CreateFailed
            | StackStatus::UpdateFailed
            | StackStatus::UpdateRollbackFailed
            | StackStatus::RollbackFailed
            | StackStatus::DeleteFailed
            | StackStatus::RollbackComplete
            | StackStatus::UpdateRollbackComplete
    )
}

/// Stable, monotonically-increasing-ish suffix for change-set
/// names. We don't pull in `chrono` for this; nanosecond clock
/// time is enough since change sets only need uniqueness within
/// a stack.
fn chrono_like_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos}")
}

/// FieldValue extension methods used inline above. (No-op here;
/// just keeps the dependency direction explicit.)
#[allow(dead_code)]
fn _force_meridian_import_check(_: &FieldValue) {}
