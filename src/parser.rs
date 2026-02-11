use std::mem::discriminant;

use crate::lexer::{SpannedToken, Token};

#[derive(Debug, PartialEq)]
enum Operand {
    Register(String),
    Immediate(i32),
    Label(String),
    Memory { offset: i32, base_reg: String },
}

#[derive(Debug, PartialEq)]
pub enum Statement {
    Instruction(String, Vec<Operand>),
    Label(String),
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

    // Consumes the expected token and advances the position, or panics with an error message if it doesn't match
    fn consume(&mut self, expected: &Token, error_message: &str) -> Result<Token, String> {
        if self.check(expected) {
            Ok(self.advance())
        } else {
            Err(format!(
                "Error on line {}: {}. Found: {:?}",
                self.tokens[self.position].line,
                error_message,
                self.peek()
            ))
        }
    }

    // Checks if we've reached the end of the token stream
    fn is_at_end(&self) -> bool {
        matches!(self.peek(), Token::Eof)
    }


    pub fn parse(&mut self) -> Result<Vec<Statement>, String> {
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

    pub fn parse_line(&mut self) -> Result<Option<Statement>, String> {
        if self.is_at_end() { return Ok(None); }

        let current_token = self.peek().clone();

        let statement = match current_token {
            Token::Label(label) => {
                let label_name = label.clone();
                self.advance();
                self.consume(&Token::Colon, "A colon is expected after a label (':')")?;
                Statement::Label(label_name)
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
                Statement::Instruction(mnemonic, operands)
            },
            Token::Newline => {
                self.advance();
                return Ok(None);
            }

            _ => return Err(format!("Unexpected token: {:?}", current_token)),

        };

        Ok(Some(statement))
    }

    fn parse_operand(&mut self) -> Result<Operand, String> {
        let current_token = self.peek().clone();

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
                        &Token::Register(String::new()),
                        "A register was expected inside parentheses for memory addressing"
                    )?;

                    let base_reg = match reg_token {
                        Token::Register(r) => r,
                        _ => unreachable!(),
                    };

                    self.consume(&Token::RParenthesis, "Right parenthesis expected after base register")?;

                    Ok(Operand::Memory { offset: imm, base_reg })
                } else {
                    Ok(Operand::Immediate(imm))
                }
            }

            Token::Label(label) => {
                self.advance();
                Ok(Operand::Label(label))
            }

            _ => Err(format!(
                "An operand was expected (register, inmediate or label), but was not found: {:?}",
                current_token
            )),
        }
    }

}

#[cfg(test)]
mod tests {
    use crate::lexer::tokenize;

    use super::*;

    #[test]
    fn test_r_instruction_parsing() {
        let tokens = tokenize("add x1, x2, x3");
        let mut parser = Parser::new(tokens);
        let nodes = parser.parse().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0], Statement::Instruction("add".to_string(), vec![
            Operand::Register("x1".to_string()),
            Operand::Register("x2".to_string()),
            Operand::Register("x3".to_string()),
        ]));
    }

    #[test]
    fn test_i_instruction_parsing() {
        let tokens = tokenize("addi x1, x2, 10");
        let mut parser = Parser::new(tokens);
        let nodes = parser.parse().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0], Statement::Instruction("addi".to_string(), vec![
            Operand::Register("x1".to_string()),
            Operand::Register("x2".to_string()),
            Operand::Immediate(10),
        ]));
    }

    #[test]
    fn test_s_instruction_parsing() {
        let tokens = tokenize("sw x1, 0(x2)");
        let mut parser = Parser::new(tokens);
        let nodes = parser.parse().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0], Statement::Instruction("sw".to_string(), vec![
            Operand::Register("x1".to_string()),
            Operand::Memory { offset: 0, base_reg: "x2".to_string() },
        ]));
    }

    #[test]
    fn test_label_parsing() {
        let tokens = tokenize("loop:\nadd x1, x2, x3");
        println!("{:#?}", tokens);
        let mut parser = Parser::new(tokens);
        let nodes = parser.parse().unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0], Statement::Label("loop".to_string()));
        assert_eq!(nodes[1], Statement::Instruction("add".to_string(), vec![
            Operand::Register("x1".to_string()),
            Operand::Register("x2".to_string()),
            Operand::Register("x3".to_string()),
        ]));
    }

}