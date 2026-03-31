mod auth;
mod client;
mod commands;
mod config;
mod display;
mod mcp;
mod plugin;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "bsctl", version, about = "A CLI client for Backstage")]
struct Cli {
    /// Backstage base URL (overrides config)
    #[arg(long, env = "BSCTL_BASE_URL")]
    base_url: Option<String>,

    /// Authentication token (overrides config)
    #[arg(long, env = "BSCTL_TOKEN")]
    token: Option<String>,

    /// Use a specific context from config
    #[arg(long, short)]
    context: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage catalog entities
    Catalog {
        #[command(subcommand)]
        command: commands::catalog::CatalogCommand,
    },
    /// Search the Backstage catalog
    Search {
        #[command(subcommand)]
        command: commands::search::SearchCommand,
    },
    /// Manage software templates
    Template {
        #[command(subcommand)]
        command: commands::template::TemplateCommand,
    },
    /// Send raw API requests to Backstage
    Api {
        #[command(subcommand)]
        command: commands::api::ApiCommand,
    },
    /// Auto-generate custom column definitions
    Columns {
        #[command(subcommand)]
        command: commands::columns::ColumnsCommand,
    },
    /// Authenticate with a Backstage instance
    Login {
        /// Auth provider (e.g. github, google, okta, microsoft)
        #[arg(long, short, default_value = "github")]
        provider: String,
    },
    /// Start MCP server (stdio transport)
    Mcp,
    /// Manage configuration and contexts
    Config {
        #[command(subcommand)]
        command: commands::config_cmd::ConfigCommand,
    },
    /// Show version information
    Version,
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        shell: clap_complete::Shell,
    },
    /// List available plugin commands from .bsctl.yaml
    Plugins,
    /// Run a plugin command defined in .bsctl.yaml
    #[command(external_subcommand)]
    Plugin(Vec<String>),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Commands that don't need an API client
    match &cli.command {
        Commands::Version => {
            println!("bsctl {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Commands::Completions { shell } => {
            clap_complete::generate(
                *shell,
                &mut <Cli as clap::CommandFactory>::command(),
                "bsctl",
                &mut std::io::stdout(),
            );
            return Ok(());
        }
        Commands::Config { command } => {
            return commands::config_cmd::run(command.clone());
        }
        Commands::Login { provider } => {
            let cfg = config::Config::load()?;
            let context_name = cli
                .context
                .clone()
                .or_else(|| cfg.current_context.clone())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "No context configured. Run:\n  bsctl config set-context <name> --base-url <url>"
                    )
                })?;
            let base_url = cli
                .base_url
                .clone()
                .or_else(|| cfg.contexts.get(&context_name).map(|c| c.base_url.clone()))
                .ok_or_else(|| anyhow::anyhow!("No base URL for context '{context_name}'"))?;
            auth::login(&base_url, provider, &context_name).await?;
            return Ok(());
        }
        _ => {}
    }

    // Resolve base_url and token from CLI args, env vars, or config file
    let cfg = config::Config::load()?;
    let context_name = cli.context.clone().or_else(|| cfg.current_context.clone());
    let ctx = context_name
        .as_ref()
        .and_then(|name| cfg.contexts.get(name));

    let base_url = cli
        .base_url
        .or_else(|| ctx.map(|c| c.base_url.clone()))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No base URL configured. Either:\n  \
                 - Pass --base-url <url>\n  \
                 - Set BSCTL_BASE_URL env var\n  \
                 - Run: bsctl config set-context <name> --base-url <url>"
            )
        })?;

    // Token priority: CLI flag > env var > config file > credentials.json (from login)
    let token = cli
        .token
        .or_else(|| ctx.and_then(|c| c.token.clone()))
        .or_else(|| {
            context_name
                .as_ref()
                .and_then(|name| auth::resolve_token(name))
        });

    let client = client::BackstageClient::new(&base_url, token.as_deref());
    let plugin_config = plugin::PluginConfig::load()?;

    match cli.command {
        Commands::Catalog { command } => {
            commands::catalog::run(&client, command, &plugin_config).await?
        }
        Commands::Columns { command } => commands::columns::run(&client, command).await?,
        Commands::Search { command } => commands::search::run(&client, command).await?,
        Commands::Template { command } => commands::template::run(&client, command).await?,
        Commands::Api { command } => commands::api::run(&client, command).await?,
        Commands::Plugins => {
            plugin::print_plugin_help(&plugin_config);
        }
        Commands::Plugin(args) => {
            run_plugin_command(&client, &plugin_config, args).await?;
        }
        Commands::Mcp => {
            mcp::serve(client).await?;
        }
        Commands::Config { .. }
        | Commands::Login { .. }
        | Commands::Version
        | Commands::Completions { .. } => unreachable!(),
    }

    Ok(())
}

async fn run_plugin_command(
    client: &client::BackstageClient,
    config: &plugin::PluginConfig,
    args: Vec<String>,
) -> Result<()> {
    if args.is_empty() {
        anyhow::bail!(
            "No plugin command specified. Run 'bsctl plugins' to see available commands."
        );
    }

    let plugin_name = &args[0];

    // Show plugin help if no subcommand
    if args.len() < 2 || args[1] == "help" || args[1] == "--help" {
        if let Some(commands) = config.plugins.get(plugin_name) {
            println!("Plugin: {plugin_name}\n");
            println!("Commands:");
            for (name, cmd) in commands {
                let desc = cmd.description.as_deref().unwrap_or("");
                println!("  {name:<20} {desc}");
            }
            return Ok(());
        }
        anyhow::bail!(
            "Unknown command '{plugin_name}'. Available plugins: {}",
            config
                .plugins
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    let subcommand = &args[1];

    // Show command help
    if args.len() > 2
        && (args[2] == "--help" || args[2] == "-h")
        && let Some(commands) = config.plugins.get(plugin_name)
        && let Some(cmd) = commands.get(subcommand)
    {
        plugin::print_command_help(plugin_name, subcommand, cmd);
        return Ok(());
    }

    // Parse remaining args into positional and named
    let mut positional = Vec::new();
    let mut named = Vec::new();
    let mut i = 2;
    while i < args.len() {
        if args[i].starts_with("--") {
            let key = args[i].trim_start_matches("--").to_string();
            let value = args.get(i + 1).cloned().unwrap_or_default();
            named.push((key, value));
            i += 2;
        } else {
            positional.push(args[i].clone());
            i += 1;
        }
    }

    plugin::run(client, plugin_name, subcommand, &positional, &named, config).await
}
