//! Abstract syntax tree for the bootstrap AGILANG language subset.
use agilang_source::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub module_name: Option<ModuleDecl>,
    pub imports: Vec<ImportDecl>,
    pub structs: Vec<StructDecl>,
    pub enums: Vec<EnumDecl>,
    pub aliases: Vec<TypeAliasDecl>,
    pub functions: Vec<Function>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModuleDecl {
    pub path: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportDecl {
    pub path: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    pub name: String,
    pub fields: Vec<StructField>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub name: String,
    pub ty: TypeRef,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeAliasDecl {
    pub name: String,
    pub target: TypeRef,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MatchPattern {
    EnumVariant {
        enum_name: String,
        variant: String,
        span: Span,
    },
    Wildcard {
        span: Span,
    },
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
    Assign {
        target: Expr,
        value: Expr,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    Break {
        span: Span,
    },
    Continue {
        span: Span,
    },
    If {
        condition: Expr,
        then_body: Vec<Stmt>,
        else_body: Vec<Stmt>,
        span: Span,
    },
    Match {
        subject: Expr,
        arms: Vec<MatchArm>,
        span: Span,
    },
    While {
        condition: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    ForIn {
        key_name: String,
        value_name: Option<String>,
        iterable: Expr,
        body: Vec<Stmt>,
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
    ListLiteral(Vec<Expr>, Span),
    ObjectLiteral(Vec<(String, Expr)>, Span),
    MemberAccess {
        object: Box<Expr>,
        member: String,
        span: Span,
    },
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
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
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
}
impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Self::Identifier(_, s)
            | Self::Integer(_, s)
            | Self::Float(_, s)
            | Self::String(_, s)
            | Self::Bool(_, s)
            | Self::ListLiteral(_, s)
            | Self::ObjectLiteral(_, s) => *s,
            Self::MemberAccess { span, .. }
            | Self::Index { span, .. }
            | Self::Call { span, .. }
            | Self::Binary { span, .. } => *span,
        }
    }
}
