//! Bark Calculator Plugin — Programmer's calculator with DEC and HEX modes.
//!
//! Overlay plugin protocol: reads JSON commands from stdin, writes JSON responses to stdout.

use std::io::{self, BufRead, Write};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--plugin-info") {
        println!(
            r#"{{"name":"Calculator","version":"1.0.0","type":"overlay","description":"Programmer calculator (DEC/HEX)","width":48,"height":20}}"#
        );
        return;
    }

    // Interactive session: read JSON commands from stdin
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();
    let mut state = CalcState::new();

    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let command = extract_str(line, "command").unwrap_or_default();

        match command.as_str() {
            "init" => {
                let w = extract_int(line, "width").unwrap_or(48) as u16;
                let h = extract_int(line, "height").unwrap_or(20) as u16;
                state.width = w;
                state.height = h;
                let response = state.render();
                let _ = writeln!(writer, "{}", response);
                let _ = writer.flush();
            }
            "key" => {
                let key = extract_str(line, "key").unwrap_or_default();
                let mods = extract_str_array(line, "modifiers");
                let response = state.handle_key(&key, &mods);
                let _ = writeln!(writer, "{}", response);
                let _ = writer.flush();
            }
            "close" => {
                break;
            }
            _ => {
                let _ = writeln!(writer, r#"{{"close":true}}"#);
                let _ = writer.flush();
                break;
            }
        }
    }
}

// ============================================================================
// Calculator State
// ============================================================================

#[derive(Clone, Copy, PartialEq)]
enum CalcMode {
    Dec,
    Hex,
}

struct CalcState {
    mode: CalcMode,
    expression: String,
    result: String,
    history: Vec<(String, String)>, // (expression, result)
    width: u16,
    height: u16,
    error: Option<String>,
}

impl CalcState {
    fn new() -> Self {
        Self {
            mode: CalcMode::Dec,
            expression: String::new(),
            result: String::new(),
            history: Vec::new(),
            width: 48,
            height: 20,
            error: None,
        }
    }

    fn handle_key(&mut self, key: &str, modifiers: &[String]) -> String {
        let ctrl = modifiers.iter().any(|m| m == "ctrl");

        self.error = None;

        match key {
            "Escape" => {
                return r#"{"close":true}"#.to_string();
            }
            "Tab" => {
                self.mode = match self.mode {
                    CalcMode::Dec => CalcMode::Hex,
                    CalcMode::Hex => CalcMode::Dec,
                };
                // Clear expression when switching modes
                self.expression.clear();
                self.result.clear();
            }
            "Enter" => {
                if ctrl {
                    return r#"{"close":true}"#.to_string();
                }
                self.evaluate();
            }
            "Backspace" => {
                self.expression.pop();
            }
            "c" | "C" if !ctrl => {
                if self.mode == CalcMode::Dec {
                    // In DEC mode, 'c' clears
                    self.expression.clear();
                    self.result.clear();
                } else {
                    // In HEX mode, c/C are hex digits
                    self.expression.push(key.chars().next().unwrap_or('c'));
                }
            }
            _ if key.len() == 1 => {
                let ch = key.chars().next().unwrap_or(' ');
                if self.is_valid_char(ch) {
                    self.expression.push(ch);
                }
            }
            _ => {}
        }

        self.render()
    }

    fn is_valid_char(&self, ch: char) -> bool {
        match self.mode {
            CalcMode::Dec => {
                matches!(ch, '0'..='9' | '+' | '-' | '*' | '/' | '(' | ')' | '.' | ' ')
            }
            CalcMode::Hex => {
                matches!(ch,
                    '0'..='9' | 'a'..='f' | 'A'..='F' |
                    '&' | '|' | '^' | '~' | '<' | '>' | '+' | '-' | '*' | '/' |
                    '(' | ')' | ' '
                )
            }
        }
    }

    fn evaluate(&mut self) {
        let expr = self.expression.trim().to_string();
        if expr.is_empty() {
            return;
        }

        let result = match self.mode {
            CalcMode::Dec => self.eval_dec(&expr),
            CalcMode::Hex => self.eval_hex(&expr),
        };

        match result {
            Ok(display) => {
                self.history.push((expr, display.clone()));
                self.result = display;
                self.expression.clear();
            }
            Err(e) => {
                self.error = Some(e);
            }
        }
    }

