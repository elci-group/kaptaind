pub fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    if headers.is_empty() {
        return;
    }

    let mut widths: Vec<usize> = headers.iter().map(|h| visible_width(h)).collect();
    for row in rows {
        for (idx, cell) in row.iter().enumerate() {
            if let Some(width) = widths.get_mut(idx) {
                *width = (*width).max(visible_width(cell));
            }
        }
    }

    println!("{}", render_separator(&widths));
    println!("{}", render_row(headers, &widths));
    println!("{}", render_separator(&widths));
    for row in rows {
        println!("{}", render_row(row, &widths));
    }
    println!("{}", render_separator(&widths));
}

fn render_separator(widths: &[usize]) -> String {
    let parts: Vec<String> = widths.iter().map(|width| "─".repeat(width + 2)).collect();
    format!("┼{}┼", parts.join("┼"))
}

fn render_row<T: AsRef<str>>(cells: &[T], widths: &[usize]) -> String {
    let mut rendered = Vec::with_capacity(widths.len());
    for (idx, width) in widths.iter().enumerate() {
        let cell = cells.get(idx).map(AsRef::as_ref).unwrap_or("");
        let padding = width.saturating_sub(visible_width(cell));
        rendered.push(format!(" {}{} ", cell, " ".repeat(padding)));
    }
    format!("│{}│", rendered.join("│"))
}

fn visible_width(input: &str) -> usize {
    let mut width = 0;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for seq in chars.by_ref() {
                if seq.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        width += 1;
    }

    width
}
