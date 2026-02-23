use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "waft", about = "Waft desktop shell daemon")]
pub struct Cli {
    /// Output in JSON format
    #[arg(short = 'j', long = "json", global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Start the waft daemon
    Daemon,
    /// Plugin management
    Plugin {
        #[command(subcommand)]
        command: PluginCommand,
    },
    /// List protocol entity types and their schemas
    Protocol {
        /// Show an entity type or filter by domain
        entity_type: Option<String>,
        /// Filter by domain (e.g. audio, display, bluetooth)
        #[arg(long)]
        domain: Option<String>,
        /// Show detailed properties and actions
        #[arg(short, long)]
        verbose: bool,
    },
    /// Query live entity state from the daemon
    #[command(alias = "state")]
    Query {
        /// Entity type to query (omit for all types)
        entity_type: Option<String>,
        /// Start the plugin if not running
        #[arg(short, long)]
        start: bool,
        /// Timeout in milliseconds (used with --start)
        #[arg(long, default_value = "5000")]
        timeout_ms: u64,
    },
}

#[derive(Subcommand)]
pub enum PluginCommand {
    /// List discovered plugins and their entity types
    Ls,
    /// Show detailed information about a specific plugin
    Describe {
        /// Plugin name (e.g. "clock", "bluez", "audio")
        name: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn no_args_is_daemon_mode() {
        let cli = Cli::try_parse_from(["waft"]).unwrap();
        assert!(cli.command.is_none());
        assert!(!cli.json);
    }

    #[test]
    fn daemon_subcommand() {
        let cli = Cli::try_parse_from(["waft", "daemon"]).unwrap();
        assert!(matches!(cli.command, Some(Command::Daemon)));
    }

    #[test]
    fn plugin_ls_subcommand() {
        let cli = Cli::try_parse_from(["waft", "plugin", "ls"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Plugin {
                command: PluginCommand::Ls
            })
        ));
    }

    #[test]
    fn json_flag_long() {
        let cli = Cli::try_parse_from(["waft", "--json", "plugin", "ls"]).unwrap();
        assert!(cli.json);
        assert!(matches!(
            cli.command,
            Some(Command::Plugin {
                command: PluginCommand::Ls
            })
        ));
    }

    #[test]
    fn json_flag_short() {
        let cli = Cli::try_parse_from(["waft", "-j", "plugin", "ls"]).unwrap();
        assert!(cli.json);
    }

    #[test]
    fn plugin_describe_subcommand() {
        let cli = Cli::try_parse_from(["waft", "plugin", "describe", "clock"]).unwrap();
        match cli.command {
            Some(Command::Plugin {
                command: PluginCommand::Describe { name },
            }) => {
                assert_eq!(name, "clock");
            }
            _ => panic!("expected Plugin Describe command"),
        }
    }

    #[test]
    fn plugin_describe_with_json() {
        let cli = Cli::try_parse_from(["waft", "-j", "plugin", "describe", "bluez"]).unwrap();
        assert!(cli.json);
        match cli.command {
            Some(Command::Plugin {
                command: PluginCommand::Describe { name },
            }) => {
                assert_eq!(name, "bluez");
            }
            _ => panic!("expected Plugin Describe command"),
        }
    }

