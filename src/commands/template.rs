use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;
use serde::Deserialize;

use crate::client::BackstageClient;
use crate::display::{self, Cell, Style};

#[derive(Subcommand)]
pub enum TemplateCommand {
    /// List available software templates
    List {
        /// Output format
        #[arg(long, short, default_value = "table")]
        output: super::catalog::OutputFormat,
    },
    /// Run a software template
    Run {
        /// Template name (e.g. tenant-creation)
        name: String,

        /// Template parameters as key=value pairs
        #[arg(long = "param", short = 'p', value_name = "KEY=VALUE")]
        params: Vec<String>,

        /// Template namespace (default: default)
        #[arg(long, default_value = "default")]
        namespace: String,
    },
    /// Check the status of a template task
    Status {
        /// Task ID
        task_id: String,

        /// Output format
        #[arg(long, short, default_value = "table")]
        output: super::catalog::OutputFormat,
    },
    /// Stream logs from a template task
    Log {
        /// Task ID
        task_id: String,
    },
}

#[derive(Deserialize)]
struct TemplateEntity {
    metadata: TemplateMetadata,
}

#[derive(Deserialize)]
struct TemplateMetadata {
    #[serde(default)]
    name: String,
    #[serde(default)]
    namespace: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Deserialize)]
struct ScaffolderTask {
    id: String,
    #[serde(default)]
    status: String,
    #[serde(default, rename = "createdAt")]
    created_at: String,
    #[serde(default, rename = "lastHeartbeatAt")]
    last_heartbeat_at: Option<String>,
    #[serde(default)]
    spec: serde_json::Value,
}

#[derive(Deserialize)]
struct TaskCreatedResponse {
    id: String,
}

pub async fn run(client: &BackstageClient, command: TemplateCommand) -> Result<()> {
    match command {
        TemplateCommand::List { output } => list(client, output).await,
        TemplateCommand::Run {
            name,
            params,
            namespace,
        } => run_template(client, &name, &namespace, params).await,
        TemplateCommand::Status { task_id, output } => status(client, &task_id, output).await,
        TemplateCommand::Log { task_id } => log(client, &task_id).await,
    }
}

async fn list(client: &BackstageClient, output: super::catalog::OutputFormat) -> Result<()> {
    let templates: Vec<TemplateEntity> = client
        .get("/api/catalog/entities?filter=kind=Template")
        .await?;

    match output {
        super::catalog::OutputFormat::Table => {
            let rows: Vec<Vec<Cell>> = templates
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
        super::catalog::OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!(
                    templates
                        .iter()
                        .map(|t| serde_json::json!({
                            "namespace": t.metadata.namespace.as_deref().unwrap_or("default"),
                            "name": t.metadata.name,
                            "title": t.metadata.title,
                            "description": t.metadata.description,
                        }))
                        .collect::<Vec<_>>()
                ))?
            );
        }
    }
    Ok(())
}

async fn run_template(
    client: &BackstageClient,
    name: &str,
    namespace: &str,
    params: Vec<String>,
) -> Result<()> {
    let mut values = serde_json::Map::new();
    for param in &params {
        let (key, value) = param.split_once('=').ok_or_else(|| {
            anyhow::anyhow!("Invalid parameter '{param}'. Expected format: key=value")
        })?;
        // Try to parse as JSON value, fall back to string
        let json_value = serde_json::from_str(value)
            .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));
        values.insert(key.to_string(), json_value);
    }

    let body = serde_json::json!({
        "templateRef": format!("template:{namespace}/{name}"),
        "values": values,
    });

    let resp: TaskCreatedResponse = client.post("/api/scaffolder/v2/tasks", &body).await?;

    println!("\n{} {}", "Task created:".green(), resp.id.bold());
    println!("\n{}", "Check progress:".dimmed());
    println!("  bsctl template status {}", resp.id);
    println!("  bsctl template log {}", resp.id);
    Ok(())
}

async fn status(
    client: &BackstageClient,
    task_id: &str,
    output: super::catalog::OutputFormat,
) -> Result<()> {
    let path = format!("/api/scaffolder/v2/tasks/{task_id}");
    let task: ScaffolderTask = client.get(&path).await?;

    match output {
        super::catalog::OutputFormat::Table => {
            let status_display = match task.status.as_str() {
                "completed" => task.status.green().to_string(),
                "failed" => task.status.red().to_string(),
                "processing" => task.status.yellow().to_string(),
                _ => task.status.clone(),
            };

            println!("\n{} {}", "Task:".dimmed(), task.id.bold());
            println!("  {:<12} {}", "Status:".dimmed(), status_display);
            println!("  {:<12} {}", "Created:".dimmed(), task.created_at);
            if let Some(hb) = &task.last_heartbeat_at {
                println!("  {:<12} {}", "Heartbeat:".dimmed(), hb);
            }
            if let Some(template_info) = task.spec.get("templateInfo")
                && let Some(entity_ref) = template_info.get("entityRef").and_then(|v| v.as_str())
            {
                println!("  {:<12} {}", "Template:".dimmed(), entity_ref);
            }

            // Show step status if available
            let steps_path = format!("/api/scaffolder/v2/tasks/{task_id}/steps");
            if let Ok(steps) = client.get::<serde_json::Value>(&steps_path).await
                && let Some(arr) = steps.as_array()
            {
                println!("\n  {}", "Steps:".dimmed());
                for step in arr {
                    let name = step.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let step_status = step.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                    let icon = match step_status {
                        "completed" => "✓".green().to_string(),
                        "failed" => "✗".red().to_string(),
                        "processing" => "▸".yellow().to_string(),
                        _ => " ".to_string(),
                    };
                    println!("    {icon} {name}");
                }
            }
            println!();
        }
        super::catalog::OutputFormat::Json => {
            let full: serde_json::Value = client.get(&path).await?;
            println!("{}", serde_json::to_string_pretty(&full)?);
        }
    }
    Ok(())
}

async fn log(client: &BackstageClient, task_id: &str) -> Result<()> {
    let path = format!("/api/scaffolder/v2/tasks/{task_id}/events");
    let events: Vec<serde_json::Value> = client.get(&path).await?;

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
            _ => {
                println!(
                    "[{created}] [{event_type}] {}",
                    serde_json::to_string(body.unwrap_or(&serde_json::Value::Null))?
                );
            }
        }
    }

    if events.is_empty() {
        println!("No events yet.");
    }
    Ok(())
}
