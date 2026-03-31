use super::config::{CommandDef, PluginConfig};

pub fn print_plugin_help(config: &PluginConfig) {
    if config.plugins.is_empty() {
        return;
    }
    println!("\nPlugin commands (from .bsctl.yaml):");
    for (plugin, commands) in &config.plugins {
        for (cmd_name, cmd_def) in commands {
            let desc = cmd_def.description.as_deref().unwrap_or("");
            println!("  {plugin} {cmd_name:<20} {desc}");
        }
    }
}

pub fn print_command_help(plugin_name: &str, subcommand: &str, cmd: &CommandDef) {
    let desc = cmd.description.as_deref().unwrap_or("No description");
    println!("{desc}\n");
    println!("Usage: bsctl {plugin_name} {subcommand}{}", {
        let mut parts = String::new();
        for arg in &cmd.args {
            if arg.required.unwrap_or(true) {
                parts.push_str(&format!(" <{}>", arg.name));
            } else {
                parts.push_str(&format!(" [{}]", arg.name));
            }
        }
        for param in &cmd.params {
            if param.required.unwrap_or(false) {
                parts.push_str(&format!(" --{} <VALUE>", param.name));
            } else {
                parts.push_str(&format!(" [--{} <VALUE>]", param.name));
            }
        }
        parts
    });
    println!("\nMethod: {:?} {}", cmd.method, cmd.path);
}
