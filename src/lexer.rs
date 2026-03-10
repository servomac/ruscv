use std::fmt;

#[derive(Debug, PartialEq, Clone)]
pub enum LexErrorKind {
    UnexpectedChar(char),
    UnterminatedString,
    UnknownEscapeSequence(char),
    InvalidNumber(String),
    EmptyNumberPrefix(String),
    InvalidRegister(String),
    EmptyDirective,
    NumericOverflow(String),
}

#[derive(Debug, PartialEq, Clone)]
pub struct LexError {
    pub line: usize,
    pub column: usize,
    pub kind: LexErrorKind,
}

impl LexError {
    pub fn new(line: usize, column: usize, kind: LexErrorKind) -> Self {
        Self { line, column, kind }
    }
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            LexErrorKind::UnexpectedChar(c) => write!(f, "Unexpected character '{}'", c),
            LexErrorKind::UnterminatedString => write!(f, "Unterminated string literal"),
            LexErrorKind::UnknownEscapeSequence(c) => write!(f, "Unknown escape sequence '\\{}'", c),
            LexErrorKind::InvalidNumber(s) => write!(f, "Invalid number format: '{}'", s),
            LexErrorKind::EmptyNumberPrefix(p) => write!(f, "Empty number prefix: '{}'", p),
            LexErrorKind::InvalidRegister(s) => write!(f, "Invalid register name: '{}'", s),
            LexErrorKind::EmptyDirective => write!(f, "Directives must have a name (e.g., '.word')"),
            LexErrorKind::NumericOverflow(s) => write!(f, "Numeric value '{}' overflows i32", s),
        }
    }
}

