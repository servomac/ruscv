use std::mem::discriminant;
use std::fmt;

use crate::lexer::{SpannedToken, Token, ModifierKind};

#[derive(Debug, PartialEq, Clone)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum MemoryOffset {
    Immediate(i32),
    Label(String),
    Modifier(ModifierKind, String),
}

impl fmt::Display for MemoryOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MemoryOffset::Immediate(n) => write!(f, "{}", n),
            MemoryOffset::Label(s) => write!(f, "{}", s),
            MemoryOffset::Modifier(kind, symbol) => {
                let kind_str = match kind {
                    ModifierKind::Hi => "hi",
                    ModifierKind::Lo => "lo",
                };
                write!(f, "%{}({})", kind_str, symbol)
            }
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum Operand {
    Register(u8),
    Immediate(i32),
    Label(String),
    StringLiteral(String),
    Modifier(ModifierKind, String),
    Memory { offset: MemoryOffset, reg: u8 },
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operand::Register(n) => write!(f, "x{}", n),
            Operand::Immediate(n) => write!(f, "{}", n),
            Operand::Label(s) => write!(f, "{}", s),
            Operand::StringLiteral(s) => write!(f, "\"{}\"", s),
            Operand::Modifier(kind, symbol) => {
                let kind_str = match kind {
                    ModifierKind::Hi => "hi",
                    ModifierKind::Lo => "lo",
                };
                write!(f, "%{}({})", kind_str, symbol)
            }
            Operand::Memory { offset, reg } => write!(f, "{}(x{})", offset, reg),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct Statement {
    pub kind: StatementKind,
    pub line: usize,
}

#[derive(Debug, PartialEq, Clone)]
pub enum StatementKind {
    Instruction(String, Vec<Operand>),
    Label(String),
    Directive(String, Vec<Operand>),
}

impl fmt::Display for Statement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            StatementKind::Instruction(name, ops) => {
                write!(f, "{}", name)?;
                if !ops.is_empty() {
                    write!(f, " ")?;
                    for (i, op) in ops.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", op)?;
                    }
                }
                Ok(())
            }
            StatementKind::Label(name) => write!(f, "{}:", name),
            StatementKind::Directive(name, ops) => {
                write!(f, "{}", name)?;
                if !ops.is_empty() {
                    write!(f, " ")?;
                    for (i, op) in ops.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", op)?;
                    }
                }
                Ok(())
            }
        }
    }
}

pub struct Parser {
    tokens: Vec<SpannedToken>,
    position: usize,
}

impl Parser {
    pub fn new(tokens: Vec<SpannedToken>) -> Self {
        Parser { tokens, position: 0 }
    }

    // Gets the current token without advancing the position
    fn peek(&self) -> &Token {
        &self.tokens[self.position].token
    }

    // Checks if the current token matches the expected token
    fn check(&self, expected: &Token) -> bool {
        if self.is_at_end() { return false; }
        discriminant(self.peek()) == discriminant(expected)
    }

    // Advances the position and returns the current token
    fn advance(&mut self) -> Token {
        if !self.is_at_end() {
            self.position += 1;
        }
        self.previous().clone()
    }

    // Gets the previous token (the one just before the current position)
    fn previous(&self) -> &Token {
        &self.tokens[self.position - 1].token
    }

    // Consumes the expected token and advances the position
    fn consume(&mut self, expected: &Token, error_message: &str) -> Result<Token, ParseError> {
        if self.check(expected) {
            Ok(self.advance())
        } else {
            Err(ParseError {
                line: self.tokens[self.position].line,
                message: format!("{}. Found: {:?}", error_message, self.peek()),
            })
        }
    }

    // Checks if we've reached the end of the token stream
    fn is_at_end(&self) -> bool {
        matches!(self.peek(), Token::Eof)
    }

    // Parses the token stream into a list of statements
    pub fn parse(&mut self) -> Result<Vec<Statement>, ParseError> {
        let mut nodes = Vec::new();
        while !self.is_at_end() {
            match self.parse_line() {
                Ok(Some(stmt)) => nodes.push(stmt),
                Ok(None) => {
                    continue;
                },
                Err(e) => return Err(e),
            }
        }
        Ok(nodes)
    }