    #[test]
    fn protocol_subcommand_no_args() {
        let cli = Cli::try_parse_from(["waft", "protocol"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Protocol { entity_type: None, domain: None, verbose: false })
        ));
    }

    #[test]
    fn protocol_subcommand_with_entity_type() {
        let cli = Cli::try_parse_from(["waft", "protocol", "audio-device"]).unwrap();
        match cli.command {
            Some(Command::Protocol { entity_type, domain, verbose }) => {
                assert_eq!(entity_type.as_deref(), Some("audio-device"));
                assert_eq!(domain, None);
                assert!(!verbose);
            }
            _ => panic!("expected Protocol command"),
        }
    }

    #[test]
    fn protocol_subcommand_with_domain_filter() {
        let cli = Cli::try_parse_from(["waft", "protocol", "--domain", "audio"]).unwrap();
        match cli.command {
            Some(Command::Protocol { entity_type, domain, verbose }) => {
                assert_eq!(entity_type, None);
                assert_eq!(domain.as_deref(), Some("audio"));
                assert!(!verbose);
            }
            _ => panic!("expected Protocol command"),
        }
    }

    #[test]
    fn protocol_subcommand_verbose() {
        let cli = Cli::try_parse_from(["waft", "protocol", "--verbose"]).unwrap();
        match cli.command {
            Some(Command::Protocol { entity_type, domain, verbose }) => {
                assert_eq!(entity_type, None);
                assert_eq!(domain, None);
                assert!(verbose);
            }
            _ => panic!("expected Protocol command"),
        }
    }

    #[test]
    fn protocol_subcommand_verbose_short() {
        let cli = Cli::try_parse_from(["waft", "protocol", "-v"]).unwrap();
        match cli.command {
            Some(Command::Protocol { verbose, .. }) => assert!(verbose),
            _ => panic!("expected Protocol command"),
        }
    }

    #[test]
    fn protocol_subcommand_with_json() {
        let cli = Cli::try_parse_from(["waft", "-j", "protocol", "audio-device"]).unwrap();
        assert!(cli.json);
        match cli.command {
            Some(Command::Protocol { entity_type, .. }) => {
                assert_eq!(entity_type.as_deref(), Some("audio-device"));
            }
            _ => panic!("expected Protocol command"),
        }
    }

    #[test]
    fn query_no_args() {
        let cli = Cli::try_parse_from(["waft", "query"]).unwrap();
        match cli.command {
            Some(Command::Query { entity_type, start, timeout_ms }) => {
                assert_eq!(entity_type, None);
                assert!(!start);
                assert_eq!(timeout_ms, 5000);
            }
            _ => panic!("expected Query command"),
        }
    }

    #[test]
    fn query_with_entity_type() {
        let cli = Cli::try_parse_from(["waft", "query", "battery"]).unwrap();
        match cli.command {
            Some(Command::Query { entity_type, start, timeout_ms }) => {
                assert_eq!(entity_type.as_deref(), Some("battery"));
                assert!(!start);
                assert_eq!(timeout_ms, 5000);
            }
            _ => panic!("expected Query command"),
        }
    }

    #[test]
    fn state_alias() {
        let cli = Cli::try_parse_from(["waft", "state", "battery"]).unwrap();
        match cli.command {
            Some(Command::Query { entity_type, .. }) => {
                assert_eq!(entity_type.as_deref(), Some("battery"));
            }
            _ => panic!("expected Query command via state alias"),
        }
    }

    #[test]
    fn query_with_start_flag() {
        let cli = Cli::try_parse_from(["waft", "query", "audio-device", "--start"]).unwrap();
        match cli.command {
            Some(Command::Query { entity_type, start, timeout_ms }) => {
                assert_eq!(entity_type.as_deref(), Some("audio-device"));
                assert!(start);
                assert_eq!(timeout_ms, 5000);
            }
            _ => panic!("expected Query command"),
        }
    }

    #[test]
    fn query_with_start_short_flag() {
        let cli = Cli::try_parse_from(["waft", "query", "-s", "audio-device"]).unwrap();
        match cli.command {
            Some(Command::Query { entity_type, start, .. }) => {
                assert_eq!(entity_type.as_deref(), Some("audio-device"));
                assert!(start);
            }
            _ => panic!("expected Query command"),
        }
    }

    #[test]
    fn query_with_timeout() {
        let cli = Cli::try_parse_from(["waft", "query", "--start", "--timeout-ms", "10000", "battery"]).unwrap();
        match cli.command {
            Some(Command::Query { entity_type, start, timeout_ms }) => {
                assert_eq!(entity_type.as_deref(), Some("battery"));
                assert!(start);
                assert_eq!(timeout_ms, 10000);
            }
            _ => panic!("expected Query command"),
        }
    }

    #[test]
    fn query_with_json_flag() {
        let cli = Cli::try_parse_from(["waft", "-j", "query", "clock"]).unwrap();
        assert!(cli.json);
        match cli.command {
            Some(Command::Query { entity_type, .. }) => {
                assert_eq!(entity_type.as_deref(), Some("clock"));
            }
            _ => panic!("expected Query command"),
        }
    }
}
