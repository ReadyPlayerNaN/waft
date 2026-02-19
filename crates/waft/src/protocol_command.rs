use waft_protocol::entity::registry::{self, EntityTypeInfo};

/// Run the `waft protocol` command, printing entity type information to stdout.
pub fn run(json: bool, verbose: bool, entity_type_filter: Option<&str>, domain_filter: Option<&str>) {
    let all = registry::all_entity_types();

    let filtered: Vec<&EntityTypeInfo> = all
        .iter()
        .filter(|e| {
            if let Some(et) = entity_type_filter {
                return e.entity_type == et;
            }
            if let Some(d) = domain_filter {
                return e.domain == d;
            }
            true
        })
        .collect();

    if filtered.is_empty() {
        if let Some(et) = entity_type_filter {
            eprintln!("Unknown entity type: {et}");
            eprintln!("Run `waft protocol` to see all available entity types.");
        } else if let Some(d) = domain_filter {
            eprintln!("Unknown domain: {d}");
            eprintln!("Run `waft protocol` to see all available domains.");
        }
        std::process::exit(1);
    }

    if json {
        print_json(&filtered);
    } else if verbose || entity_type_filter.is_some() {
        // Single entity type filter implies verbose detail
        print_text_verbose(&filtered);
    } else {
        print_text_summary(&filtered);
    }
}

fn print_json(entries: &[&EntityTypeInfo]) {
    match serde_json::to_string_pretty(&entries) {
        Ok(json) => println!("{json}"),
        Err(e) => {
            eprintln!("[waft] failed to serialize protocol registry: {e}");
            std::process::exit(1);
        }
    }
}

fn print_text_summary(entries: &[&EntityTypeInfo]) {
    // Find max entity type length for alignment
    let max_type_len = entries
        .iter()
        .map(|e| e.entity_type.len())
        .max()
        .unwrap_or(0);

    let mut current_domain = "";
    for entry in entries {
        if entry.domain != current_domain {
            if !current_domain.is_empty() {
                println!();
            }
            println!("{}", entry.domain);
            current_domain = entry.domain;
        }
        println!(
            "  {:<width$}  {}",
            entry.entity_type,
            entry.description,
            width = max_type_len,
        );
    }
}

fn print_text_verbose(entries: &[&EntityTypeInfo]) {
    let mut current_domain = "";
    let mut first = true;

    for entry in entries {
        if entry.domain != current_domain {
            if !current_domain.is_empty() {
                println!();
            }
            println!("{}", entry.domain);
            current_domain = entry.domain;
            first = true;
        }

        if !first {
            println!();
        }
        first = false;

        println!("  {}  {}", entry.entity_type, entry.description);
        println!("    URN: {}", entry.urn_pattern);

        if !entry.properties.is_empty() {
            println!("    Properties:");
            let max_name = entry.properties.iter().map(|p| p.name.len()).max().unwrap_or(0);
            let max_type = entry
                .properties
                .iter()
                .map(|p| {
                    let t = p.type_description;
                    if p.optional { t.len() + 1 } else { t.len() }
                })
                .max()
                .unwrap_or(0);

            for prop in entry.properties {
                let type_str = if prop.optional {
                    format!("{}?", prop.type_description)
                } else {
                    prop.type_description.to_string()
                };
                println!(
                    "      {:<nwidth$}  {:<twidth$}  {}",
                    prop.name,
                    type_str,
                    prop.description,
                    nwidth = max_name,
                    twidth = max_type,
                );
            }
        }

        if !entry.actions.is_empty() {
            println!("    Actions:");
            for action in entry.actions {
                println!("      {}  {}", action.name, action.description);
                for param in action.params {
                    let req = if param.required { "required" } else { "optional" };
                    println!(
                        "        {}: {} ({})  {}",
                        param.name, param.type_description, req, param.description,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_output_is_nonempty() {
        // Capture would require stdout redirect; just verify no panic
        let all = registry::all_entity_types();
        let refs: Vec<&EntityTypeInfo> = all.iter().collect();
        assert!(!refs.is_empty());
    }

    #[test]
    fn filter_by_entity_type() {
        let all = registry::all_entity_types();
        let filtered: Vec<&EntityTypeInfo> = all
            .iter()
            .filter(|e| e.entity_type == "audio-device")
            .collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].domain, "audio");
    }

    #[test]
    fn filter_by_domain() {
        let all = registry::all_entity_types();
        let filtered: Vec<&EntityTypeInfo> = all
            .iter()
            .filter(|e| e.domain == "bluetooth")
            .collect();
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn json_serialization_roundtrip() {
        let all = registry::all_entity_types();
        let refs: Vec<&EntityTypeInfo> = all.iter().collect();
        let json = serde_json::to_string(&refs).expect("failed to serialize");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("failed to parse");
        let arr = parsed.as_array().expect("expected JSON array");
        assert_eq!(arr.len(), all.len());

        for item in arr {
            assert!(item["entity_type"].is_string(), "missing entity_type");
            assert!(item["domain"].is_string(), "missing domain");
            assert!(item["description"].is_string(), "missing description");
            assert!(item["urn_pattern"].is_string(), "missing urn_pattern");
            assert!(item["properties"].is_array(), "missing properties");
            assert!(item["actions"].is_array(), "missing actions");
        }
    }
}