    fn parse_line(&mut self) -> Result<Option<Statement>, ParseError> {
        if self.is_at_end() { return Ok(None); }

        let current_token = self.peek().clone();
        let line = self.tokens[self.position].line;

        let statement_kind = match current_token {
            Token::Label(label) => {
                let label_name = label.clone();
                self.advance();
                self.consume(&Token::Colon, "A colon is expected after a label (':')")?;
                StatementKind::Label(label_name)
            },

            Token::Instruction(mnemonic) => {
                self.advance();
                let mut operands = Vec::new();

                if !self.check(&Token::Newline) && !self.is_at_end() {
                    operands.push(self.parse_operand()?);

                    while self.check(&Token::Comma) {
                        self.advance(); // consume the comma
                        operands.push(self.parse_operand()?);
                    }
                }
                StatementKind::Instruction(mnemonic, operands)
            },

            Token::Directive(directive) => {
                self.advance();
                let mut operands = Vec::new();

                if !self.check(&Token::Newline) && !self.is_at_end() {
                    operands.push(self.parse_directive_operand()?);

                    while self.check(&Token::Comma) {
                        self.advance(); // consume the comma
                        operands.push(self.parse_directive_operand()?);
                    }
                }
                StatementKind::Directive(directive, operands)
            },

            Token::Newline => {
                self.advance();
                return Ok(None);
            }

            _ => return Err(ParseError {
                line,
                message: format!("Unexpected token: {:?}", current_token)
            }),

        };

        Ok(Some(Statement { kind: statement_kind, line }))
    }

    fn parse_operand(&mut self) -> Result<Operand, ParseError> {
        let current_token = self.peek().clone();
        let line = self.tokens[self.position].line;

        match current_token {
            Token::Register(reg) => {
                self.advance();
                Ok(Operand::Register(reg))
            }

            Token::Immediate(imm) => {
                self.advance();

                // Check for memory directions
                if self.check(&Token::LParenthesis) {
                    self.advance(); // consume left parenthesis

                    // consume the register inside the parentheses
                    let reg_token = self.consume(
                        &Token::Register(0),
                        "A register was expected inside parentheses for memory addressing"
                    )?;

                    let reg = match reg_token {
                        Token::Register(r) => r,
                        _ => unreachable!(),
                    };

                    self.consume(&Token::RParenthesis, "Right parenthesis expected after base register")?;

                    Ok(Operand::Memory { offset: MemoryOffset::Immediate(imm), reg })
                } else {
                    Ok(Operand::Immediate(imm))
                }
            }

            Token::Label(label) => {
                self.advance();
                // Check if this is a memory operand with label offset
                if self.check(&Token::LParenthesis) {
                    self.advance(); // consume left parenthesis

                    // consume the register inside the parentheses
                    let reg_token = self.consume(
                        &Token::Register(0),
                        "A register was expected inside parentheses for memory addressing"
                    )?;

                    let reg = match reg_token {
                        Token::Register(r) => r,
                        _ => unreachable!(),
                    };

                    self.consume(&Token::RParenthesis, "Right parenthesis expected after base register")?;

                    Ok(Operand::Memory { offset: MemoryOffset::Label(label), reg })
                } else {
                    Ok(Operand::Label(label))
                }
            }

            Token::Modifier(kind, symbol) => {
                self.advance();

                // Check if this is a memory operand with modifier offset
                if self.check(&Token::LParenthesis) {
                    self.advance(); // consume left parenthesis

                    // consume the register inside the parentheses
                    let reg_token = self.consume(
                        &Token::Register(0),
                        "A register was expected inside parentheses for memory addressing"
                    )?;

                    let reg = match reg_token {
                        Token::Register(r) => r,
                        _ => unreachable!(),
                    };

                    self.consume(&Token::RParenthesis, "Right parenthesis expected after base register")?;

                    Ok(Operand::Memory { offset: MemoryOffset::Modifier(kind, symbol), reg })
                } else {
                    Ok(Operand::Modifier(kind, symbol))
                }
            }

            _ => Err(ParseError {
                line,
                message: format!("An operand was expected (register, immediate or label), but was not found: {:?}", current_token),
            }),
        }
    }

