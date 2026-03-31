use colored::Colorize;

/// Maximum column width before truncation (non-last columns).
const MAX_COL_WIDTH: usize = 40;
/// Column gap (spaces between columns).
const COL_GAP: usize = 2;
/// Fallback terminal width when detection fails.
const DEFAULT_TERM_WIDTH: usize = 120;

fn term_width() -> usize {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .unwrap_or(DEFAULT_TERM_WIDTH)
}

/// Print a table with dynamic column widths, uppercase header row.
/// Non-last columns cap at MAX_COL_WIDTH. Last column fills remaining terminal width.
pub fn table(headers: &[&str], rows: &[Vec<Cell>]) {
    if rows.is_empty() {
        println!("No resources found.");
        return;
    }

    let col_count = headers.len();
    let mut widths = vec![0usize; col_count];

    // Include header widths
    for (i, h) in headers.iter().enumerate() {
        widths[i] = h.len();
    }
    // Include data widths
    for row in rows {
        for (i, cell) in row.iter().enumerate().take(col_count) {
            widths[i] = widths[i].max(cell.text.len());
        }
    }

    // Cap non-last columns
    for w in widths.iter_mut().take(col_count.saturating_sub(1)) {
        *w = (*w).min(MAX_COL_WIDTH);
    }

    // Last column: fill remaining terminal width
    if col_count > 1 {
        let used: usize = widths.iter().take(col_count - 1).map(|w| w + COL_GAP).sum();
        let remaining = term_width().saturating_sub(used + COL_GAP);
        let last = col_count - 1;
        widths[last] = widths[last].min(remaining.max(10));
    }

    // Header
    let header_line: String = headers
        .iter()
        .enumerate()
        .map(|(i, h)| format!("{:<width$}", h.to_uppercase(), width = widths[i] + 2))
        .collect();
    println!("{}", header_line.dimmed());

    // Rows
    for row in rows {
        for (i, cell) in row.iter().enumerate().take(col_count) {
            let text = if cell.text.len() > widths[i] {
                format!("{}…", &cell.text[..widths[i] - 1])
            } else {
                cell.text.clone()
            };
            let padded = format!("{:<width$}", text, width = widths[i] + 2);
            match cell.style {
                Style::Default => print!("{padded}"),
                Style::Bold => print!("{}", padded.bold()),
                Style::Dim => print!("{}", padded.dimmed()),
                Style::Green => print!("{}", padded.green()),
                Style::Yellow => print!("{}", padded.yellow()),
                Style::Blue => print!("{}", padded.blue()),
                Style::Cyan => print!("{}", padded.cyan()),
                Style::Red => print!("{}", padded.red()),
            }
        }
        println!();
    }
}

pub struct Cell {
    pub text: String,
    pub style: Style,
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum Style {
    Default,
    Bold,
    Dim,
    Green,
    Yellow,
    Blue,
    Cyan,
    Red,
}

impl Cell {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: Style::Default,
        }
    }

    pub fn styled(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }
}

/// Color an environment label
pub fn env_style(env: &str) -> Style {
    match env {
        "prod" | "production" => Style::Green,
        "preview" | "staging" => Style::Yellow,
        "dev" | "development" => Style::Blue,
        _ => Style::Default,
    }
}
