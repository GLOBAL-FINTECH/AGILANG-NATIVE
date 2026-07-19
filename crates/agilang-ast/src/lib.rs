//! Abstract Syntax Tree (AST) definitions for AGILANG.

use agilang_source::Span;

/// A complete AGILANG program.
#[derive(Debug, Clone)]
pub struct Program {
    /// Top-level declarations in the program
    pub declarations: Vec<Declaration>,
}

/// Top-level declarations.
#[derive(Debug, Clone)]
pub enum Declaration {
    /// Function declaration
    Function(FunctionDeclaration),
    /// Variable declaration (at module level)
    Variable(VariableDeclaration),
    /// Constant declaration
    Constant(ConstantDeclaration),
}

/// Function declaration.
#[derive(Debug, Clone)]
pub struct FunctionDeclaration {
    /// Span of the entire declaration
    pub span: Span,
    /// Function name
    pub name: String,
    /// Parameters
    pub parameters: Vec<Parameter>,
    /// Return type
    pub return_type: TypeReference,
    /// Function body
    pub body: Block,
}

/// Function parameter.
#[derive(Debug, Clone)]
pub struct Parameter {
    /// Span of the parameter
    pub span: Span,
    /// Parameter name
    pub name: String,
    /// Parameter type
    pub param_type: TypeReference,
}

/// Variable declaration.
#[derive(Debug, Clone)]
pub struct VariableDeclaration {
    /// Span of the declaration
    pub span: Span,
    /// Variable name
    pub name: String,
    /// Variable type (optional, can be inferred)
    pub var_type: Option<TypeReference>,
    /// Initial value
    pub initializer: Option<Expression>,
}

/// Constant declaration.
#[derive(Debug, Clone)]
pub struct ConstantDeclaration {
    /// Span of the declaration
    pub span: Span,
    /// Constant name
    pub name: String,
    /// Constant type (optional, can be inferred)
    pub const_type: Option<TypeReference>,
    /// Constant value
    pub value: Expression,
}

/// A block of statements.
#[derive(Debug, Clone)]
pub struct Block {
    /// Span of the block
    pub span: Span,
    /// Statements in the block
    pub statements: Vec<Statement>,
}

/// Statements.
#[derive(Debug, Clone)]
pub enum Statement {
    /// Variable declaration
    VariableDeclaration(VariableDeclaration),
    /// Constant declaration
    ConstantDeclaration(ConstantDeclaration),
    /// Return statement
    Return(ReturnStatement),
    /// Expression statement (expression evaluated for side effects)
    Expression(ExpressionStatement),
    /// If statement
    If(IfStatement),
    /// While loop
    While(WhileStatement),
    /// Block statement
    Block(Block),
}

/// Return statement.
#[derive(Debug, Clone)]
pub struct ReturnStatement {
    /// Span of the return statement
    pub span: Span,
    /// Return value (optional for void functions)
    pub value: Option<Expression>,
}

/// Expression statement.
#[derive(Debug, Clone)]
pub struct ExpressionStatement {
    /// Span of the statement
    pub span: Span,
    /// The expression
    pub expression: Expression,
}

/// If statement.
#[derive(Debug, Clone)]
pub struct IfStatement {
    /// Span of the if statement
    pub span: Span,
    /// Condition
    pub condition: Expression,
    /// Then branch
    pub then_branch: Block,
    /// Else branch (optional)
    pub else_branch: Option<Block>,
}

/// While loop.
#[derive(Debug, Clone)]
pub struct WhileStatement {
    /// Span of the while statement
    pub span: Span,
    /// Condition
    pub condition: Expression,
    /// Loop body
    pub body: Block,
}

