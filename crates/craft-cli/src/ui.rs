use std::io::{self, IsTerminal};
use std::time::Duration;

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

pub fn error_label(code: Option<&str>) -> String {
    match code {
        Some(code) => paint_stderr(format!("error[{code}]"), "31"),
        None => paint_stderr("error".to_string(), "31"),
    }
}

pub fn warning(message: impl AsRef<str>) {
    eprintln!(
        "{}: {}",
        paint_stderr("warning".to_string(), "33"),
        message.as_ref()
    );
}

pub fn success(message: impl AsRef<str>) {
    println!(
        "{} {}",
        paint_stdout("ok".to_string(), "32"),
        message.as_ref()
    );
}

pub fn status(message: impl AsRef<str>) {
    println!(
        "{} {}",
        paint_stdout("==>".to_string(), "36"),
        message.as_ref()
    );
}

pub fn spinner(message: impl Into<String>) -> ProgressBar {
    let spinner = ProgressBar::new_spinner();
    if io::stderr().is_terminal() {
        spinner.set_draw_target(ProgressDrawTarget::stderr_with_hz(12));
    } else {
        spinner.set_draw_target(ProgressDrawTarget::hidden());
    }
    let style = ProgressStyle::with_template("{spinner:.cyan} {msg} [{elapsed_precise}]")
        .unwrap_or_else(|_| ProgressStyle::default_spinner())
        .tick_chars("|/-\\ ");
    spinner.set_style(style);
    spinner.set_message(message.into());
    spinner.enable_steady_tick(Duration::from_millis(80));
    spinner
}

pub fn finish_spinner(spinner: ProgressBar, message: impl Into<String>) {
    if io::stderr().is_terminal() {
        spinner.finish_with_message(message.into());
    } else {
        spinner.finish_and_clear();
    }
}

pub fn table(headers: &[&str], rows: &[Vec<String>]) {
    let mut widths: Vec<usize> = headers.iter().map(|header| header.len()).collect();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if let Some(width) = widths.get_mut(index) {
                *width = (*width).max(cell.len());
            }
        }
    }

    print_row(headers, &widths);
    let separators: Vec<String> = widths.iter().map(|width| "-".repeat(*width)).collect();
    let separator_refs: Vec<&str> = separators.iter().map(String::as_str).collect();
    print_row(&separator_refs, &widths);
    for row in rows {
        let cells: Vec<&str> = row.iter().map(String::as_str).collect();
        print_row(&cells, &widths);
    }
}

fn print_row(cells: &[&str], widths: &[usize]) {
    for (index, cell) in cells.iter().enumerate() {
        if index > 0 {
            print!("  ");
        }
        let width = widths.get(index).copied().unwrap_or(cell.len());
        print!("{cell:<width$}");
    }
    println!();
}

fn paint_stdout(value: String, color: &str) -> String {
    if io::stdout().is_terminal() {
        format!("\x1b[{color}m{value}\x1b[0m")
    } else {
        value
    }
}

fn paint_stderr(value: String, color: &str) -> String {
    if io::stderr().is_terminal() {
        format!("\x1b[{color}m{value}\x1b[0m")
    } else {
        value
    }
}