    fn eval_dec(&self, expr: &str) -> Result<String, String> {
        let val = parse_dec_expr(expr)?;
        // Show integer if whole, otherwise float
        if val.fract().abs() < 1e-10 && val.abs() < i64::MAX as f64 {
            Ok(format!("{}", val as i64))
        } else {
            // Trim trailing zeros after decimal point
            let s = format!("{:.10}", val);
            let s = s.trim_end_matches('0');
            let s = s.trim_end_matches('.');
            Ok(s.to_string())
        }
    }

    fn eval_hex(&self, expr: &str) -> Result<String, String> {
        let val = parse_hex_expr(expr)?;
        Ok(format!("{:X} = {}", val as u64, val))
    }

    fn render(&self) -> String {
        let mode_str = match self.mode {
            CalcMode::Dec => "DEC",
            CalcMode::Hex => "HEX",
        };
        let title = format!(" Calculator [{}] ", mode_str);
        let inner_w = self.width.saturating_sub(2) as usize;

        let mut lines: Vec<String> = Vec::new();

        // Line 0: Mode indicator
        let mode_line = match self.mode {
            CalcMode::Dec => "Mode: DECIMAL   (Tab to switch)",
            CalcMode::Hex => "Mode: HEXADECIMAL (Tab to switch)",
        };
        lines.push(truncate(mode_line, inner_w));

        // Line 1: separator
        lines.push("\u{2500}".repeat(inner_w));

        // Lines 2+: History (show last entries that fit)
        let history_height = self.height.saturating_sub(10) as usize;
        let start = self.history.len().saturating_sub(history_height);
        for (expr, result) in &self.history[start..] {
            let line = format!("  {} = {}", expr, result);
            lines.push(truncate(&line, inner_w));
        }

        // Pad to push expression/result to a consistent position
        let target_line = 2 + history_height;
        while lines.len() < target_line {
            lines.push(String::new());
        }

        // Separator before input
        lines.push("\u{2500}".repeat(inner_w));

        // Result line
        if let Some(ref err) = self.error {
            lines.push(truncate(&format!("  Error: {}", err), inner_w));
        } else if !self.result.is_empty() {
            lines.push(truncate(&format!("  = {}", self.result), inner_w));
        } else {
            lines.push(String::new());
        }

        // Expression input line with cursor
        let expr_display = format!("  > {}|", self.expression);
        lines.push(truncate(&expr_display, inner_w));

        // Empty line
        lines.push(String::new());

        // Help line
        let help = match self.mode {
            CalcMode::Dec => "Tab=HEX  Enter=Calc  C=Clear  Esc=Close",
            CalcMode::Hex => "Tab=DEC  Enter=Calc  &|^~<>=Ops  Esc=Close",
        };
        lines.push(truncate(help, inner_w));

        // Pad to fill height
        let content_height = self.height.saturating_sub(2) as usize;
        while lines.len() < content_height {
            lines.push(String::new());
        }
        lines.truncate(content_height);

        // Build JSON response
        let lines_json: Vec<String> = lines.iter()
            .map(|l| format!("\"{}\"", escape_json(l)))
            .collect();

        format!(
            r#"{{"title":"{}","width":{},"height":{},"close":false,"lines":[{}]}}"#,
            escape_json(&title),
            self.width,
            self.height,
            lines_json.join(",")
        )
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        s[..max].to_string()
    }
}

// ============================================================================
// DEC Expression Parser (recursive descent, supports +, -, *, /, parens, decimals)
// ============================================================================

fn parse_dec_expr(input: &str) -> Result<f64, String> {
    let tokens = tokenize_dec(input)?;
    let mut pos = 0;
    let result = parse_dec_add_sub(&tokens, &mut pos)?;
    if pos < tokens.len() {
        return Err(format!("Unexpected '{}'", tokens[pos]));
    }
    Ok(result)
}

fn tokenize_dec(input: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }
        if ch.is_ascii_digit() || ch == '.' {
            let mut num = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_ascii_digit() || c == '.' {
                    num.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            tokens.push(num);
        } else if "+-*/()".contains(ch) {
            tokens.push(ch.to_string());
            chars.next();
        } else {
            return Err(format!("Invalid character: {}", ch));
        }
    }
    Ok(tokens)
}

fn parse_dec_add_sub(tokens: &[String], pos: &mut usize) -> Result<f64, String> {
    let mut left = parse_dec_mul_div(tokens, pos)?;
    while *pos < tokens.len() && (tokens[*pos] == "+" || tokens[*pos] == "-") {
        let op = tokens[*pos].clone();
        *pos += 1;
        let right = parse_dec_mul_div(tokens, pos)?;
        left = if op == "+" { left + right } else { left - right };
    }
    Ok(left)
}

