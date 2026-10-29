use std::collections::BTreeMap;
use std::fmt;

use super::ast::*;
use super::{lex, LexMode, SourceSpan, Token, TokenKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub span: SourceSpan,
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{}:{}: {}",
            self.span.file, self.span.start_line, self.span.start_col, self.message
        )
    }
}

impl std::error::Error for ParseError {}

pub fn parse(source: &str, file: &str) -> Result<CompilationUnit, Vec<ParseError>> {
    let tokens = lex(source, file, LexMode::Source).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| ParseError {
                message: error.message,
                span: error.span,
            })
            .collect::<Vec<_>>()
    })?;
    Parser::new(tokens, file)
        .parse_compilation_unit()
        .map_err(|error| vec![error])
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
    file: String,
}

impl Parser {
    fn new(tokens: Vec<Token>, file: &str) -> Self {
        Self {
            tokens,
            index: 0,
            file: file.to_owned(),
        }
    }

    fn parse_compilation_unit(mut self) -> Result<CompilationUnit, ParseError> {
        let mut unit = CompilationUnit::default();
        while !self.eof() {
            match self.kind() {
                Some(TokenKind::Program) => unit.programs.push(self.parse_program()?),
                Some(TokenKind::Function) => unit.functions.push(self.parse_function()?),
                Some(TokenKind::FunctionBlock) => {
                    unit.function_blocks.push(self.parse_function_block()?)
                }
                Some(TokenKind::Interface) => unit.interfaces.push(self.parse_interface()?),
                Some(TokenKind::Type) => unit.types.extend(self.parse_type_block()?),
                Some(TokenKind::Configuration) => {
                    unit.configurations.push(self.parse_configuration()?)
                }
                Some(TokenKind::VarGlobal) => unit.global_var_blocks.push(self.parse_var_block()?),
                _ => {
                    return Err(self.error_current(format!(
                        "expected top-level declaration, found {}",
                        self.describe_current()
                    )))
                }
            }
        }
        Ok(unit)
    }

    fn parse_program(&mut self) -> Result<ProgramDeclaration, ParseError> {
        let start = self.expect(TokenKind::Program)?.span;
        let name = self.expect_identifier()?.image;
        let var_blocks = self.parse_var_blocks()?;
        let body = self.parse_statement_list_until(&[TokenKind::EndProgram])?;
        let end = self.expect(TokenKind::EndProgram)?.span;
        Ok(ProgramDeclaration {
            name,
            var_blocks,
            body,
            span: combine_span(&start, &end),
        })
    }

    fn parse_function(&mut self) -> Result<FunctionDeclaration, ParseError> {
        let start = self.expect(TokenKind::Function)?.span;
        let name = self.expect_identifier_or_keyword()?.image;
        self.expect(TokenKind::Colon)?;
        let return_type = self.parse_type_reference()?;
        let var_blocks = self.parse_var_blocks()?;
        let body = self.parse_statement_list_until(&[TokenKind::EndFunction])?;
        let end = self.expect(TokenKind::EndFunction)?.span;
        Ok(FunctionDeclaration {
            name,
            return_type,
            var_blocks,
            body,
            span: combine_span(&start, &end),
        })
    }

    fn parse_function_block(&mut self) -> Result<FunctionBlockDeclaration, ParseError> {
        let start = self.expect(TokenKind::FunctionBlock)?.span;
        let is_abstract = self.consume(TokenKind::Abstract).is_some();
        let is_final = self.consume(TokenKind::Final).is_some();
        let name = self.expect_identifier_or_keyword()?.image;
        let extends = if self.consume(TokenKind::Extends).is_some() {
            Some(self.expect_identifier()?.image)
        } else {
            None
        };
        let mut implements = Vec::new();
        if self.consume(TokenKind::Implements).is_some() {
            implements.push(self.expect_identifier()?.image);
            while self.consume(TokenKind::Comma).is_some() {
                implements.push(self.expect_identifier()?.image);
            }
        }

        let mut var_blocks = Vec::new();
        let mut methods = Vec::new();
        let mut properties = Vec::new();
        loop {
            match self.kind() {
                Some(kind) if is_var_block_start(kind) => var_blocks.push(self.parse_var_block()?),
                Some(TokenKind::Method) => methods.push(self.parse_method()?),
                Some(TokenKind::Property) => properties.push(self.parse_property()?),
                _ => break,
            }
        }
        let body = self.parse_statement_list_until(&[TokenKind::EndFunctionBlock])?;
        let end = self.expect(TokenKind::EndFunctionBlock)?.span;
        Ok(FunctionBlockDeclaration {
            name,
            is_abstract,
            is_final,
            extends,
            implements,
            var_blocks,
            methods,
            properties,
            body,
            span: combine_span(&start, &end),
        })
    }

    fn parse_interface(&mut self) -> Result<InterfaceDeclaration, ParseError> {
        let start = self.expect(TokenKind::Interface)?.span;
        let name = self.expect_identifier()?.image;
        let mut extends = Vec::new();
        if self.consume(TokenKind::Extends).is_some() {
            extends.push(self.expect_identifier()?.image);
            while self.consume(TokenKind::Comma).is_some() {
                extends.push(self.expect_identifier()?.image);
            }
        }
        let mut methods = Vec::new();
        while self.check(TokenKind::Method) {
            methods.push(self.parse_interface_method()?);
        }
        let end = self.expect(TokenKind::EndInterface)?.span;
        Ok(InterfaceDeclaration {
            name,
            extends,
            methods,
            span: combine_span(&start, &end),
        })
    }

    fn parse_interface_method(&mut self) -> Result<MethodDeclaration, ParseError> {
        let start = self.expect(TokenKind::Method)?.span;
        let name = self.expect_identifier()?.image;
        let return_type = if self.consume(TokenKind::Colon).is_some() {
            Some(self.parse_type_reference()?)
        } else {
            None
        };
        let mut var_blocks = Vec::new();
        while self.kind().is_some_and(is_method_var_block_start) {
            var_blocks.push(self.parse_var_block()?);
        }
        let end = self.expect(TokenKind::EndMethod)?.span;
        Ok(MethodDeclaration {
            name,
            visibility: Visibility::Public,
            is_abstract: true,
            is_final: false,
            is_override: false,
            return_type,
            var_blocks,
            body: Vec::new(),
            span: combine_span(&start, &end),
        })
    }

    fn parse_method(&mut self) -> Result<MethodDeclaration, ParseError> {
        let start = self.expect(TokenKind::Method)?.span;
        let visibility = match self.kind() {
            Some(TokenKind::Public) => {
                self.advance();
                Visibility::Public
            }
            Some(TokenKind::Private) => {
                self.advance();
                Visibility::Private
            }
            Some(TokenKind::Protected) => {
                self.advance();
                Visibility::Protected
            }
            _ => Visibility::Public,
        };
        let mut is_abstract = false;
        let mut is_final = false;
        let mut is_override = false;
        loop {
            match self.kind() {
                Some(TokenKind::Abstract) => {
                    self.advance();
                    is_abstract = true;
                }
                Some(TokenKind::Final) => {
                    self.advance();
                    is_final = true;
                }
                Some(TokenKind::Override) => {
                    self.advance();
                    is_override = true;
                }
                _ => break,
            }
        }
        let name = self.expect_identifier()?.image;
        let return_type = if self.consume(TokenKind::Colon).is_some() {
            Some(self.parse_type_reference()?)
        } else {
            None
        };
        let mut var_blocks = Vec::new();
        while self.kind().is_some_and(is_method_var_block_start) {
            var_blocks.push(self.parse_var_block()?);
        }
        let body = self.parse_statement_list_until(&[TokenKind::EndMethod])?;
        let end = self.expect(TokenKind::EndMethod)?.span;
        Ok(MethodDeclaration {
            name,
            visibility,
            is_abstract,
            is_final,
            is_override,
            return_type,
            var_blocks,
            body,
            span: combine_span(&start, &end),
        })
    }