/// Expressions.
#[derive(Debug, Clone)]
pub enum Expression {
    /// Identifier (variable reference)
    Identifier(Identifier),
    /// Integer literal
    IntegerLiteral(IntegerLiteral),
    /// Float literal
    FloatLiteral(FloatLiteral),
    /// String literal
    StringLiteral(StringLiteral),
    /// Boolean literal
    BooleanLiteral(BooleanLiteral),
    /// Binary operation
    Binary(BinaryExpression),
    /// Function call
    Call(CallExpression),
    /// Parenthesized expression
    Parenthesized(ParenthesizedExpression),
}

/// Identifier expression.
#[derive(Debug, Clone)]
pub struct Identifier {
    /// Span of the identifier
    pub span: Span,
    /// Identifier name
    pub name: String,
}

/// Integer literal.
#[derive(Debug, Clone)]
pub struct IntegerLiteral {
    /// Span of the literal
    pub span: Span,
    /// Integer value
    pub value: i64,
}

/// Float literal.
#[derive(Debug, Clone)]
pub struct FloatLiteral {
    /// Span of the literal
    pub span: Span,
    /// Float value
    pub value: f64,
}

/// String literal.
#[derive(Debug, Clone)]
pub struct StringLiteral {
    /// Span of the literal
    pub span: Span,
    /// String value
    pub value: String,
}

/// Boolean literal.
#[derive(Debug, Clone)]
pub struct BooleanLiteral {
    /// Span of the literal
    pub span: Span,
    /// Boolean value
    pub value: bool,
}

/// Binary expression.
#[derive(Debug, Clone)]
pub struct BinaryExpression {
    /// Span of the entire expression
    pub span: Span,
    /// Left operand
    pub left: Box<Expression>,
    /// Operator
    pub operator: BinaryOperator,
    /// Right operand
    pub right: Box<Expression>,
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    /// Addition
    Add,
    /// Subtraction
    Subtract,
    /// Multiplication
    Multiply,
    /// Division
    Divide,
    /// Equality
    Equal,
    /// Inequality
    NotEqual,
    /// Less than
    Less,
    /// Less than or equal
    LessEqual,
    /// Greater than
    Greater,
    /// Greater than or equal
    GreaterEqual,
    /// Logical AND
    And,
    /// Logical OR
    Or,
}

/// Function call expression.
#[derive(Debug, Clone)]
pub struct CallExpression {
    /// Span of the call
    pub span: Span,
    /// Function being called
    pub callee: Box<Expression>,
    /// Arguments
    pub arguments: Vec<Expression>,
}

/// Parenthesized expression.
#[derive(Debug, Clone)]
pub struct ParenthesizedExpression {
    /// Span of the parenthesized expression (including parens)
    pub span: Span,
    /// Inner expression
    pub expression: Box<Expression>,
}

/// Type reference.
#[derive(Debug, Clone)]
pub enum TypeReference {
    /// Named type (e.g., i32, string, bool)
    Named(String),
    /// Function type (parameters -> return type)
    Function {
        parameters: Vec<TypeReference>,
        return_type: Box<TypeReference>,
    },
}

impl TypeReference {
    /// Create a named type reference.
    pub fn named(name: impl Into<String>) -> Self {
        Self::Named(name.into())
    }

    /// Check if this is the void type.
    pub fn is_void(&self) -> bool {
        matches!(self, Self::Named(name) if name == "void")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_reference() {
        let int_type = TypeReference::named("i32");
        assert!(!int_type.is_void());

        let void_type = TypeReference::named("void");
        assert!(void_type.is_void());
    }

    #[test]
    fn test_program_creation() {
        let program = Program {
            declarations: vec![],
        };
        assert!(program.declarations.is_empty());
    }

    #[test]
    fn test_function_declaration() {
        let func = FunctionDeclaration {
            span: Span::new(0, 10),
            name: "main".to_string(),
            parameters: vec![],
            return_type: TypeReference::named("i32"),
            body: Block {
                span: Span::new(10, 20),
                statements: vec![],
            },
        };
        assert_eq!(func.name, "main");
    }
}
