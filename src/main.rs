use clap::{Parser, Subcommand};
use serde_json::Value;
use std::io::{self, Read};

#[derive(Parser)]
#[command(
    name = "etale",
    version,
    about = "Structured diff and merge for files and data — powered by tate + mumford"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Diff two files (auto-detect format: xlsx, docx, pptx, pdf, text)
    Diff {
        a: String,
        b: String,
        #[arg(long)]
        json: bool,
    },
    /// Diff two grids from JSON stdin: {"base": [[...]], "other": [[...]]}
    GridDiff,
    /// Diff two JSON trees from stdin: {"base": {...}, "other": {...}}
    TreeDiff,
    /// 3-way merge JSON trees from stdin: {"base":{}, "ours":{}, "theirs":{}}
    ///
    /// This is the single 3-way merge. Grid and sequence inputs are merged by
    /// keying them into trees first (see tate's keying adapters), not by a
    /// separate merge algorithm.
    TreeMerge,
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Diff { a, b, json } => cmd_diff(&a, &b, json),
        Command::GridDiff => cmd_grid_diff(),
        Command::TreeDiff => cmd_tree_diff(),
        Command::TreeMerge => cmd_tree_merge(),
    };
    if let Err(e) = result {
        eprintln!("etale: {e}");
        std::process::exit(1);
    }
}

// ─── stdin helpers ───────────────────────────────────────────────────────────

fn read_stdin_json() -> Result<Value, String> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| format!("read stdin: {e}"))?;
    if input.is_empty() {
        return Err("no input on stdin".into());
    }
    serde_json::from_str(&input).map_err(|e| format!("parse JSON: {e}"))
}

fn json_to_grid(v: &Value) -> Vec<Vec<String>> {
    v.as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    row.as_array()
                        .map(|cells| {
                            cells
                                .iter()
                                .map(|c| match c {
                                    Value::String(s) => s.clone(),
                                    _ => c.to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default()
}

fn get_field<'a>(input: &'a Value, key: &str) -> Result<&'a Value, String> {
    input
        .get(key)
        .ok_or_else(|| format!("missing '{key}' in input JSON"))
}

fn json_to_pretty<T: serde::Serialize>(val: &T) -> Result<String, String> {
    serde_json::to_string_pretty(val).map_err(|e| format!("serialize: {e}"))
}

// ─── diff ────────────────────────────────────────────────────────────────────

fn cmd_diff(a: &str, b: &str, json: bool) -> Result<(), String> {
    let result = mumford::dispatch(a, b)?;
    if json {
        println!("{}", json_to_pretty(&result)?);
    } else {
        print_diff_human(&result);
    }
    Ok(())
}

fn print_diff_human(result: &mumford::DiffResult) {
    if !result.error.is_empty() {
        eprintln!("{}", result.error);
        return;
    }

    println!("diff --etale {} {}", result.path_a, result.path_b);

    if let Some(text) = &result.text {
        println!("format: text");
        let unified = tate::unified::to_unified(&text.ops, 3);
        if unified.is_empty() {
            println!("(no changes)");
        } else {
            print!("{unified}");
        }
    } else if let Some(excel) = &result.excel {
        println!("format: excel");
        for sheet in &excel.sheets {
            let g = &sheet.grid;
            if sheet.status == "equal" {
                println!("  sheet \"{}\": equal", sheet.name);
            } else {
                println!(
                    "  sheet \"{}\": {} (+{} −{} ~{} rows)",
                    sheet.name, sheet.status, g.added_rows, g.removed_rows, g.modified_rows
                );
            }
        }
    } else if let Some(docx) = &result.docx {
        println!("format: docx");
        println!(
            "  {} added, {} modified, {} deleted paragraphs",
            docx.added_p.len(),
            docx.modified_p.len(),
            docx.deleted_p.len()
        );
    } else if let Some(_) = &result.pptx {
        println!("format: pptx");
        println!("  (use --json for details)");
    }
}

// ─── grid diff / merge ───────────────────────────────────────────────────────

fn cmd_grid_diff() -> Result<(), String> {
    let input = read_stdin_json()?;
    let base = json_to_grid(get_field(&input, "base")?);
    let other = json_to_grid(get_field(&input, "other")?);
    let diff = tate::grid::grid_diff(&base, &other, &tate::grid::GridOptions::default());
    println!("{}", json_to_pretty(&diff)?);
    Ok(())
}

// ─── tree diff / merge ───────────────────────────────────────────────────────

fn cmd_tree_diff() -> Result<(), String> {
    let input = read_stdin_json()?;
    let base = serde_json::to_string(get_field(&input, "base")?)
        .map_err(|e| format!("serialize base: {e}"))?;
    let other = serde_json::to_string(get_field(&input, "other")?)
        .map_err(|e| format!("serialize other: {e}"))?;
    let diff = mumford::json::json_diff(&base, &other)?;
    println!("{}", json_to_pretty(&diff)?);
    Ok(())
}

fn cmd_tree_merge() -> Result<(), String> {
    let input = read_stdin_json()?;
    let base_str = serde_json::to_string(get_field(&input, "base")?)
        .map_err(|e| format!("serialize base: {e}"))?;
    let ours_str = serde_json::to_string(get_field(&input, "ours")?)
        .map_err(|e| format!("serialize ours: {e}"))?;
    let theirs_str = serde_json::to_string(get_field(&input, "theirs")?)
        .map_err(|e| format!("serialize theirs: {e}"))?;
    let base = mumford::json::json_to_tree(&base_str)?;
    let ours = mumford::json::json_to_tree(&ours_str)?;
    let theirs = mumford::json::json_to_tree(&theirs_str)?;
    let result = tate::tree::tree_merge(&base, &ours, &theirs);
    println!("{}", json_to_pretty(&result)?);
    Ok(())
}
