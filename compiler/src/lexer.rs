use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Literals
    Int(i64),
    Float(f64),
    String(StringLiteral),
    Char(char),
    Bool(bool),
    Nil,

    // Identifier
    Ident(String),

    // Keywords
    Let,
    Mut,
    Const,
    Fn,
    Return,
    If,
    Else,
    Match,
    For,
    While,
    Loop,
    Break,
    Continue,
    Struct,
    Enum,
    Trait,
    Impl,
    Use,
    Module,
    Pub,
    SelfValue,  // self
    SelfType,   // Self
    Type,
    Async,
    Await,
    Spawn,
    Try,
    Catch,
    And,
    Or,
    Not,
    In,
    As,

    // Symbols
    Plus,         // +
    Minus,        // -
    Star,         // *
    Slash,        // /
    Percent,      // %
    StarStar,     // **
    Ampersand,    // &
    Pipe,         // |
    Caret,        // ^
    Tilde,        // ~
    ShiftLeft,    // <<
    ShiftRight,   // >>

    // Comparison
    Eq,           // ==
    NotEq,        // !=
    Less,         // <
    Greater,      // >
    LessEq,      // <=
    GreaterEq,   // >=

    // Assignment
    Assign,       // =
    PlusAssign,   // +=
    MinusAssign,  // -=
    StarAssign,   // *=
    SlashAssign,  // /=
    PercentAssign, // %=

    // Delimiters
    LeftParen,    // (
    RightParen,   // )
    LeftBrace,    // {
    RightBrace,   // }
    LeftBracket,  // [
    RightBracket, // ]

    // Punctuation
    Comma,        // ,
    Dot,          // .
    DotDot,       // ..
    DotDotEq,     // ..=
    Colon,        // :
    ColonAssign,  // :=
    ColonColon,   // ::
    Arrow,        // ->
    FatArrow,     // =>
    PipeArrow,    // |>
    Question,     // ?
    At,           // @
    Hash,         // #
    Ellipsis,     // ...
    NilCoalesce,  // ??

    // Special
    Newline,
    Indent,       // Increase in indentation level
    Dedent,       // Decrease in indentation level
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StringLiteral {
    pub parts: Vec<StringPart>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StringPart {
    Literal(String),
    Interpolation(Vec<Token>),
}

#[derive(Debug, Clone)]
pub struct SpannedToken {
    pub token: Token,
    pub line: usize,
    pub col: usize,
}

pub fn tokenize(source: &str) -> Result<Vec<SpannedToken>, LexError> {
    let mut lexer = Lexer::new(source);
    lexer.tokenize()
}

#[derive(Debug)]
pub struct LexError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

struct Lexer {
    chars: Vec<char>,
    source: String,
    pos: usize,
    line: usize,
    col: usize,
}

impl Lexer {
    fn new(source: &str) -> Self {
        Lexer {
            chars: source.chars().collect(),
            source: source.to_string(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn tokenize(&mut self) -> Result<Vec<SpannedToken>, LexError> {
        let mut tokens = Vec::new();

        while !self.is_at_end() {
            self.skip_whitespace_except_newline();

            if self.is_at_end() {
                break;
            }

            let c = self.peek();

            // Skip comments
            if c == '/' && self.peek_next() == Some('/') {
                self.skip_line_comment();
                continue;
            }
            if c == '/' && self.peek_next() == Some('*') {
                self.skip_block_comment()?;
                continue;
            }

            let line = self.line;
            let col = self.col;

            let token = match c {
                '\n' => {
                    self.advance();
                    // Collapse consecutive newlines
                    if tokens.last().map_or(true, |t: &SpannedToken| t.token == Token::Newline) {
                        continue;
                    }
                    Token::Newline
                }

                '0'..='9' => self.read_number()?,
                '"' => self.read_string()?,
                '\'' => self.read_char()?,
                'a'..='z' | 'A'..='Z' | '_' => {
                    let tok = self.read_ident_or_keyword();
                    // elif → Else + If (two tokens)
                    if let Token::Ident(ref name) = tok {
                        if name == "elif" {
                            tokens.push(SpannedToken { token: Token::Else, line, col });
                            Token::If
                        } else {
                            tok
                        }
                    } else {
                        tok
                    }
                }

                '+' => {
                    self.advance();
                    if self.peek() == '=' {
                        self.advance();
                        Token::PlusAssign
                    } else {
                        Token::Plus
                    }
                }
                '-' => {
                    self.advance();
                    if self.peek() == '>' {
                        self.advance();
                        Token::Arrow
                    } else if self.peek() == '=' {
                        self.advance();
                        Token::MinusAssign
                    } else {
                        Token::Minus
                    }
                }
                '*' => {
                    self.advance();
                    if self.peek() == '*' {
                        self.advance();
                        Token::StarStar
                    } else if self.peek() == '=' {
                        self.advance();
                        Token::StarAssign
                    } else {
                        Token::Star
                    }
                }
                '/' => {
                    self.advance();
                    if self.peek() == '=' {
                        self.advance();
                        Token::SlashAssign
                    } else {
                        Token::Slash
                    }
                }
                '%' => {
                    self.advance();
                    if self.peek() == '=' {
                        self.advance();
                        Token::PercentAssign
                    } else {
                        Token::Percent
                    }
                }

                '=' => {
                    self.advance();
                    if self.peek() == '=' {
                        self.advance();
                        Token::Eq
                    } else if self.peek() == '>' {
                        self.advance();
                        Token::FatArrow
                    } else {
                        Token::Assign
                    }
                }
                '!' => {
                    self.advance();
                    if self.peek() == '=' {
                        self.advance();
                        Token::NotEq
                    } else {
                        return Err(self.error("Unexpected '!'. Did you mean 'not'?"));
                    }
                }
                '<' => {
                    self.advance();
                    if self.peek() == '=' {
                        self.advance();
                        Token::LessEq
                    } else if self.peek() == '<' {
                        self.advance();
                        Token::ShiftLeft
                    } else {
                        Token::Less
                    }
                }
                '>' => {
                    self.advance();
                    if self.peek() == '=' {
                        self.advance();
                        Token::GreaterEq
                    } else if self.peek() == '>' {
                        self.advance();
                        Token::ShiftRight
                    } else {
                        Token::Greater
                    }
                }

                '&' => { self.advance(); Token::Ampersand }
                '|' => {
                    self.advance();
                    if self.peek() == '>' {
                        self.advance();
                        Token::PipeArrow
                    } else {
                        Token::Pipe
                    }
                }
                '^' => { self.advance(); Token::Caret }
                '~' => { self.advance(); Token::Tilde }

                '(' => { self.advance(); Token::LeftParen }
                ')' => { self.advance(); Token::RightParen }
                '{' => { self.advance(); Token::LeftBrace }
                '}' => { self.advance(); Token::RightBrace }
                '[' => { self.advance(); Token::LeftBracket }
                ']' => { self.advance(); Token::RightBracket }

                ',' => { self.advance(); Token::Comma }
                '.' => {
                    self.advance();
                    if self.peek() == '.' {
                        self.advance();
                        if self.peek() == '=' {
                            self.advance();
                            Token::DotDotEq
                        } else if self.peek() == '.' {
                            self.advance();
                            Token::Ellipsis
                        } else {
                            Token::DotDot
                        }
                    } else {
                        Token::Dot
                    }
                }
                ':' => {
                    self.advance();
                    if self.peek() == ':' {
                        self.advance();
                        Token::ColonColon
                    } else if self.peek() == '=' {
                        self.advance();
                        Token::ColonAssign
                    } else {
                        Token::Colon
                    }
                }
                '?' => {
                    self.advance();
                    if self.peek() == '?' {
                        self.advance();
                        Token::NilCoalesce
                    } else {
                        Token::Question
                    }
                }
                '@' => { self.advance(); Token::At }
                '#' => { self.advance(); Token::Hash }

                // Semicolons act as statement separators (same as newlines)
                ';' => {
                    self.advance();
                    if tokens.last().map_or(true, |t: &SpannedToken| t.token == Token::Newline) {
                        continue;
                    }
                    Token::Newline
                }

                other => {
                    return Err(LexError {
                        message: format!("Unexpected character: '{other}'"),
                        line,
                        col,
                    });
                }
            };

            tokens.push(SpannedToken { token, line, col });
        }

        // Remove trailing newline
        if tokens.last().map_or(false, |t| t.token == Token::Newline) {
            tokens.pop();
        }

        tokens.push(SpannedToken {
            token: Token::Eof,
            line: self.line,
            col: self.col,
        });

        // Post-pass: desugar colon-indent blocks into brace blocks.
        // "def foo():" + indent → "def foo() {" ... "}"
        // This lets the parser handle both brace and indent forms uniformly.
        let tokens = desugar_indent_blocks(tokens, &self.source);

        Ok(tokens)
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.chars.len()
    }

    fn peek(&self) -> char {
        if self.is_at_end() {
            '\0'
        } else {
            self.chars[self.pos]
        }
    }

    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> char {
        let c = self.chars[self.pos];
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        c
    }

    fn skip_whitespace_except_newline(&mut self) {
        while !self.is_at_end() {
            let c = self.peek();
            if c == ' ' || c == '\t' || c == '\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_line_comment(&mut self) {
        while !self.is_at_end() && self.peek() != '\n' {
            self.advance();
        }
    }

    fn skip_block_comment(&mut self) -> Result<(), LexError> {
        let line = self.line;
        let col = self.col;
        self.advance(); // /
        self.advance(); // *
        let mut depth = 1;
        while !self.is_at_end() && depth > 0 {
            if self.peek() == '/' && self.peek_next() == Some('*') {
                self.advance();
                self.advance();
                depth += 1;
            } else if self.peek() == '*' && self.peek_next() == Some('/') {
                self.advance();
                self.advance();
                depth -= 1;
            } else {
                self.advance();
            }
        }
        if depth > 0 {
            return Err(LexError {
                message: "Unterminated block comment".to_string(),
                line,
                col,
            });
        }
        Ok(())
    }

    fn read_number(&mut self) -> Result<Token, LexError> {
        let mut num_str = String::new();
        let mut is_float = false;

        while !self.is_at_end() {
            let c = self.peek();
            if c.is_ascii_digit() {
                num_str.push(self.advance());
            } else if c == '_' && !num_str.is_empty() {
                self.advance(); // skip underscore separators
            } else if c == '.' && !is_float {
                // Check if this is a decimal point or a range operator
                if self.peek_next().map_or(false, |n| n.is_ascii_digit()) {
                    is_float = true;
                    num_str.push(self.advance());
                } else {
                    break;
                }
            } else if (c == 'e' || c == 'E') && !num_str.is_empty() {
                is_float = true;
                num_str.push(self.advance());
                if self.peek() == '+' || self.peek() == '-' {
                    num_str.push(self.advance());
                }
            } else {
                break;
            }
        }

        if is_float {
            num_str.parse::<f64>().map(Token::Float).map_err(|_| self.error("Invalid float literal"))
        } else {
            num_str.parse::<i64>().map(Token::Int).map_err(|_| self.error("Invalid integer literal"))
        }
    }

    fn read_string(&mut self) -> Result<Token, LexError> {
        let line = self.line;
        let col = self.col;
        self.advance(); // opening "

        // Check for triple-quoted string
        if self.peek() == '"' && self.peek_next() == Some('"') {
            self.advance();
            self.advance();
            return self.read_triple_string(line, col);
        }

        let mut parts = Vec::new();
        let mut current = String::new();

        while !self.is_at_end() && self.peek() != '"' {
            if self.peek() == '\n' {
                return Err(LexError {
                    message: "Unterminated string (use triple-quotes for multi-line)".to_string(),
                    line,
                    col,
                });
            }
            if self.peek() == '\\' {
                self.advance();
                let escaped = match self.advance() {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    '\\' => '\\',
                    '"' => '"',
                    '{' => '{',
                    '}' => '}',
                    other => {
                        return Err(self.error(&format!("Invalid escape sequence: \\{other}")));
                    }
                };
                current.push(escaped);
            } else if self.peek() == '{' {
                self.advance();
                if !current.is_empty() {
                    parts.push(StringPart::Literal(current.clone()));
                    current.clear();
                }
                let interp_tokens = self.read_interpolation()?;
                parts.push(StringPart::Interpolation(interp_tokens));
            } else {
                current.push(self.advance());
            }
        }

        if self.is_at_end() {
            return Err(LexError {
                message: "Unterminated string".to_string(),
                line,
                col,
            });
        }
        self.advance(); // closing "

        if !current.is_empty() {
            parts.push(StringPart::Literal(current));
        }

        Ok(Token::String(StringLiteral { parts }))
    }

    fn read_triple_string(&mut self, _start_line: usize, _start_col: usize) -> Result<Token, LexError> {
        let mut parts = Vec::new();
        let mut current = String::new();

        loop {
            if self.is_at_end() {
                return Err(self.error("Unterminated triple-quoted string"));
            }
            if self.peek() == '"' && self.peek_next() == Some('"') {
                let saved_pos = self.pos;
                let saved_line = self.line;
                let saved_col = self.col;
                self.advance();
                self.advance();
                if self.peek() == '"' {
                    self.advance();
                    break;
                } else {
                    // Not a closing triple quote, add the two quotes we consumed
                    current.push('"');
                    current.push('"');
                    // Restore isn't needed since we already advanced
                    let _ = (saved_pos, saved_line, saved_col);
                }
            } else if self.peek() == '{' {
                self.advance();
                if !current.is_empty() {
                    parts.push(StringPart::Literal(current.clone()));
                    current.clear();
                }
                let interp_tokens = self.read_interpolation()?;
                parts.push(StringPart::Interpolation(interp_tokens));
            } else if self.peek() == '\\' {
                self.advance();
                let escaped = match self.advance() {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    '\\' => '\\',
                    '"' => '"',
                    '{' => '{',
                    '}' => '}',
                    other => {
                        return Err(self.error(&format!("Invalid escape sequence: \\{other}")));
                    }
                };
                current.push(escaped);
            } else {
                current.push(self.advance());
            }
        }

        if !current.is_empty() {
            parts.push(StringPart::Literal(current));
        }

        Ok(Token::String(StringLiteral { parts }))
    }

    fn read_interpolation(&mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();
        let mut depth = 1;

        while !self.is_at_end() && depth > 0 {
            self.skip_whitespace_except_newline();
            if self.is_at_end() {
                break;
            }

            let c = self.peek();
            if c == '{' {
                depth += 1;
                self.advance();
                tokens.push(Token::LeftBrace);
            } else if c == '}' {
                depth -= 1;
                if depth == 0 {
                    self.advance();
                    break;
                }
                self.advance();
                tokens.push(Token::RightBrace);
            } else if c.is_ascii_digit() {
                tokens.push(self.read_number()?);
            } else if c.is_ascii_alphabetic() || c == '_' {
                tokens.push(self.read_ident_or_keyword());
            } else if c == '.' {
                self.advance();
                tokens.push(Token::Dot);
            } else if c == '(' {
                self.advance();
                tokens.push(Token::LeftParen);
            } else if c == ')' {
                self.advance();
                tokens.push(Token::RightParen);
            } else if c == '[' {
                self.advance();
                tokens.push(Token::LeftBracket);
            } else if c == ']' {
                self.advance();
                tokens.push(Token::RightBracket);
            } else if c == ',' {
                self.advance();
                tokens.push(Token::Comma);
            } else if c == '+' {
                self.advance();
                tokens.push(Token::Plus);
            } else if c == '-' {
                self.advance();
                tokens.push(Token::Minus);
            } else if c == '*' {
                self.advance();
                tokens.push(Token::Star);
            } else if c == '/' {
                self.advance();
                tokens.push(Token::Slash);
            } else if c == '"' {
                // Nested string inside interpolation
                tokens.push(self.read_string()?);
            } else if c == '%' {
                self.advance();
                tokens.push(Token::Percent);
            } else if c == '=' {
                self.advance();
                if self.peek() == '=' {
                    self.advance();
                    tokens.push(Token::Eq);
                } else {
                    tokens.push(Token::Assign);
                }
            } else if c == '<' {
                self.advance();
                tokens.push(Token::Less);
            } else if c == '>' {
                self.advance();
                tokens.push(Token::Greater);
            } else if c == '!' {
                self.advance();
                if self.peek() == '=' {
                    self.advance();
                    tokens.push(Token::NotEq);
                }
            } else if c == '?' {
                self.advance();
                if self.peek() == '?' {
                    self.advance();
                    tokens.push(Token::NilCoalesce);
                } else {
                    tokens.push(Token::Question);
                }
            } else if c == ':' {
                self.advance();
                tokens.push(Token::Colon);
            } else if c == '|' {
                self.advance();
                tokens.push(Token::Pipe);
            } else if c == '&' {
                self.advance();
                tokens.push(Token::Ampersand);
            } else if c == '\'' {
                tokens.push(self.read_char()?);
            } else if c == ';' || c == '\n' || c == '\r' || c == '\t' || c == ' ' {
                // Skip whitespace and separators inside interpolation
                self.advance();
            } else {
                // Skip unknown chars rather than error — interpolation is best-effort
                self.advance();
            }
        }

        Ok(tokens)
    }

    fn read_char(&mut self) -> Result<Token, LexError> {
        self.advance(); // opening '
        let c = if self.peek() == '\\' {
            self.advance();
            match self.advance() {
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                '\\' => '\\',
                '\'' => '\'',
                other => {
                    return Err(self.error(&format!("Invalid escape sequence in char: \\{other}")));
                }
            }
        } else {
            self.advance()
        };

        if self.peek() != '\'' {
            return Err(self.error("Unterminated character literal"));
        }
        self.advance(); // closing '
        Ok(Token::Char(c))
    }

    fn read_ident_or_keyword(&mut self) -> Token {
        let mut name = String::new();
        while !self.is_at_end() && (self.peek().is_ascii_alphanumeric() || self.peek() == '_') {
            name.push(self.advance());
        }

        match name.as_str() {
            // Short (compact) and long (canonical) keyword forms
            "lt" | "let" => Token::Let,
            "mt" | "mut" => Token::Mut,
            "cn" | "const" => Token::Const,
            "fn" | "def" => Token::Fn,
            "rt" | "return" => Token::Return,
            "if" => Token::If,
            "el" | "else" => Token::Else,
            "mc" | "match" => Token::Match,
            "fo" | "for" => Token::For,
            "wl" | "while" => Token::While,
            "lp" | "loop" => Token::Loop,
            "bk" | "break" => Token::Break,
            "ct" | "continue" => Token::Continue,
            "st" | "struct" | "class" => Token::Struct,
            "en" | "enum" => Token::Enum,
            "tr" | "trait" => Token::Trait,
            "im" | "impl" => Token::Impl,
            "use" => Token::Use,
            "md" | "module" => Token::Module,
            "pb" | "pub" => Token::Pub,
            "sf" | "self" => Token::SelfValue,
            "Sf" | "Self" => Token::SelfType,
            "ty" | "type" => Token::Type,
            "ac" | "async" => Token::Async,
            "aw" | "await" => Token::Await,
            "sp" | "spawn" => Token::Spawn,
            "try" => Token::Try,
            "ch" | "catch" | "except" => Token::Catch,
            "and" => Token::And,
            "or" => Token::Or,
            "not" => Token::Not,
            "in" => Token::In,
            "as" => Token::As,
            "tt" | "True" => Token::Bool(true),
            "ff" | "False" => Token::Bool(false),
            "nil" | "None" => Token::Nil,
            _ => Token::Ident(name),
        }
    }

    fn error(&self, message: &str) -> LexError {
        LexError {
            message: message.to_string(),
            line: self.line,
            col: self.col,
        }
    }
}

/// Desugar indent-based blocks into brace blocks.
///
/// When the token stream contains `Colon Newline` followed by an increased
/// indentation level, insert `LeftBrace` and track the indent level. When
/// indentation decreases back, insert `RightBrace` tokens.
///
/// This allows both `def foo(): <indent>body<dedent>` and `def foo() { body }`
/// to produce identical token streams for the parser.
fn desugar_indent_blocks(tokens: Vec<SpannedToken>, source: &str) -> Vec<SpannedToken> {
    // Quick check: does the source contain any colon-at-end-of-line patterns?
    // If not, skip the whole pass (pure brace-mode source).
    let has_colon_blocks = source.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.ends_with(':') && !trimmed.ends_with("::") && !trimmed.starts_with("//")
            && !trimmed.starts_with('#')
    });
    if !has_colon_blocks {
        return tokens;
    }

    // Build a map of line → indentation level (number of leading spaces)
    let line_indents: Vec<usize> = source.lines()
        .map(|line| line.len() - line.trim_start().len())
        .collect();

    let mut result = Vec::with_capacity(tokens.len() + 32);
    let mut indent_stack: Vec<usize> = vec![0]; // stack of indent levels
    let mut i = 0;

    while i < tokens.len() {
        let tok = &tokens[i];

        // Detect Colon + Newline → open indent block
        if tok.token == Token::Colon {
            // Check if this colon is at the end of a statement (followed by Newline or EOF)
            let next_is_newline = i + 1 < tokens.len() && tokens[i + 1].token == Token::Newline;
            let next_is_eof = i + 1 < tokens.len() && tokens[i + 1].token == Token::Eof;
            if next_is_newline || next_is_eof {
                // Check if the next non-empty line has increased indentation
                let colon_line = tok.line;
                let next_indent = if colon_line < line_indents.len() {
                    // Find next non-empty line
                    let mut nl = colon_line; // lines are 1-indexed, vec is 0-indexed
                    while nl < line_indents.len() {
                        let line_text = source.lines().nth(nl).unwrap_or("");
                        if !line_text.trim().is_empty() {
                            break;
                        }
                        nl += 1;
                    }
                    if nl < line_indents.len() { line_indents[nl] } else { 0 }
                } else {
                    0
                };

                let current_indent = *indent_stack.last().unwrap();
                if next_indent > current_indent {
                    // Replace Colon with LeftBrace, skip the Newline
                    result.push(SpannedToken {
                        token: Token::LeftBrace,
                        line: tok.line,
                        col: tok.col,
                    });
                    indent_stack.push(next_indent);
                    i += 1; // skip colon
                    if i < tokens.len() && tokens[i].token == Token::Newline {
                        i += 1; // skip newline after colon
                    }
                    continue;
                }
            }
            // If colon is NOT a block opener (e.g., type annotation), keep it
        }

        // At Newline tokens, check for dedent
        if tok.token == Token::Newline && indent_stack.len() > 1 {
            let tok_line = tok.line; // this newline is at end of tok_line
            // Find the indentation of the NEXT non-empty line
            let mut next_line_idx = tok_line; // 0-indexed: line after tok_line
            while next_line_idx < line_indents.len() {
                let line_text = source.lines().nth(next_line_idx).unwrap_or("");
                if !line_text.trim().is_empty() {
                    break;
                }
                next_line_idx += 1;
            }
            let next_indent = if next_line_idx < line_indents.len() {
                line_indents[next_line_idx]
            } else {
                0 // EOF — close all blocks
            };

            // Emit RightBrace for each level we dedent
            while indent_stack.len() > 1 && next_indent < *indent_stack.last().unwrap() {
                indent_stack.pop();
                result.push(SpannedToken {
                    token: Token::RightBrace,
                    line: tok.line,
                    col: 0,
                });
                result.push(SpannedToken {
                    token: Token::Newline,
                    line: tok.line,
                    col: 0,
                });
            }
        }

        result.push(tok.clone());
        i += 1;
    }

    // Close any remaining open indent blocks before EOF
    while indent_stack.len() > 1 {
        indent_stack.pop();
        // Insert before EOF
        let eof_pos = result.len().saturating_sub(1);
        result.insert(eof_pos, SpannedToken {
            token: Token::RightBrace,
            line: 0,
            col: 0,
        });
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(source: &str) -> Vec<Token> {
        tokenize(source)
            .unwrap()
            .into_iter()
            .map(|s| s.token)
            .filter(|t| *t != Token::Newline && *t != Token::Eof)
            .collect()
    }

    #[test]
    fn test_integers() {
        assert_eq!(tokens("42"), vec![Token::Int(42)]);
        assert_eq!(tokens("1_000_000"), vec![Token::Int(1000000)]);
    }

    #[test]
    fn test_floats() {
        assert_eq!(tokens("3.14"), vec![Token::Float(3.14)]);
        assert_eq!(tokens("1.0e10"), vec![Token::Float(1.0e10)]);
    }

    #[test]
    fn test_keywords() {
        assert_eq!(tokens("lt mt fn if el"), vec![
            Token::Let, Token::Mut, Token::Fn, Token::If, Token::Else
        ]);
    }

    #[test]
    fn test_operators() {
        assert_eq!(tokens("|>"), vec![Token::PipeArrow]);
        assert_eq!(tokens(".."), vec![Token::DotDot]);
        assert_eq!(tokens("..="), vec![Token::DotDotEq]);
        assert_eq!(tokens("=>"), vec![Token::FatArrow]);
        assert_eq!(tokens("->"), vec![Token::Arrow]);
        assert_eq!(tokens("??"), vec![Token::NilCoalesce]);
        assert_eq!(tokens("**"), vec![Token::StarStar]);
    }

    #[test]
    fn test_string_simple() {
        let toks = tokens(r#""hello""#);
        assert_eq!(toks.len(), 1);
        match &toks[0] {
            Token::String(s) => {
                assert_eq!(s.parts, vec![StringPart::Literal("hello".to_string())]);
            }
            _ => panic!("Expected string token"),
        }
    }

    #[test]
    fn test_string_interpolation() {
        let toks = tokens(r#""Hello, {name}!""#);
        assert_eq!(toks.len(), 1);
        match &toks[0] {
            Token::String(s) => {
                assert_eq!(s.parts.len(), 3);
                assert_eq!(s.parts[0], StringPart::Literal("Hello, ".to_string()));
                assert_eq!(
                    s.parts[1],
                    StringPart::Interpolation(vec![Token::Ident("name".to_string())])
                );
                assert_eq!(s.parts[2], StringPart::Literal("!".to_string()));
            }
            _ => panic!("Expected string token"),
        }
    }

    #[test]
    fn test_comments() {
        assert_eq!(tokens("42 // comment"), vec![Token::Int(42)]);
        assert_eq!(tokens("/* block */ 42"), vec![Token::Int(42)]);
    }

    #[test]
    fn test_logical_operators() {
        assert_eq!(tokens("tt and ff or not tt"), vec![
            Token::Bool(true), Token::And, Token::Bool(false),
            Token::Or, Token::Not, Token::Bool(true)
        ]);
    }

    #[test]
    fn test_semicolons_as_separators() {
        // Semicolons should produce Newline tokens (same as actual newlines)
        let toks = tokenize("lt x = 1; lt y = 2").unwrap();
        let newlines: Vec<_> = toks.iter().filter(|t| t.token == Token::Newline).collect();
        assert_eq!(newlines.len(), 1);
    }

    #[test]
    fn test_escape_braces() {
        // \{ and \} should produce literal braces
        let toks = tokens(r#""\{\}""#);
        assert_eq!(toks.len(), 1);
        match &toks[0] {
            Token::String(s) => {
                assert_eq!(s.parts.len(), 1);
                assert_eq!(s.parts[0], StringPart::Literal("{}".to_string()));
            }
            _ => panic!("Expected string"),
        }
    }

    #[test]
    fn test_nested_string_in_interpolation() {
        // String with a nested string inside interpolation: "val={m["k"]}"
        let toks = tokens(r#""x={m["k"]}""#);
        assert_eq!(toks.len(), 1);
        match &toks[0] {
            Token::String(s) => {
                assert!(s.parts.len() >= 2); // literal + interpolation at minimum
            }
            _ => panic!("Expected string"),
        }
    }
}
