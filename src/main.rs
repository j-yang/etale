mod convert;
mod inline;
mod lcs;
mod lines;
mod unified;

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use tate::patch;
use tate::section::Value;
use tate::tree::{self, ChangeKind, TreeNode};

#[derive(Serialize, Deserialize)]
struct JsonPatch {
    edits: Vec<JsonEdit>,
}

#[derive(Serialize, Deserialize)]
struct JsonEdit {
    location: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    old: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new: Option<Value>,
}

fn patch_to_json(p: &patch::Patch) -> JsonPatch {
    JsonPatch {
        edits: p.edits.iter().map(|(loc, edit)| JsonEdit {
            location: loc.clone(),
            old: edit.old.clone(),
            new: edit.new.clone(),
        }).collect(),
    }
}

fn patch_from_json(jp: JsonPatch) -> patch::Patch {
    let mut edits = std::collections::BTreeMap::new();
    for e in jp.edits {
        edits.insert(e.location, patch::PointEdit { old: e.old, new: e.new });
    }
    patch::Patch { edits }
}

#[derive(Parser)]
#[command(
    name = "etale",
    version,
    about = "Structural diff and merge for JSON, YAML, TOML, and text — powered by tate's patch algebra"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Diff two files (auto-detect: JSON, YAML, TOML, or text)
    Diff {
        a: String,
        b: String,
        #[arg(long)]
        json: bool,
    },
    /// Diff two JSON trees from stdin: {"base":{}, "other":{}}
    TreeDiff,
    /// 3-way merge JSON trees from stdin: {"base":{}, "ours":{}, "theirs":{}}
    TreeMerge,
    /// Lossless patch algebra (diff/apply/invert/compose)
    Patch {
        #[command(subcommand)]
        action: PatchCmd,
    },
    /// Git external diff driver (called by git via diff.<name>.command)
    GitDiff {
        #[arg(allow_hyphen_values = true, num_args = 7)]
        args: Vec<String>,
    },
    /// Git merge driver: base ours theirs (writes result to ours)
    GitMerge {
        base: String,
        ours: String,
        theirs: String,
    },
}

#[derive(Subcommand)]
enum PatchCmd {
    /// Generate a lossless patch between two files
    Diff { a: String, b: String },
    /// Apply a patch file to an input file, writing the result to stdout
    Apply { patch: String, input: String },
    /// Invert a patch (swap old/new in every edit)
    Invert { patch: String },
    /// Compose two patches sequentially (first then second)
    Compose { first: String, second: String },
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Diff { a, b, json } => cmd_diff(&a, &b, json),
        Command::TreeDiff => cmd_tree_diff(),
        Command::TreeMerge => cmd_tree_merge(),
        Command::Patch { action } => match action {
            PatchCmd::Diff { a, b } => cmd_patch_diff(&a, &b),
            PatchCmd::Apply { patch, input } => cmd_patch_apply(&patch, &input),
            PatchCmd::Invert { patch } => cmd_patch_invert(&patch),
            PatchCmd::Compose { first, second } => cmd_patch_compose(&first, &second),
        },
        Command::GitDiff { args } => cmd_git_diff(&args),
        Command::GitMerge { base, ours, theirs } => cmd_git_merge(&base, &ours, &theirs),
    };
    if let Err(e) = result {
        eprintln!("etale: {e}");
        std::process::exit(1);
    }
}

// ─── diff ────────────────────────────────────────────────────────────────────

fn cmd_diff(a: &str, b: &str, json: bool) -> Result<(), String> {
    let fmt = convert::detect(a);
    match fmt {
        convert::Format::Text => cmd_text_diff(a, b),
        _ => {
            let ta = convert::file_to_tree(a, fmt)?;
            let tb = convert::file_to_tree(b, fmt)?;
            let diff = tree::tree_diff(&ta, &tb);
            if json {
                println!("{}", serde_json::to_string_pretty(&diff).map_err(|e| e.to_string())?);
            } else {
                print_structural_diff(a, b, &diff);
            }
            Ok(())
        }
    }
}