#[derive(Debug)]
pub struct SpannedToken {
    pub token: Token,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Token {
    Instruction(String),
    Register(u8),
    Immediate(i32),
    StringLiteral(String),
    Label(String),
    Colon,
    Directive(String),
    Comma,
    LParenthesis,
    RParenthesis,
    Newline,
    Eof,
}

pub fn tokenize(source: &str) -> Result<Vec<SpannedToken>, LexError> {
    let mut tokens = Vec::new();
    let mut line = 1;
    let mut column = 1;
    let mut chars = source.chars().peekable();

    while let Some(char) = chars.next() {
        match char {
            ' '  | '\t' | '\r' => {
                column += 1;
                continue;
            }
            '\n' => {
                tokens.push(SpannedToken {
                    token: Token::Newline,
                    line,
                    column,
                });
                line += 1;
                column = 1;
            }
            '#' => {
                column += 1;
                while let Some(&next_char) = chars.peek() {
                    if next_char == '\n' {
                        break;
                    }
                    chars.next();
                    column += 1;
                }
                continue;
            }
            ':' => {
                tokens.push(SpannedToken {
                    token: Token::Colon,
                    line,
                    column,
                });
                column += 1;
            }
            ',' => {
                tokens.push(SpannedToken {
                    token: Token::Comma,
                    line,
                    column,
                });
                column += 1;
            }
            '(' => {
                tokens.push(SpannedToken {
                    token: Token::LParenthesis,
                    line,
                    column,
                });
                column += 1;
            }
            ')' => {
                tokens.push(SpannedToken {
                    token: Token::RParenthesis,
                    line,
                    column,
                });
                column += 1;
            }
            '.' => {
                let start_column = column;
                let directive = consume_identifier(&mut chars, &mut column, char);
                if directive.len() == 1 {
                    return Err(LexError::new(line, start_column, LexErrorKind::EmptyDirective));
                }
                tokens.push(SpannedToken {
                    token: Token::Directive(directive.to_lowercase()),
                    line,
                    column: start_column,
                });
            }
            '"' => {
                let start_column = column;
                column += 1;
                let token = read_string_literal(&mut chars, line, start_column, &mut column)?;
                tokens.push(token);
            }
            '0'..='9' | '-' => {
                // TODO is coherent to increase the column before calling read_string_literal, but not here?
                let token = read_number(char, &mut chars, line, &mut column)?;
                tokens.push(token);
            }
            'A'..='Z' | 'a'..='z' | '_' => {
                let start_column = column;
                let identifier = consume_identifier(&mut chars, &mut column, char);
                tokens.push(SpannedToken {
                    token: classify_identifier(&identifier, line, start_column)?,
                    line,
                    column: start_column,
                });
            }
            _ => {
                return Err(LexError::new(line, column, LexErrorKind::UnexpectedChar(char)));
            }
        }
    }

    tokens.push(SpannedToken {
        token: Token::Eof,
        line,
        column,
    });

    Ok(tokens)
}

fn consume_identifier(
    chars: &mut std::iter::Peekable<std::str::Chars>,
    column: &mut usize,
    first_char: char,
) -> String {
    let mut identifier = String::new();
    identifier.push(first_char);
    *column += 1;
    while let Some(&next_char) = chars.peek() {
        if next_char.is_alphanumeric() || next_char == '_' {
            identifier.push(next_char);
            chars.next();
            *column += 1;
        } else {
            break;
        }
    }
    identifier
}

fn read_string_literal(chars: &mut std::iter::Peekable<std::str::Chars>, line: usize, start_column: usize, column: &mut usize) -> Result<SpannedToken, LexError> {
    let mut string_literal = String::new();
    let mut unterminated = true;
    while let Some(next_char) = chars.next() {
        *column += 1;
        if next_char == '"' {
            unterminated = false;
            break;
        }
        if next_char == '\\' {
            if let Some(escaped_char) = chars.next() {
                *column += 1;
                match escaped_char {
                    'n' => string_literal.push('\n'),
                    't' => string_literal.push('\t'),
                    '\\' => string_literal.push('\\'),
                    '"' => string_literal.push('"'),
                    _ => return Err(LexError::new(line, *column, LexErrorKind::UnknownEscapeSequence(escaped_char))),
                }
            } else {
                return Err(LexError::new(line, *column, LexErrorKind::UnterminatedString));
            }
        } else {
            string_literal.push(next_char);
        }
    }
    if unterminated {
        return Err(LexError::new(line, *column, LexErrorKind::UnterminatedString));
    }
    Ok(SpannedToken {
        token: Token::StringLiteral(string_literal),
        line,
        column: start_column,
    })
}

fn read_number(first_char: char, chars: &mut std::iter::Peekable<std::str::Chars>, line: usize, column: &mut usize) -> Result<SpannedToken, LexError> {
    let start_column = *column;
    let is_negative = first_char == '-';
    let mut radix = 10;
    let mut number_str = String::new();

    if is_negative {
        match chars.peek() {
            Some(&c) if c.is_digit(10) => {
                // continue to parse
            }
            _ => return Err(LexError::new(line, start_column, LexErrorKind::UnexpectedChar('-'))),
        }
    } else {
        number_str.push(first_char);
    }
    *column += 1;

    // check for prefix
    if (is_negative && chars.peek() == Some(&'0')) || (!is_negative && first_char == '0') {
        if is_negative {
            chars.next(); // consume '0'
            *column += 1;
        }

        if let Some(&prefix) = chars.peek() {
            match prefix {
                'x' | 'X' => { radix = 16; chars.next(); *column += 1; }
                'b' | 'B' => { radix = 2;  chars.next(); *column += 1; }
                'o' | 'O' => { radix = 8;  chars.next(); *column += 1; }
                _ => {
                    if !is_negative {
                        // already pushed '0'
                    } else {
                        number_str.push('0');
                    }
                }
            }

            if radix != 10 {
                // Check if we have at least one digit after the prefix
                match chars.peek() {
                    Some(&c) if c.is_digit(radix) || (radix == 16 && c.is_ascii_hexdigit()) => {}
                    _ => {
                        let prefix_str = if is_negative { format!("-0{}", prefix) } else { format!("0{}", prefix) };
                        return Err(LexError::new(line, *column, LexErrorKind::EmptyNumberPrefix(prefix_str)));
                    }
                }
            }
        } else if is_negative {
            number_str.push('0');
        }
    }

    while let Some(&next) = chars.peek() {
        if next.is_digit(radix) || (radix == 16 && next.is_ascii_hexdigit()) {
            number_str.push(next);
            chars.next();
            *column += 1;
        } else {
            break;
        }
    }

    let val = if is_negative {
        match i32::from_str_radix(&number_str, radix) {
            Ok(v) => -v,
            Err(_) => return Err(LexError::new(line, start_column, LexErrorKind::NumericOverflow(number_str))),
        }
    } else {
        match u32::from_str_radix(&number_str, radix) {
            Ok(v) => v as i32,
            Err(_) => return Err(LexError::new(line, start_column, LexErrorKind::NumericOverflow(number_str))),
        }
    };

    Ok(SpannedToken {
        token: Token::Immediate(val),
        line,
        column: start_column,
    })
}

fn classify_identifier(ident: &str, line: usize, column: usize) -> Result<Token, LexError> {
    let lower_ident = ident.to_lowercase();

    // Check if it looks like a register (x0-x31)
    if lower_ident.starts_with('x') && lower_ident.len() > 1 && lower_ident[1..].chars().all(|c| c.is_ascii_digit()) {
        if let Ok(num) = lower_ident[1..].parse::<u8>() {
            if num <= 31 {
                return Ok(Token::Register(num));
            } else {
                return Err(LexError::new(line, column, LexErrorKind::InvalidRegister(ident.to_string())));
            }
        }
    }

    // Try to match the identifier with the ABI register names
    if let Some(reg_num) = abi_to_register(&lower_ident) {
        return Ok(Token::Register(reg_num));
    }

    // Instructions
    let is_instruction = match lower_ident.as_str() {
        "add" | "sub" | "and" | "or" | "xor" | "sll" | "srl" | "sra" | "slt" | "sltu" |
        "addi" | "andi" | "ori" | "xori" | "slli" | "srli" | "srai" | "slti" | "sltiu" |
        "lw" | "sw" | "lb" | "lh" | "lbu" | "lhu" | "sb" | "sh" |
        "beq" | "bne" | "blt" | "bge" | "bltu" | "bgeu" |
        "jal" | "jalr" | "lui" | "auipc" | "ecall" | "ebreak" => true,
        _ => false,
    };

    if is_instruction {
        Ok(Token::Instruction(lower_ident))
    } else {
        // Labels are case-sensitive usually, we preserve original case
        Ok(Token::Label(ident.to_string()))
    }
}

fn abi_to_register(ident: &str) -> Option<u8> {
    match ident {
        "zero" => Some(0),
        "ra" => Some(1),
        "sp" => Some(2),
        "gp" => Some(3),
        "tp" => Some(4),
        "t0" => Some(5),
        "t1" => Some(6),
        "t2" => Some(7),
        "s0" | "fp" => Some(8),
        "s1" => Some(9),
        "a0" => Some(10), "a1" => Some(11), "a2" => Some(12), "a3" => Some(13),
        "a4" => Some(14), "a5" => Some(15), "a6" => Some(16), "a7" => Some(17),
        "s2" => Some(18),
        "s3" => Some(19),
        "s4" => Some(20), "s5" => Some(21), "s6" => Some(22), "s7" => Some(23),
        "s8" => Some(24), "s9" => Some(25), "s10" => Some(26), "s11" => Some(27),
        "t3" => Some(28), "t4" => Some(29), "t5" => Some(30), "t6" => Some(31),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize() {
        let source = "add x2, zero, x3\nsub x4, x5, x6";
        let tokens = tokenize(source).expect("Should tokenize successfully");

        assert_eq!(tokens.len(), 14); // 13 tokens + Eof

        assert_eq!(tokens[0].token, Token::Instruction("add".to_string()));
        assert_eq!(tokens[1].token, Token::Register(2));
        assert_eq!(tokens[2].token, Token::Comma);
        assert_eq!(tokens[3].token, Token::Register(0));
        assert_eq!(tokens[4].token, Token::Comma);
        assert_eq!(tokens[5].token, Token::Register(3));
        assert_eq!(tokens[6].token, Token::Newline);
        assert_eq!(tokens[7].token, Token::Instruction("sub".to_string()));
        assert_eq!(tokens[8].token, Token::Register(4));
        assert_eq!(tokens[9].token, Token::Comma);
        assert_eq!(tokens[10].token, Token::Register(5));
        assert_eq!(tokens[11].token, Token::Comma);
        assert_eq!(tokens[12].token, Token::Register(6));
        assert_eq!(tokens[13].token, Token::Eof);
    }

    #[test]
    fn test_tokenize_label_and_comment() {
        let source = "loop: add x1, x1, x2 # This is a comment\n";
        let tokens = tokenize(source).expect("Should tokenize successfully");

        assert_eq!(tokens.len(), 10); // 9 tokens + Eof

        assert_eq!(tokens[0].token, Token::Label("loop".to_string()));
        assert_eq!(tokens[1].token, Token::Colon);
        assert_eq!(tokens[2].token, Token::Instruction("add".to_string()));
        assert_eq!(tokens[3].token, Token::Register(1));
        assert_eq!(tokens[4].token, Token::Comma);
        assert_eq!(tokens[5].token, Token::Register(1));
        assert_eq!(tokens[6].token, Token::Comma);
        assert_eq!(tokens[7].token, Token::Register(2));
        assert_eq!(tokens[8].token, Token::Newline);
        assert_eq!(tokens[9].token, Token::Eof);
    }

    #[test]
    fn test_directives() {
        let source = ".text\n.align 2\n.global main";
        let tokens = tokenize(source).expect("Should tokenize successfully");
        assert_eq!(tokens.len(), 8); // 7 tokens + Eof
        assert_eq!(tokens[0].token, Token::Directive(".text".to_string()));
        assert_eq!(tokens[1].token, Token::Newline);
        assert_eq!(tokens[2].token, Token::Directive(".align".to_string()));
        assert_eq!(tokens[3].token, Token::Immediate(2));
        assert_eq!(tokens[4].token, Token::Newline);
        assert_eq!(tokens[5].token, Token::Directive(".global".to_string()));
        assert_eq!(tokens[6].token, Token::Label("main".to_string()));
        assert_eq!(tokens[7].token, Token::Eof);
    }

    #[test]
    fn test_strings() {
        let source = r#".string "Hello, %s!\n""#;
        let tokens = tokenize(source).expect("Should tokenize successfully");
        assert_eq!(tokens.len(), 3); // 2 tokens + Eof
        assert_eq!(tokens[0].token, Token::Directive(".string".to_string()));
        assert_eq!(tokens[1].token, Token::StringLiteral("Hello, %s!\n".to_string()));
    }

    #[test]
    fn test_immediate_negative_numbers() {
        let source = "addi sp, sp, -16";
        let tokens = tokenize(source).expect("Should tokenize successfully");
        assert_eq!(tokens.len(), 7); // 6 tokens + Eof
        assert_eq!(tokens[0].token, Token::Instruction("addi".to_string()));
        assert_eq!(tokens[1].token, Token::Register(2));
        assert_eq!(tokens[2].token, Token::Comma);
        assert_eq!(tokens[3].token, Token::Register(2));
        assert_eq!(tokens[4].token, Token::Comma);
        assert_eq!(tokens[5].token, Token::Immediate(-16));
    }

    #[test]
    fn test_inmediate_hexadecimal() {
        let source = "addi a0, sp, 0xFF";
        let tokens = tokenize(source).expect("Should tokenize successfully");
        assert_eq!(tokens.len(), 7); // 6 tokens + Eof
        assert_eq!(tokens[0].token, Token::Instruction("addi".to_string()));
        assert_eq!(tokens[1].token, Token::Register(10));
        assert_eq!(tokens[2].token, Token::Comma);
        assert_eq!(tokens[3].token, Token::Register(2));
        assert_eq!(tokens[4].token, Token::Comma);
        assert_eq!(tokens[5].token, Token::Immediate(255)); // 0xFF is 255 in decimal
    }
    // TODO test lines and columns in SpannedToken

    #[test]
    fn test_column_tracking() {
        // Test that strings and numbers store the START column
        // and that subsequent tokens have the correct columns
        let source = "  123   \"hello\"   add";
        //            123456789012345678901
        //            ^  ^     ^         ^
        //            1  3     9         19
        let tokens = tokenize(source).expect("Should tokenize successfully");

        assert_eq!(tokens.len(), 4); // 3 tokens + Eof

        // Number "123" starts at column 3
        assert_eq!(tokens[0].token, Token::Immediate(123));
        assert_eq!(tokens[0].column, 3);

        // String "\"hello\"" starts at column 9
        assert_eq!(tokens[1].token, Token::StringLiteral("hello".to_string()));
        assert_eq!(tokens[1].column, 9);

        // Instruction "add" starts at column 19
        assert_eq!(tokens[2].token, Token::Instruction("add".to_string()));
        assert_eq!(tokens[2].column, 19);
    }

    #[test]
    fn test_lex_errors() {
        // Unexpected character
        let res = tokenize("add x1, x2, @");
        assert_eq!(res.unwrap_err(), LexError::new(1, 13, LexErrorKind::UnexpectedChar('@')));

        // Unterminated string
        let res = tokenize(".string \"Hello");
        assert_eq!(res.unwrap_err(), LexError::new(1, 15, LexErrorKind::UnterminatedString));

        // Unknown escape sequence
        let res = tokenize(".string \"Hello\\z\"");
        assert_eq!(res.unwrap_err(), LexError::new(1, 17, LexErrorKind::UnknownEscapeSequence('z')));
    }

    #[test]
    fn test_case_insensitivity() {
        let source = "ADD X1, ZERO, x2";
        let tokens = tokenize(source).expect("Should tokenize successfully");
        assert_eq!(tokens[0].token, Token::Instruction("add".to_string()));
        assert_eq!(tokens[1].token, Token::Register(1));
        assert_eq!(tokens[3].token, Token::Register(0));
        assert_eq!(tokens[5].token, Token::Register(2));
    }

    #[test]
    fn test_invalid_register() {
        let res = tokenize("add x32, x1, x2");
        assert_eq!(res.unwrap_err(), LexError::new(1, 5, LexErrorKind::InvalidRegister("x32".to_string())));
    }

    #[test]
    fn test_empty_directive() {
        let res = tokenize(". ");
        assert_eq!(res.unwrap_err(), LexError::new(1, 1, LexErrorKind::EmptyDirective));
    }

    #[test]
    fn test_robust_numbers() {
        // Negative hex
        let res = tokenize("addi a0, a0, -0x10");
        let tokens = res.expect("Should tokenize successfully");
        assert_eq!(tokens[5].token, Token::Immediate(-16));

        // Empty prefix
        let res = tokenize("addi a0, a0, 0x");
        assert_eq!(res.unwrap_err(), LexError::new(1, 16, LexErrorKind::EmptyNumberPrefix("0x".to_string())));

        // Negative empty prefix
        let res = tokenize("addi a0, a0, -0b");
        assert_eq!(res.unwrap_err(), LexError::new(1, 17, LexErrorKind::EmptyNumberPrefix("-0b".to_string())));

        // Binary
        let res = tokenize("0b1010");
        let tokens = res.expect("Should tokenize successfully");
        assert_eq!(tokens[0].token, Token::Immediate(10));
    }

    #[test]
    fn test_numeric_overflow() {
        // Now we allow up to u32::MAX for positive literals (interpreted as bit patterns)
        let res = tokenize("4294967296"); // u32::MAX + 1
        assert_eq!(res.unwrap_err(), LexError::new(1, 1, LexErrorKind::NumericOverflow("4294967296".to_string())));

        // Negative numbers are still restricted to i32 range
        let res = tokenize("-2147483649"); // i32::MIN - 1
        // number_str in error kind does not include the '-' sign
        assert_eq!(res.unwrap_err(), LexError::new(1, 1, LexErrorKind::NumericOverflow("2147483649".to_string())));

        // Verify hex bit pattern support (0xDEADBEEF)
        let res = tokenize("0xDEADBEEF");
        let tokens = res.expect("Should tokenize successfully");
        assert_eq!(tokens[0].token, Token::Immediate(-559038737)); // cast to i32
    }
}