    fn parse_property(&mut self) -> Result<PropertyDeclaration, ParseError> {
        let start = self.expect(TokenKind::Property)?.span;
        let visibility = match self.kind() {
            Some(TokenKind::Public) => {
                self.advance();
                Visibility::Public
            }
            Some(TokenKind::Private) => {
                self.advance();
                Visibility::Private
            }
            Some(TokenKind::Protected) => {
                self.advance();
                Visibility::Protected
            }
            _ => Visibility::Public,
        };
        let name = self.expect_identifier()?.image;
        self.expect(TokenKind::Colon)?;
        let property_type = self.parse_type_reference()?;
        let mut getter = None;
        let mut setter = None;
        loop {
            match self.kind() {
                Some(TokenKind::Get) => {
                    self.advance();
                    getter = Some(self.parse_statement_list_until(&[TokenKind::EndGet])?);
                    self.expect(TokenKind::EndGet)?;
                }
                Some(TokenKind::Set) => {
                    self.advance();
                    setter = Some(self.parse_statement_list_until(&[TokenKind::EndSet])?);
                    self.expect(TokenKind::EndSet)?;
                }
                _ => break,
            }
        }
        let end = self.expect(TokenKind::EndProperty)?.span;
        Ok(PropertyDeclaration {
            name,
            property_type,
            visibility,
            getter,
            setter,
            span: combine_span(&start, &end),
        })
    }

    fn parse_var_blocks(&mut self) -> Result<Vec<VarBlock>, ParseError> {
        let mut blocks = Vec::new();
        while self.kind().is_some_and(is_var_block_start) {
            blocks.push(self.parse_var_block()?);
        }
        Ok(blocks)
    }

    fn parse_var_block(&mut self) -> Result<VarBlock, ParseError> {
        let opening = self
            .advance()
            .ok_or_else(|| self.error_eof("expected VAR block"))?;
        let block_type = match opening.kind {
            TokenKind::Var => VarBlockType::Var,
            TokenKind::VarInput => VarBlockType::Input,
            TokenKind::VarOutput => VarBlockType::Output,
            TokenKind::VarInOut => VarBlockType::InOut,
            TokenKind::VarExternal => VarBlockType::External,
            TokenKind::VarGlobal => VarBlockType::Global,
            TokenKind::VarTemp => VarBlockType::Temp,
            TokenKind::VarInst => VarBlockType::Inst,
            _ => return Err(self.error_at(&opening, "expected VAR block")),
        };
        let is_constant = self.consume(TokenKind::Constant).is_some();
        let is_retain = if is_constant {
            false
        } else {
            self.consume(TokenKind::Retain).is_some()
        };
        let mut declarations = Vec::new();
        while !self.check(TokenKind::EndVar) {
            declarations.push(self.parse_var_declaration()?);
        }
        let end = self.expect(TokenKind::EndVar)?.span;
        Ok(VarBlock {
            block_type,
            is_constant,
            is_retain,
            declarations,
            span: combine_span(&opening.span, &end),
        })
    }

    fn parse_var_declaration(&mut self) -> Result<VarDeclaration, ParseError> {
        let first = self.expect_identifier_or_keyword()?;
        let start = first.span.clone();
        let mut names = vec![first.image];
        while self.consume(TokenKind::Comma).is_some() {
            names.push(self.expect_identifier_or_keyword()?.image);
        }

        let mut address = None;
        if self.consume(TokenKind::At).is_some() {
            address = Some(self.expect(TokenKind::DirectAddress)?.image);
        }
        self.expect(TokenKind::Colon)?;

        let pointer_prefix = if self.consume(TokenKind::Pointer).is_some() {
            self.expect(TokenKind::To)?;
            true
        } else {
            false
        };
        let mut variable_type = if self.check(TokenKind::Array) {
            self.parse_inline_array_type()?
        } else {
            self.parse_type_reference()?
        };
        if pointer_prefix {
            variable_type.reference_kind = ReferenceKind::PointerTo;
        }

        if self.consume(TokenKind::At).is_some() {
            address = Some(self.expect(TokenKind::DirectAddress)?.image);
        }

        let initial_value = if self.consume(TokenKind::Assign).is_some() {
            let first = self.parse_expression()?;
            if self.check(TokenKind::Comma) {
                let expr_start = first.span().clone();
                let mut elements = vec![first];
                while self.consume(TokenKind::Comma).is_some() {
                    elements.push(self.parse_expression()?);
                }
                let expr_end = elements.last().expect("nonempty").span().clone();
                Some(Expression::ArrayLiteral {
                    elements,
                    span: combine_span(&expr_start, &expr_end),
                })
            } else {
                Some(first)
            }
        } else {
            None
        };
        let end = self.expect(TokenKind::Semicolon)?.span;
        Ok(VarDeclaration {
            names,
            variable_type,
            initial_value,
            address,
            span: combine_span(&start, &end),
        })
    }

    fn parse_inline_array_type(&mut self) -> Result<TypeReference, ParseError> {
        let start = self.expect(TokenKind::Array)?.span;
        self.expect(TokenKind::LBracket)?;
        let mut dimensions = vec![self.parse_array_dimension()?];
        while self.consume(TokenKind::Comma).is_some() {
            dimensions.push(self.parse_array_dimension()?);
        }
        self.expect(TokenKind::RBracket)?;
        self.expect(TokenKind::Of)?;
        let element = self.parse_type_reference()?;
        let span = combine_span(&start, &element.span);
        Ok(TypeReference {
            name: format!("__INLINE_ARRAY_{}", element.name),
            reference_kind: ReferenceKind::None,
            max_length: None,
            array_dimensions: dimensions,
            element_type_name: Some(element.name),
            span,
        })
    }

    fn parse_type_reference(&mut self) -> Result<TypeReference, ParseError> {
        let prefix = match self.kind() {
            Some(TokenKind::RefTo) => {
                self.advance();
                ReferenceKind::RefTo
            }
            Some(TokenKind::ReferenceTo) => {
                self.advance();
                ReferenceKind::ReferenceTo
            }
            Some(TokenKind::Pointer) => {
                self.advance();
                self.expect(TokenKind::To)?;
                ReferenceKind::PointerTo
            }
            _ => ReferenceKind::None,
        };
        let name_token = self.expect_identifier_or_keyword()?;
        let start = name_token.span.clone();
        let mut max_length = None;
        let mut end = name_token.span.clone();
        if matches!(
            name_token.image.to_ascii_uppercase().as_str(),
            "STRING" | "WSTRING"
        ) && self.consume(TokenKind::LParen).is_some()
        {
            let length = match self.kind() {
                Some(TokenKind::IntegerLiteral) | Some(TokenKind::Identifier) => {
                    self.advance().expect("checked")
                }
                _ => {
                    return Err(
                        self.error_current("expected STRING/WSTRING length integer or identifier")
                    )
                }
            };
            max_length = Some(length.image);
            end = self.expect(TokenKind::RParen)?.span;
        }
        Ok(TypeReference {
            name: name_token.image,
            reference_kind: prefix,
            max_length,
            array_dimensions: Vec::new(),
            element_type_name: None,
            span: combine_span(&start, &end),
        })
    }