    fn parse_directive_operand(&mut self) -> Result<Operand, ParseError> {
        let token = self.peek().clone();

        match token {
            Token::StringLiteral(s) => {
                self.advance();
                Ok(Operand::StringLiteral(s))
            },
            _ => self.parse_operand(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::lexer::tokenize;

    use super::*;

    #[test]
    fn test_r_instruction_parsing() {
        let tokens = tokenize("add x1, x2, x3").unwrap();
        let mut parser = Parser::new(tokens);
        let nodes = parser.parse().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].kind, StatementKind::Instruction("add".to_string(), vec![
            Operand::Register(1),
            Operand::Register(2),
            Operand::Register(3),
        ]));
        assert_eq!(nodes[0].line, 1);
    }

    #[test]
    fn test_i_instruction_parsing() {
        let tokens = tokenize("addi x1, x2, 10").unwrap();
        let mut parser = Parser::new(tokens);
        let nodes = parser.parse().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].kind, StatementKind::Instruction("addi".to_string(), vec![
            Operand::Register(1),
            Operand::Register(2),
            Operand::Immediate(10),
        ]));
        assert_eq!(nodes[0].line, 1);
    }

    #[test]
    fn test_s_instruction_parsing() {
        let tokens = tokenize("sw x1, 4(x2)").unwrap();
        let mut parser = Parser::new(tokens);
        let nodes = parser.parse().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].kind, StatementKind::Instruction("sw".to_string(), vec![
            Operand::Register(1),
            Operand::Memory { offset: MemoryOffset::Immediate(4), reg: 2 },
        ]));
        assert_eq!(nodes[0].line, 1);
    }

    #[test]
    fn test_label_parsing() {
        let tokens = tokenize("loop:\nadd x1, x2, x3").unwrap();
        let mut parser = Parser::new(tokens);
        let nodes = parser.parse().unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].kind, StatementKind::Label("loop".to_string()));
        assert_eq!(nodes[0].line, 1);
        assert_eq!(nodes[1].kind, StatementKind::Instruction("add".to_string(), vec![
            Operand::Register(1),
            Operand::Register(2),
            Operand::Register(3),
        ]));
        assert_eq!(nodes[1].line, 2);
    }

    #[test]
    fn test_directive_parsing() {
        let tokens = tokenize(".data\nmyVar: .word 42").unwrap();
        let mut parser = Parser::new(tokens);
        let nodes = parser.parse().unwrap();
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].kind, StatementKind::Directive(".data".to_string(), vec![]));
        assert_eq!(nodes[0].line, 1);
        assert_eq!(nodes[1].kind, StatementKind::Label("myVar".to_string()));
        assert_eq!(nodes[1].line, 2);
        assert_eq!(nodes[2].kind, StatementKind::Directive(".word".to_string(), vec![
            Operand::Immediate(42),
        ]));
        assert_eq!(nodes[2].line, 2);
    }

    #[test]
    fn test_directive_with_string_parsing() {
        let tokens = tokenize(".asciiz \"Hello, world!\"").unwrap();
        let mut parser = Parser::new(tokens);
        let nodes = parser.parse().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].kind, StatementKind::Directive(".asciiz".to_string(), vec![
            Operand::StringLiteral("Hello, world!".to_string()),
        ]));
        assert_eq!(nodes[0].line, 1);
    }

    #[test]
    fn test_label_in_memory_operand_parsing() {
        let tokens = tokenize("sw x1, my_label(x2)").unwrap();
        let mut parser = Parser::new(tokens);
        let nodes = parser.parse().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].kind, StatementKind::Instruction("sw".to_string(), vec![
            Operand::Register(1),
            Operand::Memory { offset: MemoryOffset::Label("my_label".to_string()), reg: 2 },
        ]));
        assert_eq!(nodes[0].line, 1);
    }

    #[test]
    fn test_modifier_parsing() {
        let tokens = tokenize("lui x1, %hi(label)").unwrap();
        let mut parser = Parser::new(tokens);
        let nodes = parser.parse().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].kind, StatementKind::Instruction("lui".to_string(), vec![
            Operand::Register(1),
            Operand::Modifier(ModifierKind::Hi, "label".to_string()),
        ]));

        let tokens = tokenize("addi x1, x1, %lo(label)").unwrap();
        let mut parser = Parser::new(tokens);
        let nodes = parser.parse().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].kind, StatementKind::Instruction("addi".to_string(), vec![
            Operand::Register(1),
            Operand::Register(1),
            Operand::Modifier(ModifierKind::Lo, "label".to_string()),
        ]));

        let tokens = tokenize("lw x1, %lo(label)(x2)").unwrap();
        let mut parser = Parser::new(tokens);
        let nodes = parser.parse().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].kind, StatementKind::Instruction("lw".to_string(), vec![
            Operand::Register(1),
            Operand::Memory {
                offset: MemoryOffset::Modifier(ModifierKind::Lo, "label".to_string()),
                reg: 2
            },
        ]));
    }
}