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
        output: String,
    },
    /// Show template details and parameter schema
    Describe {
        /// Template name
        name: String,

        /// Template namespace (default: default)
        #[arg(long, default_value = "default")]
        namespace: String,
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

        /// Wait for task completion (poll interval: 3s)
        #[arg(long)]
        wait: bool,

        /// Timeout in seconds when using --wait (default: 600)
        #[arg(long, default_value = "600")]
        timeout: u64,
    },
    /// Check the status of a template task
    Status {
        /// Task ID
        task_id: String,

        /// Output format
        #[arg(long, short, default_value = "table")]
        output: String,
    },
    /// Cancel a running template task
    Cancel {
        /// Task ID
        task_id: String,
    },
    /// View logs from a template task
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
    let templates: Vec<TemplateEntity> = client
        .get("/api/catalog/entities?filter=kind=Template")
        .await?;

    match output {
        "json" => {
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
        _ => {
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
    }
    Ok(())
}

async fn describe(client: &BackstageClient, name: &str, namespace: &str) -> Result<()> {
    let path = format!("/api/catalog/entities/by-name/template/{namespace}/{name}");
    let entity: serde_json::Value = client.get(&path).await?;

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

    // Show parameters from spec.parameters
    if let Some(parameters) = spec.get("parameters") {
        println!("\n{}", "Parameters:".dimmed());
        print_parameters(parameters, 1);
    }

    // Show steps
    if let Some(steps) = spec.get("steps").and_then(|v| v.as_array()) {
        println!("\n{}", "Steps:".dimmed());
        for (i, step) in steps.iter().enumerate() {
            let step_id = step.get("id").and_then(|v| v.as_str()).unwrap_or("?");
            let step_name = step.get("name").and_then(|v| v.as_str()).unwrap_or(step_id);
            let action = step.get("action").and_then(|v| v.as_str()).unwrap_or("");
            println!(
                "  {}. {} {}",
                i + 1,
                step_name,
                format!("({action})").dimmed()
            );
        }
    }

    println!();
    Ok(())
}

fn print_parameters(params: &serde_json::Value, depth: usize) {
    let indent = "  ".repeat(depth);

    // Parameters can be an array of pages or a single object
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
                let default = prop.get("default");
                let is_required = required.contains(&name.as_str());

                let req_marker = if is_required {
                    "*".red().to_string()
                } else {
                    " ".to_string()
                };

                let type_str = if let Some(enums) = prop.get("enum").and_then(|v| v.as_array()) {
                    let values: Vec<&str> = enums.iter().filter_map(|v| v.as_str()).collect();
                    values.join("|")
                } else {
                    prop_type.to_string()
                };

                let default_str = match default {
                    Some(serde_json::Value::String(s)) => format!(" [default: {s}]"),
                    Some(v) if !v.is_null() => format!(" [default: {v}]"),
                    _ => String::new(),
                };

                println!(
                    "{indent}{req_marker} {:<25} {:<15} {}{}",
                    name,
                    type_str.dimmed(),
                    description,
                    default_str.dimmed()
                );
            }
        }
    }
}

async fn run_template(
    client: &BackstageClient,
    name: &str,
    namespace: &str,
    params: Vec<String>,
    wait: bool,
    timeout: u64,
) -> Result<()> {
    let mut values = serde_json::Map::new();
    for param in &params {
        let (key, value) = param.split_once('=').ok_or_else(|| {
            anyhow::anyhow!("Invalid parameter '{param}'. Expected format: key=value")
        })?;
        let json_value = serde_json::from_str(value)
            .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));
        values.insert(key.to_string(), json_value);
    }

    let body = serde_json::json!({
        "templateRef": format!("template:{namespace}/{name}"),
        "values": values,
    });

    let resp: TaskCreatedResponse = client.post("/api/scaffolder/v2/tasks", &body).await?;

    println!("{} {}", "Task created:".green(), resp.id.bold());

    if wait {
        wait_for_task(client, &resp.id, timeout).await?;
    } else {
        println!("\n{}", "Check progress:".dimmed());
        println!("  bsctl template status {}", resp.id);
        println!("  bsctl template log {}", resp.id);
    }
    Ok(())
}

async fn wait_for_task(client: &BackstageClient, task_id: &str, timeout: u64) -> Result<()> {
    let start = std::time::Instant::now();
    let timeout_dur = std::time::Duration::from_secs(timeout);

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        let path = format!("/api/scaffolder/v2/tasks/{}", urlencoding::encode(task_id));
        let task: ScaffolderTask = client.get(&path).await?;

        match task.status.as_str() {
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
                    anyhow::bail!(
                        "Timed out after {timeout}s. Task {task_id} is still {status}.",
                        status = task.status
                    );
                }
                eprint!(".");
            }
        }
    }
}

async fn status(client: &BackstageClient, task_id: &str, output: &str) -> Result<()> {
    let path = format!("/api/scaffolder/v2/tasks/{}", urlencoding::encode(task_id));

    match output {
        "json" => {
            let full: serde_json::Value = client.get(&path).await?;
            println!("{}", serde_json::to_string_pretty(&full)?);
        }
        _ => {
            let task: ScaffolderTask = client.get(&path).await?;
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

            let steps_path = format!(
                "/api/scaffolder/v2/tasks/{}/steps",
                urlencoding::encode(task_id)
            );
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
    }
    Ok(())
}

async fn cancel(client: &BackstageClient, task_id: &str) -> Result<()> {
    let path = format!(
        "/api/scaffolder/v2/tasks/{}/cancel",
        urlencoding::encode(task_id)
    );
    let body = serde_json::json!({});
    let _: serde_json::Value = client.post(&path, &body).await?;
    println!("{} task {task_id}", "Cancelled".yellow());
    Ok(())
}

async fn log(client: &BackstageClient, task_id: &str) -> Result<()> {
    let path = format!(
        "/api/scaffolder/v2/tasks/{}/events",
        urlencoding::encode(task_id)
    );
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
