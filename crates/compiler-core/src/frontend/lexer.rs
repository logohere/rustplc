use std::fmt;

use super::{SourceSpan, Token, TokenKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexMode {
    Source,
    Test,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub message: String,
    pub span: SourceSpan,
}

impl fmt::Display for LexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{}:{}: {}",
            self.span.file, self.span.start_line, self.span.start_col, self.message
        )
    }
}

impl std::error::Error for LexError {}

pub fn lex(source: &str, file: &str, mode: LexMode) -> Result<Vec<Token>, Vec<LexError>> {
    let mut lexer = Lexer::new(source, file, mode);
    while !lexer.eof() {
        if let Err(error) = lexer.next_token() {
            lexer.errors.push(error);
            lexer.skip_invalid_scalar();
        }
    }
    if lexer.errors.is_empty() {
        Ok(lexer.tokens)
    } else {
        Err(lexer.errors)
    }
}

struct Lexer<'a> {
    source: &'a str,
    file: &'a str,
    mode: LexMode,
    offset: usize,
    line: usize,
    col: usize,
    tokens: Vec<Token>,
    errors: Vec<LexError>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str, file: &'a str, mode: LexMode) -> Self {
        Self {
            source,
            file,
            mode,
            offset: 0,
            line: 1,
            col: 1,
            tokens: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn eof(&self) -> bool {
        self.offset >= self.source.len()
    }

    fn rest(&self) -> &'a str {
        &self.source[self.offset..]
    }

    fn next_token(&mut self) -> Result<(), LexError> {
        if self.skip_whitespace() || self.skip_comment()? {
            return Ok(());
        }

        if self.rest().starts_with('{') && self.external_pragma_ahead() {
            let len = self.scan_external_pragma()?;
            self.emit(TokenKind::ExternalPragma, len);
            return Ok(());
        }

        if let Some((kind, len)) = self.multi_character_operator() {
            self.emit(kind, len);
            return Ok(());
        }

        if let Some((kind, len)) = self.hash_literal() {
            self.emit(kind, len);
            return Ok(());
        }

        let first = self.peek_char().expect("not eof");
        if first == '\'' {
            let len = self.scan_string(false)?;
            self.emit(TokenKind::StringLiteral, len);
            return Ok(());
        }
        if first == '"' {
            let len = self.scan_string(true)?;
            self.emit(TokenKind::WideStringLiteral, len);
            return Ok(());
        }
        if first == '%' {
            if let Some(len) = self.scan_direct_address() {
                self.emit(TokenKind::DirectAddress, len);
                return Ok(());
            }
        }
        if first.is_ascii_digit() {
            if let Some((kind, len)) = self.scan_number() {
                self.emit(kind, len);
                return Ok(());
            }
        }
        if is_identifier_start(first) {
            let len = self.scan_identifier_len();
            let image = &self.rest()[..len];
            let kind = keyword(image, self.mode).unwrap_or(TokenKind::Identifier);
            self.emit(kind, len);
            return Ok(());
        }
        if let Some(kind) = single_character_token(first) {
            self.emit(kind, first.len_utf8());
            return Ok(());
        }

        Err(self.error_here(format!("unexpected character {first:?}")))
    }

    fn skip_whitespace(&mut self) -> bool {
        let mut len = 0;
        for ch in self.rest().chars() {
            if !ch.is_whitespace() {
                break;
            }
            len += ch.len_utf8();
        }
        if len == 0 {
            false
        } else {
            self.consume(len);
            true
        }
    }

    fn skip_comment(&mut self) -> Result<bool, LexError> {
        if self.rest().starts_with("//") {
            let len = self
                .rest()
                .find(['\n', '\r'])
                .unwrap_or_else(|| self.rest().len());
            self.consume(len);
            return Ok(true);
        }
        if !self.rest().starts_with("(*") {
            return Ok(false);
        }
        let start_line = self.line;
        let start_col = self.col;
        let bytes = self.rest().as_bytes();
        let mut index = 2;
        let mut depth = 1usize;
        while index + 1 < bytes.len() {
            match (bytes[index], bytes[index + 1]) {
                (b'(', b'*') => {
                    depth += 1;
                    index += 2;
                }
                (b'*', b')') => {
                    depth -= 1;
                    index += 2;
                    if depth == 0 {
                        self.consume(index);
                        return Ok(true);
                    }
                }
                _ => {
                    index += self.rest()[index..]
                        .chars()
                        .next()
                        .map(char::len_utf8)
                        .unwrap_or(1)
                }
            }
        }
        Err(LexError {
            message: "unterminated block comment".to_owned(),
            span: SourceSpan::new(self.file, start_line, start_line, start_col, start_col + 1),
        })
    }

    fn external_pragma_ahead(&self) -> bool {
        let rest = self.rest();
        let mut index = 1;
        while rest
            .as_bytes()
            .get(index)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            index += 1;
        }
        rest.get(index..index + 8)
            .is_some_and(|word| word.eq_ignore_ascii_case("external"))
    }

    fn scan_external_pragma(&self) -> Result<usize, LexError> {
        let rest = self.rest();
        let bytes = rest.as_bytes();
        let mut index = 1;
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            index += 1;
        }
        index += 8;
        let mut depth = 1usize;
        while index < bytes.len() {
            let byte = bytes[index];
            if byte == b'\'' || byte == b'"' {
                let quote = byte;
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == b'\\' {
                        index = (index + 2).min(bytes.len());
                        continue;
                    }
                    if bytes[index] == quote {
                        index += 1;
                        break;
                    }
                    index += 1;
                }
                continue;
            }
            if byte == b'/' && bytes.get(index + 1) == Some(&b'/') {
                index += 2;
                while index < bytes.len() && !matches!(bytes[index], b'\n' | b'\r') {
                    index += 1;
                }
                continue;
            }
            if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
                index += 2;
                while index + 1 < bytes.len() {
                    if bytes[index] == b'*' && bytes[index + 1] == b'/' {
                        index += 2;
                        break;
                    }
                    index += 1;
                }
                continue;
            }
            match byte {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(index + 1);
                    }
                }
                _ => {}
            }
            index += 1;
        }
        Err(self.error_here("unterminated {external ...} pragma"))
    }

    fn multi_character_operator(&self) -> Option<(TokenKind, usize)> {
        let rest = self.rest();
        const OPS: [(&str, TokenKind); 8] = [
            ("..", TokenKind::DoubleDot),
            ("**", TokenKind::Power),
            (":=", TokenKind::Assign),
            ("=>", TokenKind::OutputAssign),
            ("<>", TokenKind::NotEqual),
            ("<=", TokenKind::LessEqual),
            (">=", TokenKind::GreaterEqual),
            ("REF=", TokenKind::RefAssign),
        ];
        OPS.into_iter().find_map(|(text, kind)| {
            rest.get(..text.len())
                .filter(|candidate| candidate.eq_ignore_ascii_case(text))
                .map(|_| (kind, text.len()))
        })
    }

    fn hash_literal(&self) -> Option<(TokenKind, usize)> {
        let rest = self.rest();
        let prefix_len = rest
            .chars()
            .take_while(|ch| is_identifier_continue(*ch))
            .map(char::len_utf8)
            .sum::<usize>();
        if prefix_len == 0 || rest.as_bytes().get(prefix_len) != Some(&b'#') {
            return None;
        }
        let prefix = rest[..prefix_len].to_ascii_uppercase();
        let body_start = prefix_len + 1;
        let (kind, len) = match prefix.as_str() {
            "T" | "TIME" => (TokenKind::TimeLiteral, scan_time_body(rest, body_start)?),
            "DT" | "DATE_AND_TIME" => (
                TokenKind::DateTimeLiteral,
                scan_date_time_body(rest, body_start)?,
            ),
            "D" | "DATE" => (TokenKind::DateLiteral, scan_date_body(rest, body_start)?),
            "TOD" | "TIME_OF_DAY" => (
                TokenKind::TimeOfDayLiteral,
                scan_time_of_day_body(rest, body_start)?,
            ),
            "BYTE" | "WORD" | "DWORD" | "LWORD" | "SINT" | "INT" | "DINT" | "LINT" | "USINT"
            | "UINT" | "UDINT" | "ULINT" | "BOOL" | "REAL" | "LREAL" => (
                TokenKind::TypedLiteral,
                scan_typed_number_body(rest, body_start)?,
            ),
            _ => return None,
        };
        Some((kind, len))
    }

    fn scan_string(&self, wide: bool) -> Result<usize, LexError> {
        let rest = self.rest();
        let bytes = rest.as_bytes();
        let quote = if wide { b'"' } else { b'\'' };
        let mut index = 1;
        while index < bytes.len() {
            if bytes[index] == quote {
                if !wide && bytes.get(index + 1) == Some(&quote) {
                    index += 2;
                    continue;
                }
                return Ok(index + 1);
            }
            if bytes[index] == b'$' {
                let Some(next) = bytes.get(index + 1).copied() else {
                    break;
                };
                if matches!(
                    next,
                    b'$' | b'L' | b'N' | b'P' | b'R' | b'T' | b'l' | b'n' | b'p' | b'r' | b't'
                ) || next == quote
                {
                    index += 2;
                    continue;
                }
                let hex_digits = if wide { 4 } else { 2 };
                if index + 1 + hex_digits < bytes.len()
                    && bytes[index + 1..index + 1 + hex_digits]
                        .iter()
                        .all(u8::is_ascii_hexdigit)
                {
                    index += 1 + hex_digits;
                    continue;
                }
            }
            index += rest[index..]
                .chars()
                .next()
                .map(char::len_utf8)
                .unwrap_or(1);
        }
        Err(self.error_here(if wide {
            "unterminated wide string literal"
        } else {
            "unterminated string literal"
        }))
    }

    fn scan_direct_address(&self) -> Option<usize> {
        let bytes = self.rest().as_bytes();
        if bytes.first() != Some(&b'%') {
            return None;
        }
        let mut index = 1;
        let area = bytes.get(index)?.to_ascii_uppercase();
        if !matches!(area, b'I' | b'Q' | b'M') {
            return None;
        }
        index += 1;
        if bytes.get(index).is_some_and(|byte| {
            matches!(byte.to_ascii_uppercase(), b'X' | b'B' | b'W' | b'D' | b'L')
        }) {
            index += 1;
        }
        let digit_start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == digit_start {
            return None;
        }
        loop {
            if bytes.get(index) != Some(&b'.') {
                break;
            }
            let dot = index;
            index += 1;
            let start = index;
            while bytes.get(index).is_some_and(u8::is_ascii_digit) {
                index += 1;
            }
            if index == start {
                index = dot;
                break;
            }
        }
        Some(index)
    }

    fn scan_number(&self) -> Option<(TokenKind, usize)> {
        let rest = self.rest();
        for base in ["16#", "8#", "2#"] {
            if rest.starts_with(base) {
                let mut index = base.len();
                let valid = |byte: u8| match base {
                    "16#" => byte.is_ascii_hexdigit() || byte == b'_',
                    "8#" => matches!(byte, b'0'..=b'7' | b'_'),
                    _ => matches!(byte, b'0' | b'1' | b'_'),
                };
                let start = index;
                while rest.as_bytes().get(index).copied().is_some_and(valid) {
                    index += 1;
                }
                return (index > start).then_some((TokenKind::IntegerLiteral, index));
            }
        }

        let bytes = rest.as_bytes();
        let mut index = 0;
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_digit() || *byte == b'_')
        {
            index += 1;
        }
        if index == 0 {
            return None;
        }
        let mut real = false;
        if bytes.get(index) == Some(&b'.') && bytes.get(index + 1).is_some_and(u8::is_ascii_digit) {
            real = true;
            index += 1;
            while bytes.get(index).is_some_and(u8::is_ascii_digit) {
                index += 1;
            }
        }
        if bytes
            .get(index)
            .is_some_and(|byte| matches!(byte, b'e' | b'E'))
        {
            let exponent_marker = index;
            let mut exponent = index + 1;
            if bytes
                .get(exponent)
                .is_some_and(|byte| matches!(byte, b'+' | b'-'))
            {
                exponent += 1;
            }
            let start = exponent;
            while bytes.get(exponent).is_some_and(u8::is_ascii_digit) {
                exponent += 1;
            }
            if exponent > start {
                real = true;
                index = exponent;
            } else {
                index = exponent_marker;
            }
        }
        Some((
            if real {
                TokenKind::RealLiteral
            } else {
                TokenKind::IntegerLiteral
            },
            index,
        ))
    }

    fn scan_identifier_len(&self) -> usize {
        self.rest()
            .chars()
            .take_while(|ch| is_identifier_continue(*ch))
            .map(char::len_utf8)
            .sum()
    }

    fn emit(&mut self, kind: TokenKind, len: usize) {
        let start_line = self.line;
        let start_col = self.col;
        let image = self.rest()[..len].to_owned();
        let span = span_for_image(self.file, start_line, start_col, &image);
        self.consume(len);
        self.tokens.push(Token::new(kind, image, span));
    }

    fn consume(&mut self, len: usize) {
        let text = &self.source[self.offset..self.offset + len];
        for ch in text.chars() {
            if ch == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
        self.offset += len;
    }

    fn skip_invalid_scalar(&mut self) {
        if let Some(ch) = self.peek_char() {
            self.consume(ch.len_utf8());
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn error_here(&self, message: impl Into<String>) -> LexError {
        LexError {
            message: message.into(),
            span: SourceSpan::new(self.file, self.line, self.line, self.col, self.col),
        }
    }
}

fn span_for_image(file: &str, start_line: usize, start_col: usize, image: &str) -> SourceSpan {
    let mut line = start_line;
    let mut col = start_col;
    let mut end_line = start_line;
    let mut end_col = start_col;
    for ch in image.chars() {
        end_line = line;
        end_col = col;
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    SourceSpan::new(file, start_line, end_line, start_col, end_col)
}

fn scan_time_body(rest: &str, mut index: usize) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut components = 0;
    loop {
        let start = index;
        while bytes
            .get(index)
            .is_some_and(|byte| byte.is_ascii_digit() || *byte == b'_')
        {
            index += 1;
        }
        if index == start {
            break;
        }
        if bytes.get(index) == Some(&b'.') {
            let decimal = index;
            index += 1;
            let fraction = index;
            while bytes
                .get(index)
                .is_some_and(|byte| byte.is_ascii_digit() || *byte == b'_')
            {
                index += 1;
            }
            if index == fraction {
                index = decimal;
            }
        }
        let unit_len = ["ms", "us", "ns", "d", "h", "m", "s"]
            .into_iter()
            .find_map(|unit| {
                rest.get(index..index + unit.len())
                    .filter(|candidate| candidate.eq_ignore_ascii_case(unit))
                    .map(|_| unit.len())
            })?;
        index += unit_len;
        components += 1;
    }
    (components > 0).then_some(index)
}

fn scan_date_body(rest: &str, index: usize) -> Option<usize> {
    let mut index = scan_exact_digits(rest, index, 4)?;
    index = scan_byte(rest, index, b'-')?;
    index = scan_range_digits(rest, index, 1, 2)?;
    index = scan_byte(rest, index, b'-')?;
    scan_range_digits(rest, index, 1, 2)
}

fn scan_time_of_day_body(rest: &str, index: usize) -> Option<usize> {
    let mut index = scan_range_digits(rest, index, 1, 2)?;
    index = scan_byte(rest, index, b':')?;
    index = scan_range_digits(rest, index, 1, 2)?;
    if rest.as_bytes().get(index) == Some(&b':') {
        index += 1;
        index = scan_range_digits(rest, index, 1, 2)?;
        if rest.as_bytes().get(index) == Some(&b'.') {
            index += 1;
            index = scan_range_digits(rest, index, 1, usize::MAX)?;
        }
    }
    Some(index)
}

fn scan_date_time_body(rest: &str, index: usize) -> Option<usize> {
    let mut index = scan_date_body(rest, index)?;
    index = scan_byte(rest, index, b'-')?;
    scan_time_of_day_body(rest, index)
}

fn scan_typed_number_body(rest: &str, mut index: usize) -> Option<usize> {
    for base in ["16#", "8#", "2#"] {
        if rest.get(index..)?.starts_with(base) {
            index += base.len();
            let valid = |byte: u8| match base {
                "16#" => byte.is_ascii_hexdigit() || byte == b'_',
                "8#" => matches!(byte, b'0'..=b'7' | b'_'),
                _ => matches!(byte, b'0' | b'1' | b'_'),
            };
            let start = index;
            while rest.as_bytes().get(index).copied().is_some_and(valid) {
                index += 1;
            }
            return (index > start).then_some(index);
        }
    }
    let bytes = rest.as_bytes();
    let start = index;
    while bytes
        .get(index)
        .is_some_and(|byte| byte.is_ascii_digit() || *byte == b'_')
    {
        index += 1;
    }
    if index == start {
        return None;
    }
    if bytes.get(index) == Some(&b'.') {
        let dot = index;
        index += 1;
        let fraction = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == fraction {
            index = dot;
        }
    }
    if bytes
        .get(index)
        .is_some_and(|byte| matches!(byte, b'e' | b'E'))
    {
        let exponent_marker = index;
        let mut exponent = index + 1;
        if bytes
            .get(exponent)
            .is_some_and(|byte| matches!(byte, b'+' | b'-'))
        {
            exponent += 1;
        }
        let digits = exponent;
        while bytes.get(exponent).is_some_and(u8::is_ascii_digit) {
            exponent += 1;
        }
        if exponent > digits {
            index = exponent;
        } else {
            index = exponent_marker;
        }
    }
    Some(index)
}

fn scan_exact_digits(rest: &str, mut index: usize, count: usize) -> Option<usize> {
    for _ in 0..count {
        if !rest.as_bytes().get(index).is_some_and(u8::is_ascii_digit) {
            return None;
        }
        index += 1;
    }
    Some(index)
}

fn scan_range_digits(rest: &str, mut index: usize, min: usize, max: usize) -> Option<usize> {
    let start = index;
    while index - start < max && rest.as_bytes().get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    (index - start >= min).then_some(index)
}

fn scan_byte(rest: &str, index: usize, byte: u8) -> Option<usize> {
    (rest.as_bytes().get(index) == Some(&byte)).then_some(index + 1)
}

fn is_identifier_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_identifier_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn single_character_token(ch: char) -> Option<TokenKind> {
    Some(match ch {
        ':' => TokenKind::Colon,
        ';' => TokenKind::Semicolon,
        ',' => TokenKind::Comma,
        '.' => TokenKind::Dot,
        '(' => TokenKind::LParen,
        ')' => TokenKind::RParen,
        '[' => TokenKind::LBracket,
        ']' => TokenKind::RBracket,
        '=' => TokenKind::Equal,
        '<' => TokenKind::Less,
        '>' => TokenKind::Greater,
        '+' => TokenKind::Plus,
        '-' => TokenKind::Minus,
        '*' => TokenKind::Star,
        '/' => TokenKind::Slash,
        '^' => TokenKind::Caret,
        '&' => TokenKind::Ampersand,
        _ => return None,
    })
}

fn keyword(image: &str, mode: LexMode) -> Option<TokenKind> {
    let upper = image.to_ascii_uppercase();
    if mode == LexMode::Test {
        let test = match upper.as_str() {
            "TEST" => TokenKind::Test,
            "END_TEST" => TokenKind::EndTest,
            "ASSERT_EQ" => TokenKind::AssertEq,
            "ASSERT_TRUE" => TokenKind::AssertTrue,
            "ASSERT_FALSE" => TokenKind::AssertFalse,
            "ASSERT_NEQ" => TokenKind::AssertNeq,
            "ASSERT_GT" => TokenKind::AssertGt,
            "ASSERT_LT" => TokenKind::AssertLt,
            "ASSERT_GE" => TokenKind::AssertGe,
            "ASSERT_LE" => TokenKind::AssertLe,
            "ASSERT_NEAR" => TokenKind::AssertNear,
            "ADVANCE_TIME" => TokenKind::AdvanceTime,
            "SETUP" => TokenKind::Setup,
            "END_SETUP" => TokenKind::EndSetup,
            "TEARDOWN" => TokenKind::Teardown,
            "END_TEARDOWN" => TokenKind::EndTeardown,
            "MOCK_VERIFY_CALL_COUNT" => TokenKind::MockVerifyCallCount,
            "MOCK_VERIFY_CALLED" => TokenKind::MockVerifyCalled,
            "MOCK_FUNCTION" => TokenKind::MockFunction,
            "MOCK" => TokenKind::Mock,
            "RETURNS" => TokenKind::Returns,
            _ => TokenKind::Identifier,
        };
        if test != TokenKind::Identifier {
            return Some(test);
        }
    }
    Some(match upper.as_str() {
        "PROGRAM" => TokenKind::Program,
        "END_PROGRAM" => TokenKind::EndProgram,
        "FUNCTION" => TokenKind::Function,
        "END_FUNCTION" => TokenKind::EndFunction,
        "FUNCTION_BLOCK" => TokenKind::FunctionBlock,
        "END_FUNCTION_BLOCK" => TokenKind::EndFunctionBlock,
        "VAR" => TokenKind::Var,
        "END_VAR" => TokenKind::EndVar,
        "VAR_INPUT" => TokenKind::VarInput,
        "VAR_OUTPUT" => TokenKind::VarOutput,
        "VAR_IN_OUT" => TokenKind::VarInOut,
        "VAR_EXTERNAL" => TokenKind::VarExternal,
        "VAR_GLOBAL" => TokenKind::VarGlobal,
        "VAR_TEMP" => TokenKind::VarTemp,
        "VAR_INST" => TokenKind::VarInst,
        "CONSTANT" => TokenKind::Constant,
        "RETAIN" => TokenKind::Retain,
        "AT" => TokenKind::At,
        "TYPE" => TokenKind::Type,
        "END_TYPE" => TokenKind::EndType,
        "STRUCT" => TokenKind::Struct,
        "END_STRUCT" => TokenKind::EndStruct,
        "ARRAY" => TokenKind::Array,
        "OF" => TokenKind::Of,
        "CONFIGURATION" => TokenKind::Configuration,
        "END_CONFIGURATION" => TokenKind::EndConfiguration,
        "RESOURCE" => TokenKind::Resource,
        "END_RESOURCE" => TokenKind::EndResource,
        "TASK" => TokenKind::Task,
        "WITH" => TokenKind::With,
        "ON" => TokenKind::On,
        "IF" => TokenKind::If,
        "THEN" => TokenKind::Then,
        "ELSIF" => TokenKind::Elsif,
        "ELSE" => TokenKind::Else,
        "END_IF" => TokenKind::EndIf,
        "CASE" => TokenKind::Case,
        "END_CASE" => TokenKind::EndCase,
        "FOR" => TokenKind::For,
        "TO" => TokenKind::To,
        "BY" => TokenKind::By,
        "DO" => TokenKind::Do,
        "END_FOR" => TokenKind::EndFor,
        "WHILE" => TokenKind::While,
        "END_WHILE" => TokenKind::EndWhile,
        "REPEAT" => TokenKind::Repeat,
        "UNTIL" => TokenKind::Until,
        "END_REPEAT" => TokenKind::EndRepeat,
        "EXIT" => TokenKind::Exit,
        "RETURN" => TokenKind::Return,
        "TRUE" => TokenKind::True,
        "FALSE" => TokenKind::False,
        "AND" => TokenKind::And,
        "OR" => TokenKind::Or,
        "XOR" => TokenKind::Xor,
        "NOT" => TokenKind::Not,
        "MOD" => TokenKind::Mod,
        "REFERENCE_TO" => TokenKind::ReferenceTo,
        "REF_TO" => TokenKind::RefTo,
        "POINTER" => TokenKind::Pointer,
        "DREF" => TokenKind::Dref,
        "REF" => TokenKind::Ref,
        "NULL" => TokenKind::Null,
        "__NEW" => TokenKind::New,
        "__DELETE" => TokenKind::Delete,
        "METHOD" => TokenKind::Method,
        "END_METHOD" => TokenKind::EndMethod,
        "INTERFACE" => TokenKind::Interface,
        "END_INTERFACE" => TokenKind::EndInterface,
        "EXTENDS" => TokenKind::Extends,
        "IMPLEMENTS" => TokenKind::Implements,
        "THIS" => TokenKind::This,
        "SUPER" => TokenKind::Super,
        "PROPERTY" => TokenKind::Property,
        "END_PROPERTY" => TokenKind::EndProperty,
        "GET" => TokenKind::Get,
        "END_GET" => TokenKind::EndGet,
        "SET" => TokenKind::Set,
        "END_SET" => TokenKind::EndSet,
        "ABSTRACT" => TokenKind::Abstract,
        "FINAL" => TokenKind::Final,
        "OVERRIDE" => TokenKind::Override,
        "PUBLIC" => TokenKind::Public,
        "PRIVATE" => TokenKind::Private,
        "PROTECTED" => TokenKind::Protected,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        lex(source, "fixture.st", LexMode::Source)
            .unwrap()
            .into_iter()
            .map(|token| token.kind)
            .collect()
    }

    #[test]
    fn keywords_use_longer_identifier_semantics() {
        assert_eq!(
            kinds("PROGRAM programmer VAR var123 END_VAR END_PROGRAM"),
            vec![
                TokenKind::Program,
                TokenKind::Identifier,
                TokenKind::Var,
                TokenKind::Identifier,
                TokenKind::EndVar,
                TokenKind::EndProgram,
            ]
        );
    }

    #[test]
    fn preserves_literal_precedence() {
        assert_eq!(
            kinds("T#1h2m3s DT#2024-1-2-3:4:5.6 D#2024-1-2 TOD#3:4 BYTE#16#FF 3.14 16#FF"),
            vec![
                TokenKind::TimeLiteral,
                TokenKind::DateTimeLiteral,
                TokenKind::DateLiteral,
                TokenKind::TimeOfDayLiteral,
                TokenKind::TypedLiteral,
                TokenKind::RealLiteral,
                TokenKind::IntegerLiteral,
            ]
        );
    }

    #[test]
    fn nested_comments_are_skipped() {
        let tokens = lex(
            "PROGRAM main (* outer (* nested *) end *) END_PROGRAM",
            "fixture.st",
            LexMode::Source,
        )
        .unwrap();
        assert_eq!(
            tokens.iter().map(|token| token.kind).collect::<Vec<_>>(),
            vec![
                TokenKind::Program,
                TokenKind::Identifier,
                TokenKind::EndProgram
            ]
        );
    }

    #[test]
    fn external_pragma_handles_nested_braces_strings_and_comments() {
        let tokens = lex(
            "{external if (x) { f(\"}\"); /* } */ } } PROGRAM p END_PROGRAM",
            "fixture.st",
            LexMode::Source,
        )
        .unwrap();
        assert_eq!(tokens[0].kind, TokenKind::ExternalPragma);
        assert!(tokens[0].image.contains("f(\"}\")"));
        assert_eq!(tokens[1].kind, TokenKind::Program);
    }

    #[test]
    fn direct_addresses_and_multichar_operators_match_priority() {
        assert_eq!(
            kinds("%IX0.1 %QD2.3 a := b ** 2; x REF= y; z => q; 1..10"),
            vec![
                TokenKind::DirectAddress,
                TokenKind::DirectAddress,
                TokenKind::Identifier,
                TokenKind::Assign,
                TokenKind::Identifier,
                TokenKind::Power,
                TokenKind::IntegerLiteral,
                TokenKind::Semicolon,
                TokenKind::Identifier,
                TokenKind::RefAssign,
                TokenKind::Identifier,
                TokenKind::Semicolon,
                TokenKind::Identifier,
                TokenKind::OutputAssign,
                TokenKind::Identifier,
                TokenKind::Semicolon,
                TokenKind::IntegerLiteral,
                TokenKind::DoubleDot,
                TokenKind::IntegerLiteral,
            ]
        );
    }

    #[test]
    fn source_spans_are_one_indexed_and_multiline() {
        let tokens = lex("\n  PROGRAM\nmain", "fixture.st", LexMode::Source).unwrap();
        assert_eq!(tokens[0].span, SourceSpan::new("fixture.st", 2, 2, 3, 9));
        assert_eq!(tokens[1].span, SourceSpan::new("fixture.st", 3, 3, 1, 4));
    }

    #[test]
    fn test_keywords_are_mode_specific() {
        let source = lex("TEST ASSERT_EQ END_TEST", "fixture.st", LexMode::Source).unwrap();
        assert!(source
            .iter()
            .all(|token| token.kind == TokenKind::Identifier));
        let test = lex("TEST ASSERT_EQ END_TEST", "fixture.st", LexMode::Test).unwrap();
        assert_eq!(
            test.iter().map(|token| token.kind).collect::<Vec<_>>(),
            vec![TokenKind::Test, TokenKind::AssertEq, TokenKind::EndTest]
        );
    }
}
