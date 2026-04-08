use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;
use serde::Deserialize;

use crate::client::BackstageClient;
use crate::display::{self, Cell, Style};
use crate::service;

#[derive(Subcommand)]
pub enum TemplateCommand {
    /// List available software templates
    List {
        #[arg(long, short, default_value = "table")]
        output: String,
    },
    /// Show template details and parameter schema
    Describe {
        name: String,
        #[arg(long, default_value = "default")]
        namespace: String,
    },
    /// Run a software template
    Run {
        name: String,
        #[arg(long = "param", short = 'p', value_name = "KEY=VALUE")]
        params: Vec<String>,
        #[arg(long, default_value = "default")]
        namespace: String,
        #[arg(long)]
        wait: bool,
        #[arg(long, default_value = "600")]
        timeout: u64,
    },
    /// Check the status of a template task
    Status {
        task_id: String,
        #[arg(long, short, default_value = "table")]
        output: String,
    },
    /// Cancel a running template task
    Cancel { task_id: String },
    /// View logs from a template task
    Log { task_id: String },
}

#[derive(Deserialize)]
struct TemplateEntity {
    metadata: TemplateMeta,
}

#[derive(Deserialize)]
struct TemplateMeta {
    #[serde(default)]
    name: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

pub async fn run(client: &BackstageClient, command: TemplateCommand) -> Result<()> {
    match command {
        TemplateCommand::List { output } => list(client, &output).await,
        TemplateCommand::Describe { name, namespace } => describe(client, &name, &namespace).await,
        TemplateCommand::Run {
            name,
            params,
            namespace,
            wait,
            timeout,
        } => run_template(client, &name, &namespace, params, wait, timeout).await,
        TemplateCommand::Status { task_id, output } => status(client, &task_id, &output).await,
        TemplateCommand::Cancel { task_id } => cancel(client, &task_id).await,
        TemplateCommand::Log { task_id } => log(client, &task_id).await,
    }
}

async fn list(client: &BackstageClient, output: &str) -> Result<()> {
    let templates = service::template_list(client).await?;
    match output {
        "json" => println!("{}", serde_json::to_string_pretty(&templates)?),
        _ => {
            let parsed: Vec<TemplateEntity> =
                serde_json::from_value(serde_json::Value::Array(templates))?;
            let rows: Vec<Vec<Cell>> = parsed
                .iter()
                .map(|t| {
                    let title = t.metadata.title.as_deref().unwrap_or("");
                    let desc = t
                        .metadata
                        .description
                        .as_deref()
                        .and_then(|d| d.lines().next())
                        .unwrap_or("");
                    let subtitle = if !title.is_empty() && !desc.is_empty() && title != desc {
                        desc
                    } else {
                        ""
                    };
                    vec![
                        Cell::new(&t.metadata.name),
                        Cell::new(title),
                        Cell::styled(subtitle, Style::Dim),
                    ]
                })
                .collect();
            display::table(&["Name", "Title", "Description"], &rows);
        }
    }
    Ok(())
}

async fn describe(client: &BackstageClient, name: &str, namespace: &str) -> Result<()> {
    let entity = service::template_describe(client, name, namespace).await?;
    let metadata = entity.get("metadata").cloned().unwrap_or_default();
    let spec = entity.get("spec").cloned().unwrap_or_default();

    let title = metadata
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or(name);
    let desc = metadata
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    println!("\n{} {}", "Template:".dimmed(), title.bold());
    if !desc.is_empty() {
        println!("{}", desc.dimmed());
    }

    if let Some(parameters) = spec.get("parameters") {
        println!("\n{}", "Parameters:".dimmed());
        print_parameters(parameters, 1);
    }
    if let Some(steps) = spec.get("steps").and_then(|v| v.as_array()) {
        println!("\n{}", "Steps:".dimmed());
        for (i, step) in steps.iter().enumerate() {
            let id = step.get("id").and_then(|v| v.as_str()).unwrap_or("?");
            let name = step.get("name").and_then(|v| v.as_str()).unwrap_or(id);
            let action = step.get("action").and_then(|v| v.as_str()).unwrap_or("");
            println!("  {}. {} {}", i + 1, name, format!("({action})").dimmed());
        }
    }
    println!();
    Ok(())
}

fn print_parameters(params: &serde_json::Value, depth: usize) {
    let indent = "  ".repeat(depth);
    let schemas = if let Some(arr) = params.as_array() {
        arr.clone()
    } else {
        vec![params.clone()]
    };
    for schema in &schemas {
        if let Some(title) = schema.get("title").and_then(|v| v.as_str()) {
            println!("{indent}{}", title.bold());
        }
        if let Some(properties) = schema.get("properties").and_then(|v| v.as_object()) {
            let required: Vec<&str> = schema
                .get("required")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            for (name, prop) in properties {
                let prop_type = prop.get("type").and_then(|v| v.as_str()).unwrap_or("");
                let description = prop
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let is_required = required.contains(&name.as_str());
                let req = if is_required {
                    "*".red().to_string()
                } else {
                    " ".to_string()
                };
                let type_str = if let Some(enums) = prop.get("enum").and_then(|v| v.as_array()) {
                    enums
                        .iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join("|")
                } else {
                    prop_type.to_string()
                };
                let default_str = match prop.get("default") {
                    Some(serde_json::Value::String(s)) => format!(" [default: {s}]"),
                    Some(v) if !v.is_null() => format!(" [default: {v}]"),
                    _ => String::new(),
                };
                println!(
                    "{indent}{req} {:<25} {:<15} {}{}",
                    name,
                    type_str.dimmed(),
                    description,
                    default_str.dimmed()
                );
            }
        }
    }
}

/// Extract default values from a template's parameters schema.
///
/// The schema's `parameters` field is an array of steps, each with a
/// `properties` object. We walk every step and collect `default` values.
fn extract_defaults(entity: &serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    let mut defaults = serde_json::Map::new();
    let Some(parameters) = entity.get("spec").and_then(|s| s.get("parameters")) else {
        return defaults;
    };

    let steps: Vec<&serde_json::Value> = if let Some(arr) = parameters.as_array() {
        arr.iter().collect()
    } else {
        vec![parameters]
    };

    for step in steps {
        if let Some(properties) = step.get("properties").and_then(|v| v.as_object()) {
            for (name, prop) in properties {
                if let Some(default) = prop.get("default")
                    && !default.is_null()
                {
                    defaults.insert(name.clone(), default.clone());
                }
            }
        }
    }
    defaults
}

async fn run_template(
    client: &BackstageClient,
    name: &str,
    namespace: &str,
    params: Vec<String>,
    wait: bool,
    timeout: u64,
) -> Result<()> {
    // Fetch the template schema to collect default values
    let entity = service::template_describe(client, name, namespace).await?;
    let mut values = extract_defaults(&entity);

    // --param values override defaults
    for param in &params {
        let (key, value) = param
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("Invalid parameter '{param}'. Expected: key=value"))?;
        let json_value = serde_json::from_str(value)
            .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));
        values.insert(key.to_string(), json_value);
    }

    let task_id = service::template_run(client, name, namespace, values).await?;
    println!("{} {}", "Task created:".green(), task_id.bold());

    if wait {
        wait_for_task(client, &task_id, timeout).await?;
    } else {
        println!("\n{}", "Check progress:".dimmed());
        println!("  bsctl template status {task_id}");
        println!("  bsctl template log {task_id}");
    }
    Ok(())
}