fn parse_dec_mul_div(tokens: &[String], pos: &mut usize) -> Result<f64, String> {
    let mut left = parse_dec_unary(tokens, pos)?;
    while *pos < tokens.len() && (tokens[*pos] == "*" || tokens[*pos] == "/") {
        let op = tokens[*pos].clone();
        *pos += 1;
        let right = parse_dec_unary(tokens, pos)?;
        if op == "/" && right == 0.0 {
            return Err("Division by zero".to_string());
        }
        left = if op == "*" { left * right } else { left / right };
    }
    Ok(left)
}

fn parse_dec_unary(tokens: &[String], pos: &mut usize) -> Result<f64, String> {
    if *pos < tokens.len() && tokens[*pos] == "-" {
        *pos += 1;
        let val = parse_dec_atom(tokens, pos)?;
        Ok(-val)
    } else if *pos < tokens.len() && tokens[*pos] == "+" {
        *pos += 1;
        parse_dec_atom(tokens, pos)
    } else {
        parse_dec_atom(tokens, pos)
    }
}

fn parse_dec_atom(tokens: &[String], pos: &mut usize) -> Result<f64, String> {
    if *pos >= tokens.len() {
        return Err("Unexpected end of expression".to_string());
    }

    if tokens[*pos] == "(" {
        *pos += 1;
        let result = parse_dec_add_sub(tokens, pos)?;
        if *pos >= tokens.len() || tokens[*pos] != ")" {
            return Err("Missing closing parenthesis".to_string());
        }
        *pos += 1;
        return Ok(result);
    }

    let val: f64 = tokens[*pos].parse()
        .map_err(|_| format!("Invalid number: {}", tokens[*pos]))?;
    *pos += 1;
    Ok(val)
}

// ============================================================================
// HEX Expression Parser (supports +, -, *, /, &, |, ^, ~, <<, >>, parens)
// ============================================================================

fn parse_hex_expr(input: &str) -> Result<i64, String> {
    let tokens = tokenize_hex(input)?;
    let mut pos = 0;
    let result = parse_hex_or(&tokens, &mut pos)?;
    if pos < tokens.len() {
        return Err(format!("Unexpected '{}'", tokens[pos]));
    }
    Ok(result)
}

fn tokenize_hex(input: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }
        if ch.is_ascii_hexdigit() {
            let mut num = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_ascii_hexdigit() {
                    num.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            tokens.push(num);
        } else if ch == '<' {
            chars.next();
            if chars.peek() == Some(&'<') {
                chars.next();
                tokens.push("<<".to_string());
            } else {
                return Err("Expected << for shift left".to_string());
            }
        } else if ch == '>' {
            chars.next();
            if chars.peek() == Some(&'>') {
                chars.next();
                tokens.push(">>".to_string());
            } else {
                return Err("Expected >> for shift right".to_string());
            }
        } else if "&|^~+-*/()".contains(ch) {
            tokens.push(ch.to_string());
            chars.next();
        } else {
            return Err(format!("Invalid character: {}", ch));
        }
    }
    Ok(tokens)
}

// Precedence (low to high): | -> ^ -> & -> << >> -> + - -> * / -> unary ~ -
fn parse_hex_or(tokens: &[String], pos: &mut usize) -> Result<i64, String> {
    let mut left = parse_hex_xor(tokens, pos)?;
    while *pos < tokens.len() && tokens[*pos] == "|" {
        *pos += 1;
        let right = parse_hex_xor(tokens, pos)?;
        left |= right;
    }
    Ok(left)
}

fn parse_hex_xor(tokens: &[String], pos: &mut usize) -> Result<i64, String> {
    let mut left = parse_hex_and(tokens, pos)?;
    while *pos < tokens.len() && tokens[*pos] == "^" {
        *pos += 1;
        let right = parse_hex_and(tokens, pos)?;
        left ^= right;
    }
    Ok(left)
}

fn parse_hex_and(tokens: &[String], pos: &mut usize) -> Result<i64, String> {
    let mut left = parse_hex_shift(tokens, pos)?;
    while *pos < tokens.len() && tokens[*pos] == "&" {
        *pos += 1;
        let right = parse_hex_shift(tokens, pos)?;
        left &= right;
    }
    Ok(left)
}

fn parse_hex_shift(tokens: &[String], pos: &mut usize) -> Result<i64, String> {
    let mut left = parse_hex_add_sub(tokens, pos)?;
    while *pos < tokens.len() && (tokens[*pos] == "<<" || tokens[*pos] == ">>") {
        let op = tokens[*pos].clone();
        *pos += 1;
        let right = parse_hex_add_sub(tokens, pos)?;
        if right < 0 || right > 63 {
            return Err("Shift amount must be 0-63".to_string());
        }
        left = if op == "<<" { left << right } else { left >> right };
    }
    Ok(left)
}