    fn parse_type_block(&mut self) -> Result<Vec<TypeDeclaration>, ParseError> {
        self.expect(TokenKind::Type)?;
        let mut types = Vec::new();
        while !self.check(TokenKind::EndType) {
            let name = self.expect_identifier()?;
            let start = name.span.clone();
            self.expect(TokenKind::Colon)?;
            let definition = if self.check(TokenKind::Struct) {
                self.parse_struct_definition()?
            } else if self.check(TokenKind::LParen) {
                self.parse_simple_enum_definition()?
            } else if self.check(TokenKind::Array) {
                let array = self.parse_inline_array_type()?;
                TypeDefinition::Array {
                    dimensions: array.array_dimensions,
                    element_type: TypeReference {
                        name: array.element_type_name.unwrap_or_default(),
                        reference_kind: ReferenceKind::None,
                        max_length: None,
                        array_dimensions: Vec::new(),
                        element_type_name: None,
                        span: array.span,
                    },
                }
            } else {
                self.parse_typed_enum_subrange_or_alias()?
            };
            let end = self
                .consume(TokenKind::Semicolon)
                .map(|token| token.span)
                .unwrap_or_else(|| self.previous_span().unwrap_or_else(|| start.clone()));
            types.push(TypeDeclaration {
                name: name.image,
                definition,
                span: combine_span(&start, &end),
            });
        }
        self.expect(TokenKind::EndType)?;
        Ok(types)
    }

    fn parse_struct_definition(&mut self) -> Result<TypeDefinition, ParseError> {
        self.expect(TokenKind::Struct)?;
        let mut fields = Vec::new();
        while !self.check(TokenKind::EndStruct) {
            fields.push(self.parse_var_declaration()?);
        }
        self.expect(TokenKind::EndStruct)?;
        Ok(TypeDefinition::Struct(fields))
    }

    fn parse_simple_enum_definition(&mut self) -> Result<TypeDefinition, ParseError> {
        self.expect(TokenKind::LParen)?;
        let mut members = vec![self.parse_enum_member()?];
        while self.consume(TokenKind::Comma).is_some() {
            members.push(self.parse_enum_member()?);
        }
        self.expect(TokenKind::RParen)?;
        let default_value = if self.consume(TokenKind::Assign).is_some() {
            Some(self.expect_identifier()?.image)
        } else {
            None
        };
        Ok(TypeDefinition::Enum {
            base_type: None,
            members,
            default_value,
        })
    }

    fn parse_typed_enum_subrange_or_alias(&mut self) -> Result<TypeDefinition, ParseError> {
        let base_type = self.parse_type_reference()?;
        if !self.check(TokenKind::LParen) {
            return Ok(TypeDefinition::Alias(base_type));
        }

        if self.subrange_ahead() {
            self.expect(TokenKind::LParen)?;
            let lower_bound = self.parse_expression()?;
            self.expect(TokenKind::DoubleDot)?;
            let upper_bound = self.parse_expression()?;
            self.expect(TokenKind::RParen)?;
            return Ok(TypeDefinition::Subrange {
                base_type,
                lower_bound,
                upper_bound,
            });
        }

        self.expect(TokenKind::LParen)?;
        let mut members = vec![self.parse_enum_member()?];
        while self.consume(TokenKind::Comma).is_some() {
            members.push(self.parse_enum_member()?);
        }
        self.expect(TokenKind::RParen)?;
        Ok(TypeDefinition::Enum {
            base_type: Some(base_type),
            members,
            default_value: None,
        })
    }

    fn subrange_ahead(&self) -> bool {
        if !self.check(TokenKind::LParen) {
            return false;
        }
        let mut nested = 0usize;
        for offset in 1..=100 {
            let Some(kind) = self.peek_kind(offset) else {
                return false;
            };
            match kind {
                TokenKind::LParen => nested += 1,
                TokenKind::RParen if nested == 0 => return false,
                TokenKind::RParen => nested = nested.saturating_sub(1),
                TokenKind::DoubleDot if nested == 0 => return true,
                TokenKind::Comma if nested == 0 => return false,
                _ => {}
            }
        }
        false
    }

    fn parse_enum_member(&mut self) -> Result<EnumMember, ParseError> {
        let name = self.expect_identifier()?;
        let start = name.span.clone();
        let value = if self.consume(TokenKind::Assign).is_some() {
            Some(self.parse_expression()?)
        } else {
            None
        };
        let end = value
            .as_ref()
            .map(|value| value.span().clone())
            .unwrap_or_else(|| start.clone());
        Ok(EnumMember {
            name: name.image,
            value,
            span: combine_span(&start, &end),
        })
    }

    fn parse_array_dimension(&mut self) -> Result<ArrayDimension, ParseError> {
        if let Some(star) = self.consume(TokenKind::Star) {
            return Ok(ArrayDimension {
                is_variable_length: true,
                start: None,
                end: None,
                span: star.span,
            });
        }
        let start_expr = self.parse_expression()?;
        let start_span = start_expr.span().clone();
        self.expect(TokenKind::DoubleDot)?;
        let end_expr = self.parse_expression()?;
        let end_span = end_expr.span().clone();
        Ok(ArrayDimension {
            is_variable_length: false,
            start: Some(start_expr),
            end: Some(end_expr),
            span: combine_span(&start_span, &end_span),
        })
    }

    fn parse_configuration(&mut self) -> Result<ConfigurationDeclaration, ParseError> {
        let start = self.expect(TokenKind::Configuration)?.span;
        let name = self.expect_identifier()?.image;
        let mut var_blocks = Vec::new();
        let mut resources = Vec::new();
        while !self.check(TokenKind::EndConfiguration) {
            if self.kind().is_some_and(is_var_block_start) {
                var_blocks.push(self.parse_var_block()?);
            } else if self.check(TokenKind::Resource) {
                resources.push(self.parse_resource()?);
            } else {
                return Err(self.error_current("expected VAR block or RESOURCE in CONFIGURATION"));
            }
        }
        let end = self.expect(TokenKind::EndConfiguration)?.span;
        Ok(ConfigurationDeclaration {
            name,
            var_blocks,
            resources,
            span: combine_span(&start, &end),
        })
    }

    fn parse_resource(&mut self) -> Result<ResourceDeclaration, ParseError> {
        let start = self.expect(TokenKind::Resource)?.span;
        let name = self.expect_identifier()?.image;
        self.expect(TokenKind::On)?;
        let on_type = self.expect_identifier_or_keyword()?.image;
        let mut tasks = Vec::new();
        let mut program_instances = Vec::new();
        while !self.check(TokenKind::EndResource) {
            match self.kind() {
                Some(TokenKind::Task) => tasks.push(self.parse_task()?),
                Some(TokenKind::Program) => program_instances.push(self.parse_program_instance()?),
                _ => return Err(self.error_current("expected TASK or PROGRAM in RESOURCE")),
            }
        }
        let end = self.expect(TokenKind::EndResource)?.span;
        Ok(ResourceDeclaration {
            name,
            on_type,
            tasks,
            program_instances,
            span: combine_span(&start, &end),
        })
    }

