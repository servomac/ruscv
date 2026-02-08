use crate::lexer::SpannedToken;

enum Operand {
    Register(String),
    Immediate(i32),
    Label(String),
}

pub enum ParsedNode {
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

    pub fn parse(&mut self) -> Vec<ParsedNode> {
        let mut nodes = Vec::new();
        while self.position < self.tokens.len() {
            let token = &self.tokens[self.position];
            println!("Parsing token: {:?}", token);
            self.position += 1;
        }
        nodes
    }
}
