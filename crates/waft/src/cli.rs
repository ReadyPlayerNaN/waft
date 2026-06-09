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
    /// List and run command palette actions
    Commands {
        /// Filter commands by label
        filter: Option<String>,
        /// Run the best-matching command instead of listing
        #[arg(short, long)]
        run: bool,
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
        let cli = Cli::try_parse_from(["waft"]).expect("expected value");
        assert!(cli.command.is_none());
        assert!(!cli.json);
    }

    #[test]
    fn daemon_subcommand() {
        let cli = Cli::try_parse_from(["waft", "daemon"]).expect("expected value");
        assert!(matches!(cli.command, Some(Command::Daemon)));
    }

    #[test]
    fn plugin_ls_subcommand() {
        let cli = Cli::try_parse_from(["waft", "plugin", "ls"]).expect("expected value");
        assert!(matches!(
            cli.command,
            Some(Command::Plugin {
                command: PluginCommand::Ls
            })
        ));
    }

    #[test]
    fn json_flag_long() {
        let cli = Cli::try_parse_from(["waft", "--json", "plugin", "ls"]).expect("expected value");
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
        let cli = Cli::try_parse_from(["waft", "-j", "plugin", "ls"]).expect("expected value");
        assert!(cli.json);
    }

    #[test]
    fn plugin_describe_subcommand() {
        let cli =
            Cli::try_parse_from(["waft", "plugin", "describe", "clock"]).expect("expected value");
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
        let cli = Cli::try_parse_from(["waft", "-j", "plugin", "describe", "bluez"])
            .expect("expected value");
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
        let cli = Cli::try_parse_from(["waft", "protocol"]).expect("expected value");
        assert!(matches!(
            cli.command,
            Some(Command::Protocol {
                entity_type: None,
                domain: None,
                verbose: false
            })
        ));
    }

    #[test]
    fn protocol_subcommand_with_entity_type() {
        let cli =
            Cli::try_parse_from(["waft", "protocol", "audio-device"]).expect("expected value");
        match cli.command {
            Some(Command::Protocol {
                entity_type,
                domain,
                verbose,
            }) => {
                assert_eq!(entity_type.as_deref(), Some("audio-device"));
                assert_eq!(domain, None);
                assert!(!verbose);
            }
            _ => panic!("expected Protocol command"),
        }
    }

    #[test]
    fn protocol_subcommand_with_domain_filter() {
        let cli =
            Cli::try_parse_from(["waft", "protocol", "--domain", "audio"]).expect("expected value");
        match cli.command {
            Some(Command::Protocol {
                entity_type,
                domain,
                verbose,
            }) => {
                assert_eq!(entity_type, None);
                assert_eq!(domain.as_deref(), Some("audio"));
                assert!(!verbose);
            }
            _ => panic!("expected Protocol command"),
        }
    }

    #[test]
    fn protocol_subcommand_verbose() {
        let cli = Cli::try_parse_from(["waft", "protocol", "--verbose"]).expect("expected value");
        match cli.command {
            Some(Command::Protocol {
                entity_type,
                domain,
                verbose,
            }) => {
                assert_eq!(entity_type, None);
                assert_eq!(domain, None);
                assert!(verbose);
            }
            _ => panic!("expected Protocol command"),
        }
    }

    #[test]
    fn protocol_subcommand_verbose_short() {
        let cli = Cli::try_parse_from(["waft", "protocol", "-v"]).expect("expected value");
        match cli.command {
            Some(Command::Protocol { verbose, .. }) => assert!(verbose),
            _ => panic!("expected Protocol command"),
        }
    }

    #[test]
    fn protocol_subcommand_with_json() {
        let cli = Cli::try_parse_from(["waft", "-j", "protocol", "audio-device"])
            .expect("expected value");
        assert!(cli.json);
        match cli.command {
            Some(Command::Protocol { entity_type, .. }) => {
                assert_eq!(entity_type.as_deref(), Some("audio-device"));
            }
            _ => panic!("expected Protocol command"),
        }
    }

    #[test]
    fn commands_no_args() {
        let cli = Cli::try_parse_from(["waft", "commands"]).expect("expected value");
        match cli.command {
            Some(Command::Commands { filter, run }) => {
                assert_eq!(filter, None);
                assert!(!run);
            }
            _ => panic!("expected Commands command"),
        }
    }

    #[test]
    fn commands_with_filter() {
        let cli = Cli::try_parse_from(["waft", "commands", "dark"]).expect("expected value");
        match cli.command {
            Some(Command::Commands { filter, run }) => {
                assert_eq!(filter.as_deref(), Some("dark"));
                assert!(!run);
            }
            _ => panic!("expected Commands command"),
        }
    }

    #[test]
    fn commands_with_run_flag() {
        let cli =
            Cli::try_parse_from(["waft", "commands", "--run", "lock"]).expect("expected value");
        match cli.command {
            Some(Command::Commands { filter, run }) => {
                assert_eq!(filter.as_deref(), Some("lock"));
                assert!(run);
            }
            _ => panic!("expected Commands command"),
        }
    }

    #[test]
    fn commands_with_run_short_flag() {
        let cli = Cli::try_parse_from(["waft", "commands", "-r", "dark"]).expect("expected value");
        match cli.command {
            Some(Command::Commands { filter, run }) => {
                assert_eq!(filter.as_deref(), Some("dark"));
                assert!(run);
            }
            _ => panic!("expected Commands command"),
        }
    }

    #[test]
    fn commands_with_json() {
        let cli = Cli::try_parse_from(["waft", "-j", "commands"]).expect("expected value");
        assert!(cli.json);
        assert!(matches!(
            cli.command,
            Some(Command::Commands {
                filter: None,
                run: false
            })
        ));
    }

    #[test]
    fn commands_with_json_and_filter() {
        let cli = Cli::try_parse_from(["waft", "-j", "commands", "dark"]).expect("expected value");
        assert!(cli.json);
        match cli.command {
            Some(Command::Commands { filter, run }) => {
                assert_eq!(filter.as_deref(), Some("dark"));
                assert!(!run);
            }
            _ => panic!("expected Commands command"),
        }
    }

    #[test]
    fn query_no_args() {
        let cli = Cli::try_parse_from(["waft", "query"]).expect("expected value");
        match cli.command {
            Some(Command::Query {
                entity_type,
                start,
                timeout_ms,
            }) => {
                assert_eq!(entity_type, None);
                assert!(!start);
                assert_eq!(timeout_ms, 5000);
            }
            _ => panic!("expected Query command"),
        }
    }

    #[test]
    fn query_with_entity_type() {
        let cli = Cli::try_parse_from(["waft", "query", "battery"]).expect("expected value");
        match cli.command {
            Some(Command::Query {
                entity_type,
                start,
                timeout_ms,
            }) => {
                assert_eq!(entity_type.as_deref(), Some("battery"));
                assert!(!start);
                assert_eq!(timeout_ms, 5000);
            }
            _ => panic!("expected Query command"),
        }
    }

    #[test]
    fn state_alias() {
        let cli = Cli::try_parse_from(["waft", "state", "battery"]).expect("expected value");
        match cli.command {
            Some(Command::Query { entity_type, .. }) => {
                assert_eq!(entity_type.as_deref(), Some("battery"));
            }
            _ => panic!("expected Query command via state alias"),
        }
    }

    #[test]
    fn query_with_start_flag() {
        let cli = Cli::try_parse_from(["waft", "query", "audio-device", "--start"])
            .expect("expected value");
        match cli.command {
            Some(Command::Query {
                entity_type,
                start,
                timeout_ms,
            }) => {
                assert_eq!(entity_type.as_deref(), Some("audio-device"));
                assert!(start);
                assert_eq!(timeout_ms, 5000);
            }
            _ => panic!("expected Query command"),
        }
    }

    #[test]
    fn query_with_start_short_flag() {
        let cli =
            Cli::try_parse_from(["waft", "query", "-s", "audio-device"]).expect("expected value");
        match cli.command {
            Some(Command::Query {
                entity_type, start, ..
            }) => {
                assert_eq!(entity_type.as_deref(), Some("audio-device"));
                assert!(start);
            }
            _ => panic!("expected Query command"),
        }
    }

    #[test]
    fn query_with_timeout() {
        let cli = Cli::try_parse_from([
            "waft",
            "query",
            "--start",
            "--timeout-ms",
            "10000",
            "battery",
        ])
        .expect("expected value");
        match cli.command {
            Some(Command::Query {
                entity_type,
                start,
                timeout_ms,
            }) => {
                assert_eq!(entity_type.as_deref(), Some("battery"));
                assert!(start);
                assert_eq!(timeout_ms, 10000);
            }
            _ => panic!("expected Query command"),
        }
    }

    #[test]
    fn query_with_json_flag() {
        let cli = Cli::try_parse_from(["waft", "-j", "query", "clock"]).expect("expected value");
        assert!(cli.json);
        match cli.command {
            Some(Command::Query { entity_type, .. }) => {
                assert_eq!(entity_type.as_deref(), Some("clock"));
            }
            _ => panic!("expected Query command"),
        }
    }
}
