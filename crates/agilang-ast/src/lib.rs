//! Abstract syntax tree for the bootstrap AGILANG language subset.
use agilang_source::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub functions: Vec<Function>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    pub params: Vec<Parameter>,
    pub return_type: Option<TypeRef>,
    pub body: Vec<Stmt>,
    pub span: Span,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub ty: Option<TypeRef>,
    pub span: Span,
}
#[derive(Debug, Clone, PartialEq)]
pub struct TypeRef {
    pub name: String,
    pub span: Span,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let {
        name: String,
        ty: Option<TypeRef>,
        value: Expr,
        mutable: bool,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    Expr(Expr),
}
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Identifier(String, Span),
    Integer(i64, Span),
    Float(f64, Span),
    String(String, Span),
    Bool(bool, Span),
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
        span: Span,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
}
impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Self::Identifier(_, s)
            | Self::Integer(_, s)
            | Self::Float(_, s)
            | Self::String(_, s)
            | Self::Bool(_, s) => *s,
            Self::Call { span, .. } | Self::Binary { span, .. } => *span,
        }
    }
}