    fn parse_task(&mut self) -> Result<TaskDeclaration, ParseError> {
        let start = self.expect(TokenKind::Task)?.span;
        let name = self.expect_identifier()?.image;
        self.expect(TokenKind::LParen)?;
        let mut properties = BTreeMap::new();
        if !self.check(TokenKind::RParen) {
            loop {
                let key = self.expect_identifier_or_keyword()?.image;
                self.expect(TokenKind::Assign)?;
                let value = self.parse_expression()?;
                properties.insert(key, value);
                if self.consume(TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen)?;
        let end = self.expect(TokenKind::Semicolon)?.span;
        Ok(TaskDeclaration {
            name,
            properties,
            span: combine_span(&start, &end),
        })
    }

    fn parse_program_instance(&mut self) -> Result<ProgramInstance, ParseError> {
        let start = self.expect(TokenKind::Program)?.span;
        let instance_name = self.expect_identifier()?.image;
        let task_name = if self.consume(TokenKind::With).is_some() {
            Some(self.expect_identifier()?.image)
        } else {
            None
        };
        self.expect(TokenKind::Colon)?;
        let program_type = self.expect_identifier()?.image;
        let end = self.expect(TokenKind::Semicolon)?.span;
        Ok(ProgramInstance {
            instance_name,
            task_name,
            program_type,
            span: combine_span(&start, &end),
        })
    }

    fn parse_statement_list_until(
        &mut self,
        stop: &[TokenKind],
    ) -> Result<Vec<Statement>, ParseError> {
        let mut statements = Vec::new();
        while !self.eof() && !self.kind().is_some_and(|kind| stop.contains(&kind)) {
            statements.push(self.parse_statement()?);
        }
        Ok(statements)
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        match self.kind() {
            Some(TokenKind::If) => self.parse_if().map(Statement::If),
            Some(TokenKind::Case) => self.parse_case().map(Statement::Case),
            Some(TokenKind::For) => self.parse_for().map(Statement::For),
            Some(TokenKind::While) => self.parse_while().map(Statement::While),
            Some(TokenKind::Repeat) => self.parse_repeat().map(Statement::Repeat),
            Some(TokenKind::Exit) => {
                let start = self.advance().expect("checked").span;
                let end = self.expect(TokenKind::Semicolon)?.span;
                Ok(Statement::Exit(combine_span(&start, &end)))
            }
            Some(TokenKind::Return) => {
                let start = self.advance().expect("checked").span;
                let end = self.expect(TokenKind::Semicolon)?.span;
                Ok(Statement::Return(combine_span(&start, &end)))
            }
            Some(TokenKind::Delete) => self.parse_delete_statement(),
            Some(TokenKind::Super) => self.parse_super_statement(),
            Some(TokenKind::This) => self.parse_this_statement(),
            Some(TokenKind::ExternalPragma) => {
                let token = self.advance().expect("checked");
                let code = external_code(&token.image);
                Ok(Statement::ExternalCode {
                    code,
                    span: token.span,
                })
            }
            Some(TokenKind::Semicolon) => {
                let token = self.advance().expect("checked");
                Ok(Statement::Empty(token.span))
            }
            Some(kind) if is_identifier_or_keyword(kind) => self.parse_assignment_or_call(),
            _ => Err(self.error_current(format!(
                "expected statement, found {}",
                self.describe_current()
            ))),
        }
    }

    fn parse_assignment_or_call(&mut self) -> Result<Statement, ParseError> {
        let first = self.expect_identifier_or_keyword()?;
        if self.check(TokenKind::LParen) {
            let call = self.parse_call_after_name(first)?;
            self.expect(TokenKind::Semicolon)?;
            return Ok(Statement::FunctionCall(call));
        }
        if self.dotted_call_ahead() {
            let call = self.parse_dotted_call_after_object(first)?;
            self.expect(TokenKind::Semicolon)?;
            return Ok(Statement::FunctionCall(call));
        }
        let target = self.parse_variable_after_name(first)?;
        let start = target.span.clone();
        self.expect(TokenKind::Assign)?;
        let value = self.parse_expression()?;
        let end = self.expect(TokenKind::Semicolon)?.span;
        Ok(Statement::Assignment(AssignmentStatement {
            target: Expression::Variable(target),
            value,
            span: combine_span(&start, &end),
        }))
    }

    fn parse_dotted_call_after_object(
        &mut self,
        object: Token,
    ) -> Result<FunctionCallExpression, ParseError> {
        let start = object.span.clone();
        self.expect(TokenKind::Dot)?;
        let method = self.expect_identifier_or_keyword()?;
        let name = Token {
            kind: TokenKind::Identifier,
            image: format!("{}.{}", object.image, method.image),
            span: combine_span(&start, &method.span),
        };
        self.parse_call_after_name(name)
    }

    fn dotted_call_ahead(&self) -> bool {
        self.check(TokenKind::Dot)
            && self.peek_kind(1).is_some_and(is_identifier_or_keyword)
            && self.peek_kind(2) == Some(TokenKind::LParen)
    }

    fn parse_delete_statement(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect(TokenKind::Delete)?.span;
        self.expect(TokenKind::LParen)?;
        let pointer = self.parse_expression()?;
        self.expect(TokenKind::RParen)?;
        let end = self.expect(TokenKind::Semicolon)?.span;
        Ok(Statement::Delete {
            pointer,
            span: combine_span(&start, &end),
        })
    }

    fn parse_super_statement(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect(TokenKind::Super)?;
        self.expect(TokenKind::Caret)?;
        if self.check(TokenKind::LParen) {
            let call = self.parse_call_after_name(Token {
                kind: TokenKind::Identifier,
                image: "SUPER".to_owned(),
                span: start.span,
            })?;
            self.expect(TokenKind::Semicolon)?;
            return Ok(Statement::FunctionCall(call));
        }
        self.expect(TokenKind::Dot)?;
        let method = self.expect_identifier_or_keyword()?;
        let call = self.parse_call_after_name(Token {
            kind: TokenKind::Identifier,
            image: format!("SUPER.{}", method.image),
            span: combine_span(&start.span, &method.span),
        })?;
        self.expect(TokenKind::Semicolon)?;
        Ok(Statement::FunctionCall(call))
    }

    fn parse_this_statement(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect(TokenKind::This)?;
        if self.consume(TokenKind::Caret).is_some() && self.check(TokenKind::Semicolon) {
            let end = self.expect(TokenKind::Semicolon)?.span;
            return Ok(Statement::Empty(combine_span(&start.span, &end)));
        }
        if self.check(TokenKind::Dot) && self.peek_kind(2) == Some(TokenKind::LParen) {
            let call = self.parse_dotted_call_after_object(Token {
                kind: TokenKind::Identifier,
                image: "THIS".to_owned(),
                span: start.span,
            })?;
            self.expect(TokenKind::Semicolon)?;
            return Ok(Statement::FunctionCall(call));
        }
        let target = self.parse_variable_after_name(Token {
            kind: TokenKind::Identifier,
            image: "THIS".to_owned(),
            span: start.span,
        })?;
        let assignment_start = target.span.clone();
        self.expect(TokenKind::Assign)?;
        let value = self.parse_expression()?;
        let end = self.expect(TokenKind::Semicolon)?.span;
        Ok(Statement::Assignment(AssignmentStatement {
            target: Expression::Variable(target),
            value,
            span: combine_span(&assignment_start, &end),
        }))
    }

    fn parse_if(&mut self) -> Result<IfStatement, ParseError> {
        let start = self.expect(TokenKind::If)?.span;
        let condition = self.parse_expression()?;
        self.expect(TokenKind::Then)?;
        let then_statements = self.parse_statement_list_until(&[
            TokenKind::Elsif,
            TokenKind::Else,
            TokenKind::EndIf,
        ])?;
        let mut elsif_clauses = Vec::new();
        while let Some(elsif) = self.consume(TokenKind::Elsif) {
            let condition = self.parse_expression()?;
            self.expect(TokenKind::Then)?;
            let statements = self.parse_statement_list_until(&[
                TokenKind::Elsif,
                TokenKind::Else,
                TokenKind::EndIf,
            ])?;
            let end = statements
                .last()
                .map(statement_span)
                .cloned()
                .unwrap_or_else(|| condition.span().clone());
            elsif_clauses.push(ElsifClause {
                condition,
                statements,
                span: combine_span(&elsif.span, &end),
            });
        }
        let else_statements = if self.consume(TokenKind::Else).is_some() {
            self.parse_statement_list_until(&[TokenKind::EndIf])?
        } else {
            Vec::new()
        };
        let mut end = self.expect(TokenKind::EndIf)?.span;
        if let Some(semicolon) = self.consume(TokenKind::Semicolon) {
            end = semicolon.span;
        }
        Ok(IfStatement {
            condition,
            then_statements,
            elsif_clauses,
            else_statements,
            span: combine_span(&start, &end),
        })
    }

    fn parse_case(&mut self) -> Result<CaseStatement, ParseError> {
        let start = self.expect(TokenKind::Case)?.span;
        let selector = self.parse_expression()?;
        self.expect(TokenKind::Of)?;
        let mut cases = Vec::new();
        while !self.eof()
            && !self.check(TokenKind::Else)
            && !self.check(TokenKind::EndCase)
            && self.is_case_label_start()
        {
            cases.push(self.parse_case_element()?);
        }
        let else_statements = if self.consume(TokenKind::Else).is_some() {
            self.parse_statement_list_until(&[TokenKind::EndCase])?
        } else {
            Vec::new()
        };
        let mut end = self.expect(TokenKind::EndCase)?.span;
        if let Some(semicolon) = self.consume(TokenKind::Semicolon) {
            end = semicolon.span;
        }
        Ok(CaseStatement {
            selector,
            cases,
            else_statements,
            span: combine_span(&start, &end),
        })
    }

    fn parse_case_element(&mut self) -> Result<CaseElement, ParseError> {
        let first = self.parse_case_label()?;
        let start = first.span.clone();
        let mut labels = vec![first];
        while self.consume(TokenKind::Comma).is_some() {
            labels.push(self.parse_case_label()?);
        }
        self.expect(TokenKind::Colon)?;
        let mut statements = Vec::new();
        while !self.eof()
            && !self.check(TokenKind::Else)
            && !self.check(TokenKind::EndCase)
            && !self.is_case_label_start()
        {
            statements.push(self.parse_statement()?);
        }
        let end = statements
            .last()
            .map(statement_span)
            .cloned()
            .unwrap_or_else(|| labels.last().expect("nonempty").span.clone());
        Ok(CaseElement {
            labels,
            statements,
            span: combine_span(&start, &end),
        })
    }

    fn parse_case_label(&mut self) -> Result<CaseLabel, ParseError> {
        let start_expr = self.parse_expression()?;
        let start_span = start_expr.span().clone();
        let end_expr = if self.consume(TokenKind::DoubleDot).is_some() {
            Some(self.parse_expression()?)
        } else {
            None
        };
        let end_span = end_expr
            .as_ref()
            .map(|expression| expression.span().clone())
            .unwrap_or_else(|| start_span.clone());
        Ok(CaseLabel {
            start: start_expr,
            end: end_expr,
            span: combine_span(&start_span, &end_span),
        })
    }

    fn parse_for(&mut self) -> Result<ForStatement, ParseError> {
        let start = self.expect(TokenKind::For)?.span;
        let control_variable = self.expect_identifier()?.image;
        self.expect(TokenKind::Assign)?;
        let start_expr = self.parse_expression()?;
        self.expect(TokenKind::To)?;
        let end_expr = self.parse_expression()?;
        let step = if self.consume(TokenKind::By).is_some() {
            Some(self.parse_expression()?)
        } else {
            None
        };
        self.expect(TokenKind::Do)?;
        let body = self.parse_statement_list_until(&[TokenKind::EndFor])?;
        let mut end = self.expect(TokenKind::EndFor)?.span;
        if let Some(semicolon) = self.consume(TokenKind::Semicolon) {
            end = semicolon.span;
        }
        Ok(ForStatement {
            control_variable,
            start: start_expr,
            end: end_expr,
            step,
            body,
            span: combine_span(&start, &end),
        })
    }

    fn parse_while(&mut self) -> Result<WhileStatement, ParseError> {
        let start = self.expect(TokenKind::While)?.span;
        let condition = self.parse_expression()?;
        self.expect(TokenKind::Do)?;
        let body = self.parse_statement_list_until(&[TokenKind::EndWhile])?;
        let mut end = self.expect(TokenKind::EndWhile)?.span;
        if let Some(semicolon) = self.consume(TokenKind::Semicolon) {
            end = semicolon.span;
        }
        Ok(WhileStatement {
            condition,
            body,
            span: combine_span(&start, &end),
        })
    }

    fn parse_repeat(&mut self) -> Result<RepeatStatement, ParseError> {
        let start = self.expect(TokenKind::Repeat)?.span;
        let body = self.parse_statement_list_until(&[TokenKind::Until])?;
        self.expect(TokenKind::Until)?;
        let condition = self.parse_expression()?;
        let mut end = self.expect(TokenKind::EndRepeat)?.span;
        if let Some(semicolon) = self.consume(TokenKind::Semicolon) {
            end = semicolon.span;
        }
        Ok(RepeatStatement {
            body,
            condition,
            span: combine_span(&start, &end),
        })
    }

    fn parse_expression(&mut self) -> Result<Expression, ParseError> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expression, ParseError> {
        let mut expression = self.parse_xor()?;
        while self.consume(TokenKind::Or).is_some() {
            let right = self.parse_xor()?;
            expression = binary(expression, BinaryOperator::Or, right);
        }
        Ok(expression)
    }

    fn parse_xor(&mut self) -> Result<Expression, ParseError> {
        let mut expression = self.parse_and()?;
        while self.consume(TokenKind::Xor).is_some() {
            let right = self.parse_and()?;
            expression = binary(expression, BinaryOperator::Xor, right);
        }
        Ok(expression)
    }

    fn parse_and(&mut self) -> Result<Expression, ParseError> {
        let mut expression = self.parse_comparison()?;
        while self.check(TokenKind::And) || self.check(TokenKind::Ampersand) {
            self.advance().expect("checked");
            let right = self.parse_comparison()?;
            expression = binary(expression, BinaryOperator::And, right);
        }
        Ok(expression)
    }

    fn parse_comparison(&mut self) -> Result<Expression, ParseError> {
        let expression = self.parse_add()?;
        let Some(operator) = comparison_operator(self.kind()) else {
            return Ok(expression);
        };
        self.advance();
        let right = self.parse_add()?;
        Ok(binary(expression, operator, right))
    }

    fn parse_add(&mut self) -> Result<Expression, ParseError> {
        let mut expression = self.parse_mul()?;
        loop {
            let operator = match self.kind() {
                Some(TokenKind::Plus) => BinaryOperator::Add,
                Some(TokenKind::Minus) => BinaryOperator::Subtract,
                _ => break,
            };
            self.advance();
            let right = self.parse_mul()?;
            expression = binary(expression, operator, right);
        }
        Ok(expression)
    }

    fn parse_mul(&mut self) -> Result<Expression, ParseError> {
        let mut expression = self.parse_power()?;
        loop {
            let operator = match self.kind() {
                Some(TokenKind::Star) => BinaryOperator::Multiply,
                Some(TokenKind::Slash) => BinaryOperator::Divide,
                Some(TokenKind::Mod) => BinaryOperator::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_power()?;
            expression = binary(expression, operator, right);
        }
        Ok(expression)
    }

    fn parse_power(&mut self) -> Result<Expression, ParseError> {
        let expression = self.parse_unary()?;
        if self.consume(TokenKind::Power).is_none() {
            return Ok(expression);
        }
        let right = self.parse_unary()?;
        Ok(binary(expression, BinaryOperator::Power, right))
    }

    fn parse_unary(&mut self) -> Result<Expression, ParseError> {
        let operator = match self.kind() {
            Some(TokenKind::Not) if self.peek_kind(1) != Some(TokenKind::LParen) => {
                Some(UnaryOperator::Not)
            }
            Some(TokenKind::Minus) => Some(UnaryOperator::Negate),
            Some(TokenKind::Plus) => Some(UnaryOperator::Positive),
            _ => None,
        };
        let Some(operator) = operator else {
            return self.parse_primary();
        };
        let start = self.advance().expect("checked").span;
        let operand = self.parse_primary()?;
        let span = combine_span(&start, operand.span());
        Ok(Expression::Unary {
            operator,
            operand: Box::new(operand),
            span,
        })
    }

    fn parse_primary(&mut self) -> Result<Expression, ParseError> {
        if self.kind().is_some_and(is_literal) {
            return self.parse_literal();
        }
        if self.check(TokenKind::New) {
            return self.parse_new_expression();
        }
        if self.check(TokenKind::This) {
            return self.parse_this_expression();
        }
        if self.check(TokenKind::Super) {
            return self.parse_super_expression();
        }
        if self.check(TokenKind::LBracket) {
            return self.parse_array_literal();
        }
        if self.check(TokenKind::LParen) {
            let start = self.advance().expect("checked").span;
            let expression = self.parse_expression()?;
            let end = self.expect(TokenKind::RParen)?.span;
            return Ok(Expression::Parenthesized {
                expression: Box::new(expression),
                span: combine_span(&start, &end),
            });
        }
        if self.kind().is_some_and(is_identifier_or_keyword) {
            let name = self.expect_identifier_or_keyword()?;
            if self.check(TokenKind::LParen) {
                return self
                    .parse_call_after_name(name)
                    .map(Expression::FunctionCall);
            }
            if self.dotted_call_ahead() {
                return self
                    .parse_dotted_call_after_object(name)
                    .map(Expression::FunctionCall);
            }
            return self
                .parse_variable_after_name(name)
                .map(Expression::Variable);
        }
        Err(self.error_current(format!(
            "expected expression, found {}",
            self.describe_current()
        )))
    }

    fn parse_new_expression(&mut self) -> Result<Expression, ParseError> {
        let start = self.expect(TokenKind::New)?.span;
        self.expect(TokenKind::LParen)?;
        let allocation_type = self.parse_type_reference()?;
        let array_size = if self.consume(TokenKind::Comma).is_some() {
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };
        let end = self.expect(TokenKind::RParen)?.span;
        Ok(Expression::New {
            allocation_type,
            array_size,
            span: combine_span(&start, &end),
        })
    }

    fn parse_this_expression(&mut self) -> Result<Expression, ParseError> {
        let this = self.expect(TokenKind::This)?;
        let this_token = Token {
            kind: TokenKind::Identifier,
            image: "THIS".to_owned(),
            span: this.span,
        };
        if self.consume(TokenKind::Caret).is_some() {
            let mut variable = self.parse_variable_after_name(this_token)?;
            variable.access_chain.insert(0, AccessStep::Dereference);
            return Ok(Expression::Variable(variable));
        }
        if self.dotted_call_ahead() {
            return self
                .parse_dotted_call_after_object(this_token)
                .map(Expression::FunctionCall);
        }
        self.parse_variable_after_name(this_token)
            .map(Expression::Variable)
    }

    fn parse_super_expression(&mut self) -> Result<Expression, ParseError> {
        let super_token = self.expect(TokenKind::Super)?;
        self.expect(TokenKind::Caret)?;
        let base = Token {
            kind: TokenKind::Identifier,
            image: "SUPER".to_owned(),
            span: super_token.span,
        };
        if self.check(TokenKind::LParen) {
            return self
                .parse_call_after_name(base)
                .map(Expression::FunctionCall);
        }
        if self.dotted_call_ahead() {
            return self
                .parse_dotted_call_after_object(base)
                .map(Expression::FunctionCall);
        }
        self.parse_variable_after_name(base)
            .map(Expression::Variable)
    }

    fn parse_call_after_name(&mut self, name: Token) -> Result<FunctionCallExpression, ParseError> {
        let start = name.span.clone();
        self.expect(TokenKind::LParen)?;
        let mut arguments = Vec::new();
        if !self.check(TokenKind::RParen) {
            loop {
                arguments.push(self.parse_argument()?);
                if self.consume(TokenKind::Comma).is_none() {
                    break;
                }
            }
        }
        let end = self.expect(TokenKind::RParen)?.span;
        Ok(FunctionCallExpression {
            function_name: name.image,
            arguments,
            span: combine_span(&start, &end),
        })
    }

    fn parse_argument(&mut self) -> Result<Argument, ParseError> {
        let start = self
            .current_span()
            .ok_or_else(|| self.error_eof("expected function argument"))?;
        let named = self.kind().is_some_and(is_identifier_or_keyword)
            && matches!(
                self.peek_kind(1),
                Some(TokenKind::Assign | TokenKind::OutputAssign)
            );
        let (name, is_output) = if named {
            let name = self.expect_identifier_or_keyword()?.image;
            let operator = self.advance().expect("lookahead checked");
            (Some(name), operator.kind == TokenKind::OutputAssign)
        } else {
            (None, false)
        };
        let value = self.parse_expression()?;
        let end = value.span().clone();
        Ok(Argument {
            name,
            is_output,
            value,
            span: combine_span(&start, &end),
        })
    }

    fn parse_variable_after_name(&mut self, name: Token) -> Result<VariableExpression, ParseError> {
        let start = name.span.clone();
        let mut access_chain = Vec::new();
        let mut end = name.span.clone();
        loop {
            if self.consume(TokenKind::LBracket).is_some() {
                let mut indices = vec![self.parse_expression()?];
                while self.consume(TokenKind::Comma).is_some() {
                    indices.push(self.parse_expression()?);
                }
                end = self.expect(TokenKind::RBracket)?.span;
                access_chain.push(AccessStep::Subscript(indices));
                continue;
            }
            if self.consume(TokenKind::Dot).is_some() {
                let field = match self.kind() {
                    Some(TokenKind::IntegerLiteral) => self.advance().expect("checked"),
                    Some(kind) if is_identifier_or_keyword(kind) => {
                        self.expect_identifier_or_keyword()?
                    }
                    _ => {
                        return Err(
                            self.error_current("expected field name or bit number after '.'")
                        )
                    }
                };
                end = field.span.clone();
                access_chain.push(AccessStep::Field(field.image));
                continue;
            }
            if let Some(caret) = self.consume(TokenKind::Caret) {
                end = caret.span;
                access_chain.push(AccessStep::Dereference);
                continue;
            }
            break;
        }
        Ok(VariableExpression {
            name: name.image,
            access_chain,
            span: combine_span(&start, &end),
        })
    }

    fn parse_literal(&mut self) -> Result<Expression, ParseError> {
        let token = self
            .advance()
            .ok_or_else(|| self.error_eof("expected literal"))?;
        let literal_type = match token.kind {
            TokenKind::True | TokenKind::False => LiteralType::Bool,
            TokenKind::IntegerLiteral => LiteralType::Int,
            TokenKind::RealLiteral => LiteralType::Real,
            TokenKind::StringLiteral => LiteralType::String,
            TokenKind::WideStringLiteral => LiteralType::WString,
            TokenKind::TimeLiteral => LiteralType::Time,
            TokenKind::DateLiteral => LiteralType::Date,
            TokenKind::TimeOfDayLiteral => LiteralType::TimeOfDay,
            TokenKind::DateTimeLiteral => LiteralType::DateAndTime,
            TokenKind::Null => LiteralType::Null,
            TokenKind::TypedLiteral => LiteralType::Typed,
            _ => return Err(self.error_at(&token, "expected literal")),
        };
        let type_prefix = (token.kind == TokenKind::TypedLiteral)
            .then(|| token.image.split('#').next().unwrap_or_default().to_owned());
        Ok(Expression::Literal(LiteralExpression {
            literal_type,
            raw_value: token.image,
            type_prefix,
            span: token.span,
        }))
    }

    fn parse_array_literal(&mut self) -> Result<Expression, ParseError> {
        let start = self.expect(TokenKind::LBracket)?.span;
        let mut elements = vec![self.parse_expression()?];
        while self.consume(TokenKind::Comma).is_some() {
            elements.push(self.parse_expression()?);
        }
        let end = self.expect(TokenKind::RBracket)?.span;
        Ok(Expression::ArrayLiteral {
            elements,
            span: combine_span(&start, &end),
        })
    }

    fn is_case_label_start(&self) -> bool {
        if !matches!(
            self.kind(),
            Some(TokenKind::IntegerLiteral | TokenKind::Minus | TokenKind::Identifier)
        ) && !self.kind().is_some_and(is_identifier_or_keyword)
        {
            return false;
        }
        for offset in 1..=8 {
            match self.peek_kind(offset) {
                Some(TokenKind::Colon) => return true,
                Some(
                    TokenKind::Assign
                    | TokenKind::Semicolon
                    | TokenKind::LParen
                    | TokenKind::EndCase
                    | TokenKind::Else,
                )
                | None => return false,
                _ => {}
            }
        }
        false
    }

    fn expect_identifier(&mut self) -> Result<Token, ParseError> {
        self.expect(TokenKind::Identifier)
    }

    fn expect_identifier_or_keyword(&mut self) -> Result<Token, ParseError> {
        match self.kind() {
            Some(kind) if is_identifier_or_keyword(kind) => Ok(self.advance().expect("checked")),
            _ => Err(self.error_current("expected identifier")),
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token, ParseError> {
        if self.check(kind) {
            return Ok(self.advance().expect("checked"));
        }
        Err(self.error_current(format!(
            "expected {kind:?}, found {}",
            self.describe_current()
        )))
    }

    fn consume(&mut self, kind: TokenKind) -> Option<Token> {
        self.check(kind).then(|| self.advance().expect("checked"))
    }

    fn check(&self, kind: TokenKind) -> bool {
        self.kind() == Some(kind)
    }

    fn kind(&self) -> Option<TokenKind> {
        self.tokens.get(self.index).map(|token| token.kind)
    }

    fn peek_kind(&self, offset: usize) -> Option<TokenKind> {
        self.tokens.get(self.index + offset).map(|token| token.kind)
    }

    fn advance(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.index).cloned()?;
        self.index += 1;
        Some(token)
    }

    fn eof(&self) -> bool {
        self.index >= self.tokens.len()
    }

    fn current_span(&self) -> Option<SourceSpan> {
        self.tokens.get(self.index).map(|token| token.span.clone())
    }

    fn previous_span(&self) -> Option<SourceSpan> {
        self.index
            .checked_sub(1)
            .and_then(|index| self.tokens.get(index))
            .map(|token| token.span.clone())
    }

    fn describe_current(&self) -> String {
        self.tokens
            .get(self.index)
            .map(|token| format!("{:?} {:?}", token.kind, token.image))
            .unwrap_or_else(|| "end of input".to_owned())
    }

    fn error_current(&self, message: impl Into<String>) -> ParseError {
        ParseError {
            message: message.into(),
            span: self
                .current_span()
                .unwrap_or_else(|| eof_span(&self.file, &self.tokens)),
        }
    }

    fn error_eof(&self, message: impl Into<String>) -> ParseError {
        ParseError {
            message: message.into(),
            span: eof_span(&self.file, &self.tokens),
        }
    }

    fn error_at(&self, token: &Token, message: impl Into<String>) -> ParseError {
        ParseError {
            message: message.into(),
            span: token.span.clone(),
        }
    }
}

fn is_var_block_start(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Var
            | TokenKind::VarInput
            | TokenKind::VarOutput
            | TokenKind::VarInOut
            | TokenKind::VarExternal
            | TokenKind::VarGlobal
            | TokenKind::VarTemp
            | TokenKind::VarInst
    )
}

fn is_method_var_block_start(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Var
            | TokenKind::VarInput
            | TokenKind::VarOutput
            | TokenKind::VarInOut
            | TokenKind::VarTemp
            | TokenKind::VarInst
    )
}

fn is_identifier_or_keyword(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Identifier
            | TokenKind::Set
            | TokenKind::Get
            | TokenKind::On
            | TokenKind::Override
            | TokenKind::Abstract
            | TokenKind::Final
            | TokenKind::And
            | TokenKind::Or
            | TokenKind::Xor
            | TokenKind::Not
            | TokenKind::Mod
    )
}

fn is_literal(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::True
            | TokenKind::False
            | TokenKind::TypedLiteral
            | TokenKind::IntegerLiteral
            | TokenKind::RealLiteral
            | TokenKind::StringLiteral
            | TokenKind::WideStringLiteral
            | TokenKind::TimeLiteral
            | TokenKind::DateLiteral
            | TokenKind::TimeOfDayLiteral
            | TokenKind::DateTimeLiteral
            | TokenKind::Null
    )
}