fn print_structural_diff(a: &str, b: &str, diff: &tree::TreeDiff) {
    println!("diff --etale {} {}", a, b);
    if diff.changes.is_empty() {
        println!("(no changes)");
        return;
    }
    for c in &diff.changes {
        let kind = match c.kind {
            ChangeKind::Added => "added",
            ChangeKind::Removed => "removed",
            ChangeKind::Modified => "modified",
        };
        let path = if c.path.is_empty() {
            c.id.clone()
        } else {
            c.path.last().cloned().unwrap_or_default()
        };
        match c.kind {
            ChangeKind::Added | ChangeKind::Removed => {
                println!("  {kind:10} {path}");
            }
            ChangeKind::Modified => {
                for attr in &c.changed_attrs {
                    if attr.name == "value" {
                        println!("  {kind:10} {path:30} {} → {}", attr.old, attr.new);
                    } else {
                        println!("  {kind:10} {path}.{}: {} → {}", attr.name, attr.old, attr.new);
                    }
                }
                if let Some((old, new)) = &c.changed_text {
                    if c.changed_attrs.is_empty() {
                        println!("  {kind:10} {path:30} {old} → {new}");
                    }
                }
            }
        }
    }
}

fn cmd_text_diff(a: &str, b: &str) -> Result<(), String> {
    let lines_a = read_lines(a)?;
    let lines_b = read_lines(b)?;
    let ops = lines::diff(&lines_a, &lines_b);
    let paired = inline::pair_replacements(ops, inline::DEFAULT_SIMILARITY);
    let unified = unified::to_unified(&paired, 3);
    if unified.is_empty() {
        println!("diff --etale {} {}", a, b);
        println!("(no changes)");
    } else {
        print!("diff --etale {} {}\n{}", a, b, unified);
    }
    Ok(())
}

fn read_lines(path: &str) -> Result<Vec<String>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
    let text = String::from_utf8_lossy(&bytes);
    let normalized = text.replace("\r\n", "\n");
    let mut lines: Vec<String> = normalized.split('\n').map(String::from).collect();
    if lines.last().map(|s| s.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    Ok(lines)
}

// ─── tree-diff / tree-merge (stdin JSON API) ─────────────────────────────────

fn read_stdin_json() -> Result<serde_json::Value, String> {
    let mut input = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)
        .map_err(|e| format!("read stdin: {e}"))?;
    if input.is_empty() {
        return Err("no input on stdin".into());
    }
    serde_json::from_str(&input).map_err(|e| format!("parse JSON: {e}"))
}

fn get_field<'a>(input: &'a serde_json::Value, key: &str) -> Result<&'a serde_json::Value, String> {
    input.get(key).ok_or_else(|| format!("missing '{key}' in input JSON"))
}

fn value_to_tree(v: &serde_json::Value) -> TreeNode {
    convert::from_json_value("root", v)
}

fn cmd_tree_diff() -> Result<(), String> {
    let input = read_stdin_json()?;
    let base = value_to_tree(get_field(&input, "base")?);
    let other = value_to_tree(get_field(&input, "other")?);
    let diff = tree::tree_diff(&base, &other);
    println!("{}", serde_json::to_string_pretty(&diff).map_err(|e| e.to_string())?);
    Ok(())
}

fn cmd_tree_merge() -> Result<(), String> {
    let input = read_stdin_json()?;
    let base = value_to_tree(get_field(&input, "base")?);
    let ours = value_to_tree(get_field(&input, "ours")?);
    let theirs = value_to_tree(get_field(&input, "theirs")?);
    let result = tree::tree_merge(&base, &ours, &theirs);
    println!("{}", serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?);
    Ok(())
}

// ─── patch algebra ───────────────────────────────────────────────────────────

fn read_patch(path: &str) -> Result<patch::Patch, String> {
    let s = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
    let jp: JsonPatch = serde_json::from_str(&s).map_err(|e| format!("parse patch: {e}"))?;
    Ok(patch_from_json(jp))
}

fn write_patch(p: &patch::Patch) -> Result<String, String> {
    let jp = patch_to_json(p);
    serde_json::to_string_pretty(&jp).map_err(|e| format!("serialize patch: {e}"))
}

