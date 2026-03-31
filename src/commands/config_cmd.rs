use anyhow::Result;
use clap::Subcommand;

use crate::config::Config;

#[derive(Subcommand, Clone)]
pub enum ConfigCommand {
    /// Set a context (profile) for a Backstage instance
    SetContext {
        /// Context name
        name: String,

        /// Backstage base URL
        #[arg(long)]
        base_url: String,

        /// Static token (optional)
        #[arg(long)]
        token: Option<String>,
    },
    /// Switch the active context
    UseContext {
        /// Context name
        name: String,
    },
    /// Show the current context
    CurrentContext,
    /// List all contexts
    GetContexts,
    /// Delete a context
    DeleteContext {
        /// Context name
        name: String,
    },
}

pub fn run(command: ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::SetContext {
            name,
            base_url,
            token,
        } => set_context(&name, &base_url, token),
        ConfigCommand::UseContext { name } => use_context(&name),
        ConfigCommand::CurrentContext => current_context(),
        ConfigCommand::GetContexts => get_contexts(),
        ConfigCommand::DeleteContext { name } => delete_context(&name),
    }
}

fn set_context(name: &str, base_url: &str, token: Option<String>) -> Result<()> {
    let mut config = Config::load()?;
    config.contexts.insert(
        name.to_string(),
        crate::config::ContextConfig {
            base_url: base_url.to_string(),
            token,
        },
    );
    // Auto-set current context if it's the first one
    if config.current_context.is_none() {
        config.current_context = Some(name.to_string());
    }
    config.save()?;
    println!("Context '{name}' set.");
    if config.current_context.as_deref() == Some(name) {
        println!("Active context: {name}");
    }
    Ok(())
}

fn use_context(name: &str) -> Result<()> {
    let mut config = Config::load()?;
    if !config.contexts.contains_key(name) {
        anyhow::bail!(
            "Context '{name}' not found. Available: {}",
            config
                .contexts
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    config.current_context = Some(name.to_string());
    config.save()?;
    println!("Switched to context '{name}'.");
    Ok(())
}

fn current_context() -> Result<()> {
    let config = Config::load()?;
    match &config.current_context {
        Some(name) => {
            println!("{name}");
            if let Some(ctx) = config.contexts.get(name) {
                println!("  base-url: {}", ctx.base_url);
                println!(
                    "  token:    {}",
                    if ctx.token.is_some() { "***" } else { "(none)" }
                );
            }
        }
        None => println!("No active context. Run 'bsctl config set-context' to create one."),
    }
    Ok(())
}

fn get_contexts() -> Result<()> {
    let config = Config::load()?;
    if config.contexts.is_empty() {
        println!("No contexts configured. Run 'bsctl config set-context' to create one.");
        return Ok(());
    }
    let current = config.current_context.as_deref().unwrap_or("");
    println!("{:<3} {:<20} {:<40} TOKEN", "", "NAME", "BASE URL");
    for (name, ctx) in &config.contexts {
        let marker = if name == current { "*" } else { "" };
        let token_status = if ctx.token.is_some() { "***" } else { "(none)" };
        println!(
            "{:<3} {:<20} {:<40} {}",
            marker, name, ctx.base_url, token_status
        );
    }
    Ok(())
}

fn delete_context(name: &str) -> Result<()> {
    let mut config = Config::load()?;
    if config.contexts.remove(name).is_none() {
        anyhow::bail!("Context '{name}' not found.");
    }
    if config.current_context.as_deref() == Some(name) {
        config.current_context = config.contexts.keys().next().cloned();
    }
    config.save()?;
    println!("Context '{name}' deleted.");
    Ok(())
}