fn comparison_operator(kind: Option<TokenKind>) -> Option<BinaryOperator> {
    Some(match kind? {
        TokenKind::Equal => BinaryOperator::Equal,
        TokenKind::NotEqual => BinaryOperator::NotEqual,
        TokenKind::Less => BinaryOperator::Less,
        TokenKind::Greater => BinaryOperator::Greater,
        TokenKind::LessEqual => BinaryOperator::LessEqual,
        TokenKind::GreaterEqual => BinaryOperator::GreaterEqual,
        _ => return None,
    })
}

fn binary(left: Expression, operator: BinaryOperator, right: Expression) -> Expression {
    let span = combine_span(left.span(), right.span());
    Expression::Binary {
        operator,
        left: Box::new(left),
        right: Box::new(right),
        span,
    }
}

fn statement_span(statement: &Statement) -> &SourceSpan {
    match statement {
        Statement::Assignment(statement) => &statement.span,
        Statement::If(statement) => &statement.span,
        Statement::Case(statement) => &statement.span,
        Statement::For(statement) => &statement.span,
        Statement::While(statement) => &statement.span,
        Statement::Repeat(statement) => &statement.span,
        Statement::Exit(span)
        | Statement::Return(span)
        | Statement::Empty(span)
        | Statement::Delete { span, .. }
        | Statement::ExternalCode { span, .. } => span,
        Statement::FunctionCall(call) => &call.span,
    }
}

