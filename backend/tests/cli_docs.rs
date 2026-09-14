use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

/**
 * Every command line the documentation shows must exist in the real binary, with the flags it
 * shows. Documentation drifts silently otherwise: a renamed flag would only be noticed by a user
 * following the guide on a phone, which is the worst place to find out.
 */
fn help(directory: &Path, path: &[String]) -> String {
    let output: Output = Command::new(env!("CARGO_BIN_EXE_justlocationd"))
        .args(["--data-dir", directory.to_str().unwrap(), "--json"])
        .args(path)
        .arg("--help")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "help failed for {:?}: {}",
        path,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/** Command names offered by a help screen, ignoring the `help` entry clap always adds. */
fn subcommands(help: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut in_commands = false;
    for line in help.lines() {
        if line.starts_with("Commands:") {
            in_commands = true;
            continue;
        }
        if in_commands {
            if line.trim().is_empty() || !line.starts_with("  ") {
                in_commands = false;
                continue;
            }
            if let Some(name) = line.split_whitespace().next() {
                if name != "help" {
                    names.insert(name.to_owned());
                }
            }
        }
    }
    names
}

fn flags(help: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut in_options = false;
    for line in help.lines() {
        if line.trim_start().starts_with("Options:") {
            in_options = true;
            continue;
        }
        // A section header ends the list; option lines are the ones that start with a dash.
        if in_options && !line.trim_start().starts_with('-') && !line.starts_with(' ') {
            in_options = false;
        }
        if !in_options {
            continue;
        }
        let trimmed = line.trim_start();
        if !trimmed.starts_with('-') {
            continue;
        }
        for token in trimmed.split([',', ' ']) {
            let token = token.trim();
            if token.starts_with("--") {
                names.insert(token.split('=').next().unwrap().to_owned());
            }
        }
    }
    names
}

struct Usage {
    path: Vec<String>,
    flags: Vec<String>,
    file: String,
}

/** `justlocation` command lines shown in the documentation, in their written order. */
fn usages() -> Vec<Usage> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
    let mut documents = Vec::new();
    for name in ["README.md", "README.zh.md", "docs/roadmap.md", "docs/places.md", "docs/captures.md"] {
        documents.push(root.join(name));
    }
    let mut found = Vec::new();
    for document in documents {
        let text = fs::read_to_string(&document).unwrap();
        for line in text.lines() {
            let trimmed = line.trim_start_matches([' ', '-', '*', '>']).trim();
            for prefix in ["justlocation ", "justlocationd "] {
                let Some(rest) = trimmed.strip_prefix(prefix) else { continue };
                let rest = rest.split('#').next().unwrap().trim();
                let words: Vec<&str> = rest.split_whitespace().collect();
                // A placeholder like COMMAND or ARGS is not a command line to check.
                let mut path = Vec::new();
                for word in &words {
                    if word.starts_with('-') || word.chars().any(|c| c.is_ascii_uppercase()) {
                        break;
                    }
                    path.push((*word).to_owned());
                }
                if path.is_empty() {
                    continue;
                }
                let flags = words
                    .iter()
                    .filter(|word| word.starts_with("--") && word.len() > 2)
                    .map(|word| word.split('=').next().unwrap().to_owned())
                    .collect();
                found.push(Usage {
                    path,
                    flags,
                    file: document.file_name().unwrap().to_string_lossy().into_owned(),
                });
            }
        }
    }
    assert!(!found.is_empty(), "no documented command lines were found");
    found
}

#[test]
fn every_documented_command_line_exists_with_the_flags_it_shows() {
    let directory =
        std::env::temp_dir().join(format!("justlocation-cli-docs-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let mut checked = 0;
    for usage in usages() {
        // Resolve the command path step by step, so a renamed subcommand is reported at the depth
        // where it broke rather than as a generic failure.
        let mut path = Vec::new();
        let root_help = help(&directory, &path);
        let mut available = subcommands(&root_help);
        let mut current = root_help;
        for name in usage.path.iter() {
            assert!(
                available.contains(name),
                "{}: command `{}` does not exist (path: {})",
                usage.file,
                name,
                usage.path.join(" ")
            );
            path.push(name.clone());
            current = help(&directory, &path);
            available = subcommands(&current);
        }
        let options = flags(&current);
        for flag in &usage.flags {
            // `--input` and `--output` take a path in the examples and are part of the surface the
            // help screen must still offer.
            assert!(
                options.contains(flag),
                "{}: `{}` has no `{}`\n{}",
                usage.file,
                usage.path.join(" "),
                flag,
                current
            );
        }
        checked += 1;
    }
    assert!(checked >= 8, "only {checked} documented command lines were checked");
    fs::remove_dir_all(&directory).unwrap();
}