async fn wait_for_task(client: &BackstageClient, task_id: &str, timeout: u64) -> Result<()> {
    let start = std::time::Instant::now();
    let timeout_dur = std::time::Duration::from_secs(timeout);
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let task = service::template_status(client, task_id).await?;
        let status = task
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        match status {
            "completed" => {
                eprintln!();
                println!("{}", "Task completed successfully.".green());
                return Ok(());
            }
            "failed" => {
                eprintln!();
                anyhow::bail!("Task failed. Run 'bsctl template log {task_id}' for details.");
            }
            "cancelled" => {
                eprintln!();
                anyhow::bail!("Task was cancelled.");
            }
            _ => {
                if start.elapsed() > timeout_dur {
                    eprintln!();
                    anyhow::bail!("Timed out after {timeout}s. Task {task_id} is still {status}.");
                }
                eprint!(".");
            }
        }
    }
}

async fn status(client: &BackstageClient, task_id: &str, output: &str) -> Result<()> {
    let task = service::template_status(client, task_id).await?;
    match output {
        "json" => println!("{}", serde_json::to_string_pretty(&task)?),
        _ => {
            let s = task
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let colored = match s {
                "completed" => s.green().to_string(),
                "failed" => s.red().to_string(),
                "processing" => s.yellow().to_string(),
                _ => s.to_string(),
            };
            let id = task.get("id").and_then(|v| v.as_str()).unwrap_or(task_id);
            let created = task.get("createdAt").and_then(|v| v.as_str()).unwrap_or("");
            println!("\n{} {}", "Task:".dimmed(), id.bold());
            println!("  {:<12} {}", "Status:".dimmed(), colored);
            println!("  {:<12} {}", "Created:".dimmed(), created);
            if let Some(hb) = task.get("lastHeartbeatAt").and_then(|v| v.as_str()) {
                println!("  {:<12} {}", "Heartbeat:".dimmed(), hb);
            }
            if let Some(ti) = task
                .get("spec")
                .and_then(|s| s.get("templateInfo"))
                .and_then(|t| t.get("entityRef"))
                .and_then(|v| v.as_str())
            {
                println!("  {:<12} {}", "Template:".dimmed(), ti);
            }
        }
    }
    Ok(())
}