fn combine_span(start: &SourceSpan, end: &SourceSpan) -> SourceSpan {
    SourceSpan::new(
        start.file.clone(),
        start.start_line,
        end.end_line,
        start.start_col,
        end.end_col,
    )
}

fn eof_span(file: &str, tokens: &[Token]) -> SourceSpan {
    tokens
        .last()
        .map(|token| {
            SourceSpan::new(
                file,
                token.span.end_line,
                token.span.end_line,
                token.span.end_col.saturating_add(1),
                token.span.end_col.saturating_add(1),
            )
        })
        .unwrap_or_else(|| SourceSpan::new(file, 1, 1, 1, 1))
}

fn external_code(image: &str) -> String {
    let mut inner = image.strip_prefix('{').unwrap_or(image);
    inner = inner.strip_suffix('}').unwrap_or(inner);
    let trimmed = inner.trim_start();
    let code = trimmed
        .get(8..)
        .filter(|_| {
            trimmed
                .get(..8)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("external"))
        })
        .unwrap_or(trimmed);
    code.trim_start().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_editor_generated_program_and_configuration() {
        let source = r#"
PROGRAM main
  VAR
    x : INT;
    sensor AT %IX0.0 : BOOL := TRUE;
  END_VAR

  IF sensor THEN
    x := x + 1;
  ELSE
    x := 0;
  END_IF;
END_PROGRAM

CONFIGURATION Config0
  RESOURCE Res0 ON PLC
    TASK task0(INTERVAL := T#20ms,PRIORITY := 1);
    PROGRAM instance0 WITH task0 : main;
  END_RESOURCE
END_CONFIGURATION
"#;
        let unit = parse(source, "program.st").unwrap();
        assert_eq!(unit.programs.len(), 1);
        assert_eq!(unit.programs[0].name, "main");
        assert_eq!(unit.programs[0].var_blocks[0].declarations.len(), 2);
        assert_eq!(
            unit.programs[0].var_blocks[0].declarations[1]
                .address
                .as_deref(),
            Some("%IX0.0")
        );
        assert!(matches!(unit.programs[0].body[0], Statement::If(_)));
        assert_eq!(unit.configurations.len(), 1);
        assert_eq!(unit.configurations[0].resources[0].tasks[0].name, "task0");
        assert_eq!(
            unit.configurations[0].resources[0].program_instances[0].program_type,
            "main"
        );
    }

    #[test]
    fn expression_precedence_matches_strucpp_grammar() {
        let source = r#"
PROGRAM main
VAR x : BOOL; a,b,c,d : INT; END_VAR
x := a + b * c > d AND NOT(FALSE);
END_PROGRAM
"#;
        let unit = parse(source, "precedence.st").unwrap();
        let Statement::Assignment(assignment) = &unit.programs[0].body[0] else {
            panic!("assignment expected")
        };
        let Expression::Binary {
            operator: BinaryOperator::And,
            left,
            right,
            ..
        } = &assignment.value
        else {
            panic!("AND root expected: {:?}", assignment.value)
        };
        assert!(matches!(
            **left,
            Expression::Binary {
                operator: BinaryOperator::Greater,
                ..
            }
        ));
        assert!(
            matches!(**right, Expression::FunctionCall(ref call) if call.function_name.eq_ignore_ascii_case("NOT"))
        );
    }

    #[test]
    fn parses_named_input_and_output_arguments() {
        let source = r#"
PROGRAM main
VAR x,y : DINT; eno : BOOL; END_VAR
x := MOD(EN := TRUE, IN1 := y, IN2 := 60, ENO => eno);
END_PROGRAM
"#;
        let unit = parse(source, "calls.st").unwrap();
        let Statement::Assignment(assignment) = &unit.programs[0].body[0] else {
            panic!("assignment expected")
        };
        let Expression::FunctionCall(call) = &assignment.value else {
            panic!("call expected")
        };
        assert_eq!(call.arguments.len(), 4);
        assert_eq!(call.arguments[0].name.as_deref(), Some("EN"));
        assert!(!call.arguments[0].is_output);
        assert_eq!(call.arguments[3].name.as_deref(), Some("ENO"));
        assert!(call.arguments[3].is_output);
    }

    #[test]
    fn parses_openplc_data_type_shapes() {
        let source = r#"
TYPE
  Mode : (IDLE, RUNNING, ERROR) := IDLE;
  Pair : STRUCT
    left : DINT;
    right : DINT;
  END_STRUCT;
  Values : ARRAY [0..7] OF BYTE;
END_TYPE
"#;
        let unit = parse(source, "types.st").unwrap();
        assert_eq!(unit.types.len(), 3);
        assert!(matches!(
            unit.types[0].definition,
            TypeDefinition::Enum { .. }
        ));
        assert!(matches!(
            unit.types[1].definition,
            TypeDefinition::Struct(_)
        ));
        assert!(matches!(
            unit.types[2].definition,
            TypeDefinition::Array { .. }
        ));
    }

    #[test]
    fn parses_control_flow_statements() {
        let source = r#"
PROGRAM main
VAR i,x : INT; END_VAR
FOR i := 0 TO 10 BY 2 DO x := x + i; END_FOR;
WHILE x < 100 DO x := x + 1; END_WHILE;
REPEAT x := x - 1; UNTIL x = 0 END_REPEAT;
CASE x OF 0: x := 1; 1..3: x := 4; ELSE x := 5; END_CASE;
END_PROGRAM
"#;
        let unit = parse(source, "flow.st").unwrap();
        assert!(matches!(unit.programs[0].body[0], Statement::For(_)));
        assert!(matches!(unit.programs[0].body[1], Statement::While(_)));
        assert!(matches!(unit.programs[0].body[2], Statement::Repeat(_)));
        assert!(matches!(unit.programs[0].body[3], Statement::Case(_)));
    }
}
