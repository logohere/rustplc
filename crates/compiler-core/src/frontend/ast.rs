use std::collections::BTreeMap;

use super::SourceSpan;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CompilationUnit {
    pub programs: Vec<ProgramDeclaration>,
    pub functions: Vec<FunctionDeclaration>,
    pub function_blocks: Vec<FunctionBlockDeclaration>,
    pub interfaces: Vec<InterfaceDeclaration>,
    pub types: Vec<TypeDeclaration>,
    pub configurations: Vec<ConfigurationDeclaration>,
    pub global_var_blocks: Vec<VarBlock>,
}

impl CompilationUnit {
    pub fn extend(&mut self, other: CompilationUnit) {
        self.programs.extend(other.programs);
        self.functions.extend(other.functions);
        self.function_blocks.extend(other.function_blocks);
        self.interfaces.extend(other.interfaces);
        self.types.extend(other.types);
        self.configurations.extend(other.configurations);
        self.global_var_blocks.extend(other.global_var_blocks);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProgramDeclaration {
    pub name: String,
    pub var_blocks: Vec<VarBlock>,
    pub body: Vec<Statement>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDeclaration {
    pub name: String,
    pub return_type: TypeReference,
    pub var_blocks: Vec<VarBlock>,
    pub body: Vec<Statement>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionBlockDeclaration {
    pub name: String,
    pub is_abstract: bool,
    pub is_final: bool,
    pub extends: Option<String>,
    pub implements: Vec<String>,
    pub var_blocks: Vec<VarBlock>,
    pub methods: Vec<MethodDeclaration>,
    pub properties: Vec<PropertyDeclaration>,
    pub body: Vec<Statement>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default]
    Public,
    Private,
    Protected,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MethodDeclaration {
    pub name: String,
    pub visibility: Visibility,
    pub is_abstract: bool,
    pub is_final: bool,
    pub is_override: bool,
    pub return_type: Option<TypeReference>,
    pub var_blocks: Vec<VarBlock>,
    pub body: Vec<Statement>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceDeclaration {
    pub name: String,
    pub extends: Vec<String>,
    pub methods: Vec<MethodDeclaration>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropertyDeclaration {
    pub name: String,
    pub property_type: TypeReference,
    pub visibility: Visibility,
    pub getter: Option<Vec<Statement>>,
    pub setter: Option<Vec<Statement>>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarBlockType {
    Var,
    Input,
    Output,
    InOut,
    External,
    Global,
    Temp,
    Inst,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VarBlock {
    pub block_type: VarBlockType,
    pub is_constant: bool,
    pub is_retain: bool,
    pub declarations: Vec<VarDeclaration>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VarDeclaration {
    pub names: Vec<String>,
    pub variable_type: TypeReference,
    pub initial_value: Option<Expression>,
    pub address: Option<String>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceKind {
    None,
    RefTo,
    ReferenceTo,
    PointerTo,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeReference {
    pub name: String,
    pub reference_kind: ReferenceKind,
    pub max_length: Option<String>,
    pub array_dimensions: Vec<ArrayDimension>,
    pub element_type_name: Option<String>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArrayDimension {
    pub is_variable_length: bool,
    pub start: Option<Expression>,
    pub end: Option<Expression>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeDeclaration {
    pub name: String,
    pub definition: TypeDefinition,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum TypeDefinition {
    Struct(Vec<VarDeclaration>),
    Enum {
        base_type: Option<TypeReference>,
        members: Vec<EnumMember>,
        default_value: Option<String>,
    },
    Array {
        dimensions: Vec<ArrayDimension>,
        element_type: TypeReference,
    },
    Subrange {
        base_type: TypeReference,
        lower_bound: Expression,
        upper_bound: Expression,
    },
    Alias(TypeReference),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumMember {
    pub name: String,
    pub value: Option<Expression>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigurationDeclaration {
    pub name: String,
    pub var_blocks: Vec<VarBlock>,
    pub resources: Vec<ResourceDeclaration>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResourceDeclaration {
    pub name: String,
    pub on_type: String,
    pub tasks: Vec<TaskDeclaration>,
    pub program_instances: Vec<ProgramInstance>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskDeclaration {
    pub name: String,
    pub properties: BTreeMap<String, Expression>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProgramInstance {
    pub instance_name: String,
    pub task_name: Option<String>,
    pub program_type: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Statement {
    Assignment(AssignmentStatement),
    If(IfStatement),
    Case(CaseStatement),
    For(ForStatement),
    While(WhileStatement),
    Repeat(RepeatStatement),
    Exit(SourceSpan),
    Return(SourceSpan),
    FunctionCall(FunctionCallExpression),
    Delete {
        pointer: Expression,
        span: SourceSpan,
    },
    ExternalCode {
        code: String,
        span: SourceSpan,
    },
    Empty(SourceSpan),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssignmentStatement {
    pub target: Expression,
    pub value: Expression,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IfStatement {
    pub condition: Expression,
    pub then_statements: Vec<Statement>,
    pub elsif_clauses: Vec<ElsifClause>,
    pub else_statements: Vec<Statement>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ElsifClause {
    pub condition: Expression,
    pub statements: Vec<Statement>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaseStatement {
    pub selector: Expression,
    pub cases: Vec<CaseElement>,
    pub else_statements: Vec<Statement>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaseElement {
    pub labels: Vec<CaseLabel>,
    pub statements: Vec<Statement>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaseLabel {
    pub start: Expression,
    pub end: Option<Expression>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForStatement {
    pub control_variable: String,
    pub start: Expression,
    pub end: Expression,
    pub step: Option<Expression>,
    pub body: Vec<Statement>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WhileStatement {
    pub condition: Expression,
    pub body: Vec<Statement>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RepeatStatement {
    pub body: Vec<Statement>,
    pub condition: Expression,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Binary {
        operator: BinaryOperator,
        left: Box<Expression>,
        right: Box<Expression>,
        span: SourceSpan,
    },
    Unary {
        operator: UnaryOperator,
        operand: Box<Expression>,
        span: SourceSpan,
    },
    FunctionCall(FunctionCallExpression),
    New {
        allocation_type: TypeReference,
        array_size: Option<Box<Expression>>,
        span: SourceSpan,
    },
    Variable(VariableExpression),
    Literal(LiteralExpression),
    Parenthesized {
        expression: Box<Expression>,
        span: SourceSpan,
    },
    ArrayLiteral {
        elements: Vec<Expression>,
        span: SourceSpan,
    },
}

impl Expression {
    pub fn span(&self) -> &SourceSpan {
        match self {
            Self::Binary { span, .. }
            | Self::Unary { span, .. }
            | Self::New { span, .. }
            | Self::Parenthesized { span, .. }
            | Self::ArrayLiteral { span, .. } => span,
            Self::FunctionCall(call) => &call.span,
            Self::Variable(variable) => &variable.span,
            Self::Literal(literal) => &literal.span,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Mod,
    Power,
    And,
    Or,
    Xor,
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOperator {
    Not,
    Negate,
    Positive,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionCallExpression {
    pub function_name: String,
    pub arguments: Vec<Argument>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Argument {
    pub name: Option<String>,
    pub is_output: bool,
    pub value: Expression,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VariableExpression {
    pub name: String,
    pub access_chain: Vec<AccessStep>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AccessStep {
    Field(String),
    Subscript(Vec<Expression>),
    Dereference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiteralType {
    Bool,
    Int,
    Real,
    String,
    WString,
    Time,
    Date,
    TimeOfDay,
    DateAndTime,
    Null,
    Typed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiteralExpression {
    pub literal_type: LiteralType,
    pub raw_value: String,
    pub type_prefix: Option<String>,
    pub span: SourceSpan,
}
