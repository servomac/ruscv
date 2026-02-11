
#[derive(Debug)]
pub struct SpannedToken {
    pub token: Token,
    pub line: usize,
    column: usize,
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

pub fn tokenize(source: &str) -> Vec<SpannedToken> {
    // TODO return Result<Vec<SpannedToken>, LexError> instead of panicking on errors
    // TODO handle tabs and other whitespace correctly for column counting
    let mut tokens = Vec::new();
    let mut line = 1;
    let mut column = 1;
    let mut chars = source.chars().peekable();

    while let Some(char) = chars.next() {
        match char {
            ' '  | '\t' => {
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
                let mut directive = char.to_string();
                while let Some(&next_char) = chars.peek() {
                    if next_char.is_alphanumeric() || next_char == '_' {
                        directive.push(next_char);
                        chars.next();
                        column += 1;
                    } else {
                        break;
                    }
                }
                tokens.push(SpannedToken {
                    token: Token::Directive(directive),
                    line,
                    column,
                });
            }
            '"' => {
                // TODO extract to read_string_literal function
                // TODO column is the end of the string, not the start, fix it
                let mut string_literal = String::new();
                while let Some(next_char) = chars.next() {
                    column += 1;
                    if next_char == '"' {
                        break;
                    }
                    if next_char == '\\' {
                        if let Some(escaped_char) = chars.next() {
                            column += 1;
                            match escaped_char {
                                'n' => string_literal.push('\n'),
                                't' => string_literal.push('\t'),
                                '\\' => string_literal.push('\\'),
                                '"' => string_literal.push('"'),
                                _ => panic!("Unknown escape sequence \\{}", escaped_char),
                            }
                        } else {
                            panic!("Unterminated string literal at line {}, column {}", line, column);
                        }
                    } else {
                        string_literal.push(next_char);
                    }
                }
                tokens.push(SpannedToken {
                    token: Token::StringLiteral(string_literal),
                    line,
                    column,
                });
            }
            '0'..='9' | '-' => {
                let mut number_str = char.to_string();
                while let Some(&next_char) = chars.peek() {
                    if next_char.is_ascii_digit() {
                        number_str.push(next_char);
                        chars.next();
                        column += 1;
                    } else {
                        break;
                    }
                }
                let number = number_str.parse::<i32>().unwrap();
                tokens.push(SpannedToken {
                    token: Token::Immediate(number),
                    line,
                    column,
                });
                column += 1;
            }
            'A'..='Z' | 'a'..='z' | '_' => {
                let mut identifier = char.to_string();
                while let Some(&next_char) = chars.peek() {
                    if next_char.is_alphanumeric() || next_char == '_' {
                        identifier.push(next_char);
                        chars.next();
                        column += 1;
                    } else {
                        break;
                    }
                }
                tokens.push(SpannedToken {
                    token: classify_identifier(&identifier),
                    line,
                    column,
                });
            }
            _ => {
                panic!("Unexpected character '{}' at line {}, column {}", char, line, column);
            }
        }
    }

    tokens.push(SpannedToken {
        token: Token::Eof,
        line,
        column,
    });

    tokens
}

fn classify_identifier(ident: &str) -> Token {
    // Lets search for registers first, since they can be confused with labels or instructions
    if ident.starts_with('x') && ident.len() > 1 {
        if let Ok(num) = ident[1..].parse::<u8>() {
            if num <= 31 {
                return Token::Register(num);
            }
        }
    }

    // Try to match the identifier with the ABI register names (like "zero", "ra", "sp", etc)
    if let Some(reg_num) = abi_to_register(ident) {
        return Token::Register(reg_num);
    }

    // If its not a register, it can be an instruction, a directive or a label
    match ident {
        "add" | "sub" | "and" | "or" | "xor" | "sll" | "srl" | "sra" | "slt" | "sltu" |
        "addi" | "andi" | "ori" | "xori" | "slli" | "srli" | "srai" | "slti" | "sltiu" |
        "lw" | "sw" | "beq" | "bne" | "blt" | "bge" | "jal" | "jalr" => {
            Token::Instruction(ident.to_string())
        }
        
        // Si no es nada de lo anterior, es una etiqueta (label)
        _ => Token::Label(ident.to_string()),
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
        let tokens = tokenize(source);

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
        let tokens = tokenize(source);

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
        let tokens = tokenize(source);
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
        let tokens = tokenize(source);
        assert_eq!(tokens.len(), 3); // 2 tokens + Eof
        assert_eq!(tokens[0].token, Token::Directive(".string".to_string()));
        assert_eq!(tokens[1].token, Token::StringLiteral("Hello, %s!\n".to_string()));
    }

    #[test]
    fn test_immediate_negative_numbers() {
        let source = "addi sp, sp, -16";
        let tokens = tokenize(source);
        assert_eq!(tokens.len(), 7); // 6 tokens + Eof
        assert_eq!(tokens[0].token, Token::Instruction("addi".to_string()));
        assert_eq!(tokens[1].token, Token::Register(2));
        assert_eq!(tokens[2].token, Token::Comma);
        assert_eq!(tokens[3].token, Token::Register(2));
        assert_eq!(tokens[4].token, Token::Comma);
        assert_eq!(tokens[5].token, Token::Immediate(-16));
    }
    // TODO test lines and columns in SpannedToken
}