fn tree_to_string(tree: &TreeNode, fmt: convert::Format) -> Result<String, String> {
    match fmt {
        convert::Format::Json => Ok(convert::tree_to_json_pretty(tree)),
        convert::Format::Yaml => {
            let jv = convert::tree_to_json_value(tree);
            serde_yaml::to_string(&jv).map_err(|e| format!("serialize YAML: {e}"))
        }
        convert::Format::Toml => {
            let jv = convert::tree_to_json_value(tree);
            toml::to_string(&jv).map_err(|e| format!("serialize TOML: {e}"))
        }
        convert::Format::Text => Err("text format has no tree serialization".into()),
    }
}

fn cmd_patch_diff(a: &str, b: &str) -> Result<(), String> {
    let fmt = convert::detect(a);
    let ta = convert::file_to_tree(a, fmt)?;
    let tb = convert::file_to_tree(b, fmt)?;
    let p = patch::diff(&ta, &tb);
    println!("{}", write_patch(&p)?);
    Ok(())
}

fn cmd_patch_apply(patch_path: &str, input_path: &str) -> Result<(), String> {
    let p = read_patch(patch_path)?;
    let fmt = convert::detect(input_path);
    let tree = convert::file_to_tree(input_path, fmt)?;
    let result = patch::apply(&p, &tree).map_err(|e| format!("apply failed: {e}"))?;
    println!("{}", tree_to_string(&result, fmt)?);
    Ok(())
}

fn cmd_patch_invert(patch_path: &str) -> Result<(), String> {
    let p = read_patch(patch_path)?;
    let inv = patch::invert(&p);
    println!("{}", write_patch(&inv)?);
    Ok(())
}

fn cmd_patch_compose(first: &str, second: &str) -> Result<(), String> {
    let p1 = read_patch(first)?;
    let p2 = read_patch(second)?;
    let composed = patch::compose(&p1, &p2);
    println!("{}", write_patch(&composed)?);
    Ok(())
}

// ─── git integration ─────────────────────────────────────────────────────────

fn cmd_git_diff(args: &[String]) -> Result<(), String> {
    if args.len() < 7 {
        return Err("git-diff needs 7 arguments (path old-file old-hex old-mode new-file new-hex new-mode)".into());
    }
    let path = &args[0];
    let old_file = &args[1];
    let new_file = &args[4];
    let fmt = convert::detect(path);

    match fmt {
        convert::Format::Text => {
            let lines_a = read_lines(old_file)?;
            let lines_b = read_lines(new_file)?;
            let ops = lines::diff(&lines_a, &lines_b);
            let paired = inline::pair_replacements(ops, inline::DEFAULT_SIMILARITY);
            let unified = unified::to_unified(&paired, 3);
            print!("diff --etale a/{path} b/{path}\n{unified}");
        }
        _ => {
            let ta = convert::file_to_tree(old_file, fmt)?;
            let tb = convert::file_to_tree(new_file, fmt)?;
            let diff = tree::tree_diff(&ta, &tb);
            print!("diff --etale a/{path} b/{path}\n");
            if diff.changes.is_empty() {
                println!("(no changes)");
            } else {
                for c in &diff.changes {
                    let kind = match c.kind {
                        ChangeKind::Added => "+",
                        ChangeKind::Removed => "-",
                        ChangeKind::Modified => "~",
                    };
                    let loc = c.path.last().cloned().unwrap_or_else(|| c.id.clone());
                    println!("  {kind} {loc}");
                }
            }
        }
    }
    Ok(())
}

fn cmd_git_merge(base: &str, ours: &str, theirs: &str) -> Result<(), String> {
    let fmt = convert::detect(ours);
    let tb = convert::file_to_tree(base, fmt)?;
    let to = convert::file_to_tree(ours, fmt)?;
    let tt = convert::file_to_tree(theirs, fmt)?;
    let result = tree::tree_merge(&tb, &to, &tt);

    let output = match fmt {
        convert::Format::Text => {
            return Err("etale git-merge does not support text format — use git's built-in merge".into());
        }
        _ => tree_to_string(&result.tree, fmt)?,
    };

    std::fs::write(ours, &output).map_err(|e| format!("write {ours}: {e}"))?;

    if result.conflicts.is_empty() {
        Ok(())
    } else {
        for c in &result.conflicts {
            let loc = c.path.last().cloned().unwrap_or_default();
            eprintln!("CONFLICT ({:?}): {} {}", c.kind, loc, c.attr);
        }
        std::process::exit(1);
    }
}