fn parse_hex_add_sub(tokens: &[String], pos: &mut usize) -> Result<i64, String> {
    let mut left = parse_hex_mul_div(tokens, pos)?;
    while *pos < tokens.len() && (tokens[*pos] == "+" || tokens[*pos] == "-") {
        let op = tokens[*pos].clone();
        *pos += 1;
        let right = parse_hex_mul_div(tokens, pos)?;
        left = if op == "+" { left.wrapping_add(right) } else { left.wrapping_sub(right) };
    }
    Ok(left)
}

fn parse_hex_mul_div(tokens: &[String], pos: &mut usize) -> Result<i64, String> {
    let mut left = parse_hex_unary(tokens, pos)?;
    while *pos < tokens.len() && (tokens[*pos] == "*" || tokens[*pos] == "/") {
        let op = tokens[*pos].clone();
        *pos += 1;
        let right = parse_hex_unary(tokens, pos)?;
        if op == "/" && right == 0 {
            return Err("Division by zero".to_string());
        }
        left = if op == "*" { left.wrapping_mul(right) } else { left / right };
    }
    Ok(left)
}

fn parse_hex_unary(tokens: &[String], pos: &mut usize) -> Result<i64, String> {
    if *pos < tokens.len() && tokens[*pos] == "~" {
        *pos += 1;
        let val = parse_hex_atom(tokens, pos)?;
        Ok(!val)
    } else if *pos < tokens.len() && tokens[*pos] == "-" {
        *pos += 1;
        let val = parse_hex_atom(tokens, pos)?;
        Ok(-val)
    } else {
        parse_hex_atom(tokens, pos)
    }
}

fn parse_hex_atom(tokens: &[String], pos: &mut usize) -> Result<i64, String> {
    if *pos >= tokens.len() {
        return Err("Unexpected end of expression".to_string());
    }

    if tokens[*pos] == "(" {
        *pos += 1;
        let result = parse_hex_or(tokens, pos)?;
        if *pos >= tokens.len() || tokens[*pos] != ")" {
            return Err("Missing closing parenthesis".to_string());
        }
        *pos += 1;
        return Ok(result);
    }

    let val = i64::from_str_radix(&tokens[*pos], 16)
        .map_err(|_| format!("Invalid hex number: {}", tokens[*pos]))?;
    *pos += 1;
    Ok(val)
}

// ============================================================================
// JSON Helpers (minimal, no dependencies)
// ============================================================================

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn extract_str(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\":", key);
    let start = json.find(&pattern)? + pattern.len();
    let rest = json[start..].trim_start();
    if !rest.starts_with('"') {
        return None;
    }
    let rest = &rest[1..];
    let mut result = String::new();
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => break,
            '\\' => {
                if let Some(&next) = chars.peek() {
                    chars.next();
                    match next {
                        'n' => result.push('\n'),
                        't' => result.push('\t'),
                        '"' => result.push('"'),
                        '\\' => result.push('\\'),
                        _ => { result.push('\\'); result.push(next); }
                    }
                }
            }
            _ => result.push(c),
        }
    }
    Some(result)
}

fn extract_int(json: &str, key: &str) -> Option<i64> {
    let pattern = format!("\"{}\":", key);
    let start = json.find(&pattern)? + pattern.len();
    let rest = json[start..].trim_start();
    let end = rest.find(|c: char| !c.is_ascii_digit() && c != '-').unwrap_or(rest.len());
    rest[..end].parse().ok()
}

fn extract_str_array(json: &str, key: &str) -> Vec<String> {
    let pattern = format!("\"{}\":", key);
    let start = match json.find(&pattern) {
        Some(s) => s + pattern.len(),
        None => return Vec::new(),
    };
    let rest = json[start..].trim_start();
    if !rest.starts_with('[') {
        return Vec::new();
    }

    let mut depth = 0;
    let mut end = 0;
    for (i, c) in rest.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }

    if end == 0 {
        return Vec::new();
    }

    let inner = &rest[1..end];
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut escape = false;

    for c in inner.chars() {
        if escape {
            current.push(c);
            escape = false;
            continue;
        }
        match c {
            '\\' if in_string => escape = true,
            '"' => {
                if in_string {
                    result.push(current.clone());
                    current.clear();
                }
                in_string = !in_string;
            }
            _ if in_string => current.push(c),
            _ => {}
        }
    }
    result
}
