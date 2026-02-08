
#[derive(Debug)]
pub struct SpannedToken {
    token: Token,
    line: usize,
    column: usize,
}

#[derive(Debug, PartialEq)]
enum Token {
    Instruction(String),
    Register(String),
    Immediate(i32),
    Label(String),
    Colon,
    Directive(String),
    Comma,
    LParenthesis,
    RParenthesis,
    Newline,
    EOF,
}

pub fn tokenize(source: &str) -> Vec<SpannedToken> {
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
        token: Token::EOF,
        line,
        column,
    });

    tokens
}

fn classify_identifier(ident: &str) -> Token {
    if ident.starts_with('x') && ident[1..].chars().all(|c| c.is_ascii_digit()) {
        Token::Register(ident.to_string())
    } else {
        match ident {
            "zero" | "ra" | "sp" | "gp" | "tp" | "fp" | "s0" | "s1" | "s2" | "s3" | "s4" | "s5" |
            "s6" | "s7" | "s8" | "s9" | "s10" | "s11" | "a0" | "a1" | "a2" | "a3" | "a4" | "a5" |
            "a6" | "a7" | "t0" | "t1" | "t2" | "t3" | "t4" | "t5" | "t6" => Token::Register(ident.to_string()),

            "add" | "sub" | "and" | "or" | "xor" | "sll" | "srl" | "sra" | "slt" | "sltu" |
            "addi" | "andi" | "ori" | "xori" | "slli" | "srli" | "srai" | "slti" | "sltiu" |
            "lw" | "sw" | "beq" | "bne" | "blt" | "bge" | "jal" | "jalr" => Token::Instruction(ident.to_string()),

            _ => Token::Label(ident.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize() {
        let source = "add x2, x0, x3\nsub x4, x5, x6";
        let tokens = tokenize(source);

        assert_eq!(tokens.len(), 14); // 13 tokens + EOF

        assert_eq!(tokens[0].token, Token::Instruction("add".to_string()));
        assert_eq!(tokens[1].token, Token::Register("x2".to_string()));
        assert_eq!(tokens[2].token, Token::Comma);
        assert_eq!(tokens[3].token, Token::Register("x0".to_string()));
        assert_eq!(tokens[4].token, Token::Comma);
        assert_eq!(tokens[5].token, Token::Register("x3".to_string()));
        assert_eq!(tokens[6].token, Token::Newline);
        assert_eq!(tokens[7].token, Token::Instruction("sub".to_string()));
        assert_eq!(tokens[8].token, Token::Register("x4".to_string()));
        assert_eq!(tokens[9].token, Token::Comma);
        assert_eq!(tokens[10].token, Token::Register("x5".to_string()));
        assert_eq!(tokens[11].token, Token::Comma);
        assert_eq!(tokens[12].token, Token::Register("x6".to_string()));
        assert_eq!(tokens[13].token, Token::EOF);
    }

    #[test]
    fn test_tokenize_label_and_comment() {
        let source = "loop: add x1, x1, x2 # This is a comment\n";
        let tokens = tokenize(source);

        assert_eq!(tokens.len(), 10); // 9 tokens + EOF

        assert_eq!(tokens[0].token, Token::Label("loop".to_string()));
        assert_eq!(tokens[1].token, Token::Colon);
        assert_eq!(tokens[2].token, Token::Instruction("add".to_string()));
        assert_eq!(tokens[3].token, Token::Register("x1".to_string()));
        assert_eq!(tokens[4].token, Token::Comma);
        assert_eq!(tokens[5].token, Token::Register("x1".to_string()));
        assert_eq!(tokens[6].token, Token::Comma);
        assert_eq!(tokens[7].token, Token::Register("x2".to_string()));
        assert_eq!(tokens[8].token, Token::Newline);
        assert_eq!(tokens[9].token, Token::EOF);
    }

    #[test]
    fn test_directives() {
        let source = ".text\n.align 2\n.global main";
        let tokens = tokenize(source);
        assert_eq!(tokens.len(), 8); // 7 tokens + EOF
        assert_eq!(tokens[0].token, Token::Directive(".text".to_string()));
        assert_eq!(tokens[1].token, Token::Newline);
        assert_eq!(tokens[2].token, Token::Directive(".align".to_string()));
        assert_eq!(tokens[3].token, Token::Immediate(2));
        assert_eq!(tokens[4].token, Token::Newline);
        assert_eq!(tokens[5].token, Token::Directive(".global".to_string()));
        assert_eq!(tokens[6].token, Token::Label("main".to_string()));
        assert_eq!(tokens[7].token, Token::EOF);
    }

    // TODO strings
    // TODO example: jump to label
}