async fn cancel(client: &BackstageClient, task_id: &str) -> Result<()> {
    service::template_cancel(client, task_id).await?;
    println!("{} task {task_id}", "Cancelled".yellow());
    Ok(())
}

async fn log(client: &BackstageClient, task_id: &str) -> Result<()> {
    let events = service::template_events(client, task_id).await?;
    for event in &events {
        let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let created = event
            .get("createdAt")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let body = event.get("body");
        match event_type {
            "log" => {
                if let Some(msg) = body.and_then(|b| b.get("message")).and_then(|v| v.as_str()) {
                    let step = body
                        .and_then(|b| b.get("stepId"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    println!("[{created}] [{step}] {msg}");
                }
            }
            "completion" => {
                if let Some(output) = body.and_then(|b| b.get("output")) {
                    println!("[{created}] [completed] output:");
                    println!("{}", serde_json::to_string_pretty(output)?);
                }
            }
            _ => println!(
                "[{created}] [{event_type}] {}",
                serde_json::to_string(body.unwrap_or(&serde_json::Value::Null))?
            ),
        }
    }
    if events.is_empty() {
        println!("No events yet.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_entity(parameters: serde_json::Value) -> serde_json::Value {
        json!({
            "spec": {
                "parameters": parameters
            }
        })
    }

    #[test]
    fn extract_defaults_merges_across_steps() {
        let entity = make_entity(json!([
            {
                "title": "Step 1",
                "properties": {
                    "customer": { "type": "string" },
                    "unit": { "type": "string", "default": "Hammerhead" }
                }
            },
            {
                "title": "Step 2",
                "properties": {
                    "project_name": { "type": "string", "default": "FY25 Tactna" },
                    "sso_group_prefix": { "type": "string", "default": "hammerhead" }
                }
            }
        ]));

        let defaults = extract_defaults(&entity);
        assert_eq!(defaults.len(), 3);
        assert_eq!(defaults["unit"], json!("Hammerhead"));
        assert_eq!(defaults["project_name"], json!("FY25 Tactna"));
        assert_eq!(defaults["sso_group_prefix"], json!("hammerhead"));
        // customer has no default, so it should not appear
        assert!(!defaults.contains_key("customer"));
    }

    #[test]
    fn extract_defaults_preserves_non_string_types() {
        let entity = make_entity(json!([{
            "properties": {
                "count": { "type": "integer", "default": 42 },
                "enabled": { "type": "boolean", "default": true },
                "tags": { "type": "array", "default": ["a", "b"] }
            }
        }]));

        let defaults = extract_defaults(&entity);
        assert_eq!(defaults["count"], json!(42));
        assert_eq!(defaults["enabled"], json!(true));
        assert_eq!(defaults["tags"], json!(["a", "b"]));
    }

    #[test]
    fn explicit_param_overrides_default() {
        let entity = make_entity(json!([{
            "properties": {
                "unit": { "type": "string", "default": "Hammerhead" },
                "customer": { "type": "string" }
            }
        }]));

        let mut values = extract_defaults(&entity);
        // Simulate --param unit=OtherUnit --param customer=msalife
        values.insert("unit".to_string(), json!("OtherUnit"));
        values.insert("customer".to_string(), json!("msalife"));

        assert_eq!(values["unit"], json!("OtherUnit"));
        assert_eq!(values["customer"], json!("msalife"));
    }

    #[test]
    fn no_default_field_stays_absent() {
        let entity = make_entity(json!([{
            "properties": {
                "customer": { "type": "string" },
                "region": { "type": "string" }
            }
        }]));

        let defaults = extract_defaults(&entity);
        assert!(defaults.is_empty());
    }

    #[test]
    fn extract_defaults_single_step_not_array() {
        // parameters can be a single object instead of an array
        let entity = make_entity(json!({
            "properties": {
                "name": { "type": "string", "default": "test" }
            }
        }));

        let defaults = extract_defaults(&entity);
        assert_eq!(defaults.len(), 1);
        assert_eq!(defaults["name"], json!("test"));
    }

    #[test]
    fn extract_defaults_no_spec() {
        let entity = json!({});
        let defaults = extract_defaults(&entity);
        assert!(defaults.is_empty());
    }

    #[test]
    fn extract_defaults_null_default_ignored() {
        let entity = make_entity(json!([{
            "properties": {
                "field": { "type": "string", "default": null }
            }
        }]));

        let defaults = extract_defaults(&entity);
        assert!(!defaults.contains_key("field"));
    }
}
