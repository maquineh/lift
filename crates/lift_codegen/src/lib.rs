//! Deterministic Rust source and Cargo-manifest generation from validated HIR.
//!
//! Consumes explicit type, ownership, clone, and argument-evaluation decisions
//! from `lift_sema`; it does not repeat semantic analysis.
//!
//! PT-BR: Geração determinística de Rust e manifesto Cargo a partir do HIR
//! validado. Consome decisões explícitas de tipos, ownership, clones e avaliação
//! de argumentos da crate `lift_sema`, sem repetir a análise semântica.

use std::collections::{BTreeMap, HashMap};

use lift_ast::{BinaryOperator, UnaryOperator};
use lift_sema::{
    ArgPassMode, BindingId, CallTempId, CloneMode, CoalesceMode, ExprDiscardMode, HirBlock,
    HirCallArgument, HirDeclaration, HirElseBranch, HirExpr, HirExprKind, HirFunction, HirIf,
    HirImport, HirProgram, HirStatement, ParameterPassMode, ResolvedType, RustDependency,
    CONSOLE_LOG_BUILTIN, OPTION_IS_SOME_BUILTIN, OPTION_UNWRAP_BUILTIN, RESULT_IS_OK_BUILTIN,
    RESULT_UNWRAP_BUILTIN, RESULT_UNWRAP_ERR_BUILTIN,
};

/// Emite um crate Rust executável que representa o programa HIR.
///
/// Statements de topo são colocados no `main` gerado. Funções e tipos Lift
/// recebem nomes internos prefixados para não colidirem com o `main` gerado ou
/// com palavras reservadas de Rust.
pub fn emit_rust(program: &HirProgram) -> String {
    Emitter::new(program).emit_program(program)
}

/// Emite o manifest Cargo independente usado pela CLI no cache de build.
///
/// O pacote gerado declara seu próprio workspace para poder viver dentro de
/// outro workspace Cargo, como `.lift_cache` dentro de um projeto Rust.
pub fn emit_cargo_manifest(package_name: &str) -> String {
    let package_name = normalize_package_name(package_name);
    format!(
        "[package]\nname = \"{package_name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\nrust-version = \"1.94.1\"\n\n[workspace]\nexclude = [\"lift_db\", \"lift_web\"]\n\n[[bin]]\nname = \"lift_program\"\npath = \"src/main.rs\"\n"
    )
}

/// Adds validated HIR dependencies; `core_path` is a UTF-8 Cargo path for the core.
/// Relative paths resolve from the generated manifest directory.
///
/// PT-BR: Acrescenta dependências validadas no HIR; `core_path` é um caminho
/// Cargo UTF-8 para o core. Caminhos relativos usam o diretório do manifesto
/// gerado. A API anterior continua sem dependências.
pub fn emit_cargo_manifest_for_program(
    package_name: &str,
    program: &HirProgram,
    core_path: &str,
) -> String {
    let mut manifest = emit_cargo_manifest(package_name);
    let mut dependencies = BTreeMap::new();
    let mut needs_core = false;
    let mut needs_db = false;
    let mut needs_web = false;
    for declaration in &program.declarations {
        match declaration {
            HirDeclaration::Import(import) => match &import.dependency {
                RustDependency::LocalCore => needs_core = true,
                RustDependency::LocalDb => needs_db = true,
                RustDependency::LocalWeb => needs_web = true,
                RustDependency::Registry { package, version } => {
                    dependencies.insert(package, version);
                }
            },
            HirDeclaration::Function(function) => {
                needs_core |= function.body.statements.iter().any(statement_uses_console)
            }
            HirDeclaration::Statement(statement) => needs_core |= statement_uses_console(statement),
            HirDeclaration::Struct(_) => {}
        }
    }
    if needs_core || needs_db || needs_web || !dependencies.is_empty() {
        manifest.push_str("\n[dependencies]\n");
        if needs_core {
            manifest.push_str(&format!(
                "lift_core = {{ path = {} }}\n",
                toml_string(&core_path.replace('\\', "/"))
            ));
        }
        if needs_db {
            manifest.push_str("lift_db = { path = \"lift_db\" }\n");
        }
        if needs_web {
            manifest.push_str("lift_web = { path = \"lift_web\" }\n");
        }
        for (package, version) in dependencies {
            manifest.push_str(&format!("{package} = \"={version}\"\n"));
        }
    }
    manifest
}

fn toml_string(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            c if c.is_control() => {
                use std::fmt::Write as _;
                let _ = write!(output, "\\u{:04X}", c as u32);
            }
            c => output.push(c),
        }
    }
    output.push('"');
    output
}

fn statement_uses_console(statement: &HirStatement) -> bool {
    match statement {
        HirStatement::Variable(v) => expr_uses_console(&v.initializer),
        HirStatement::Assignment(a) => expr_uses_console(&a.target) || expr_uses_console(&a.value),
        HirStatement::Expression(e) => expr_uses_console(&e.expression),
        HirStatement::Return(r) => r.value.as_ref().is_some_and(expr_uses_console),
        HirStatement::While(w) => {
            expr_uses_console(&w.condition) || w.body.statements.iter().any(statement_uses_console)
        }
        HirStatement::If(i) => if_uses_console(i),
    }
}

fn if_uses_console(i: &HirIf) -> bool {
    expr_uses_console(&i.condition)
        || i.then_branch.statements.iter().any(statement_uses_console)
        || i.else_branch.as_ref().is_some_and(|branch| match branch {
            HirElseBranch::Block(block) => block.statements.iter().any(statement_uses_console),
            HirElseBranch::ElseIf(i) => if_uses_console(i),
        })
}

fn expr_uses_console(expression: &HirExpr) -> bool {
    match &expression.kind {
        HirExprKind::Closure { captures, body, .. } => {
            captures.iter().any(|(_, e)| expr_uses_console(e))
                || body.statements.iter().any(statement_uses_console)
        }
        HirExprKind::ClosureCall { callee, arguments } => {
            expr_uses_console(callee) || arguments.iter().any(expr_uses_console)
        }
        HirExprKind::OptionNone => false,
        HirExprKind::OptionSome(payload) => expr_uses_console(payload),
        HirExprKind::Coalesce {
            option, fallback, ..
        } => expr_uses_console(option) || expr_uses_console(fallback),
        HirExprKind::Call {
            function,
            arguments,
        } => {
            function == CONSOLE_LOG_BUILTIN
                || arguments.iter().any(|a| expr_uses_console(&a.expression))
        }
        HirExprKind::StructLiteral { fields, .. } => {
            fields.iter().any(|f| expr_uses_console(&f.value))
        }
        HirExprKind::ArrayLiteral(elements) => elements.iter().any(expr_uses_console),
        HirExprKind::Unary { operand, .. }
        | HirExprKind::FieldAccess {
            object: operand, ..
        }
        | HirExprKind::Grouping(operand)
        | HirExprKind::Move(operand)
        | HirExprKind::Clone {
            expression: operand,
            ..
        } => expr_uses_console(operand),
        HirExprKind::Binary { left, right, .. }
        | HirExprKind::Index {
            object: left,
            index: right,
        } => expr_uses_console(left) || expr_uses_console(right),
        HirExprKind::IntegerLiteral(_)
        | HirExprKind::NumberLiteral(_)
        | HirExprKind::StringLiteral(_)
        | HirExprKind::BooleanLiteral(_)
        | HirExprKind::Local { .. } => false,
    }
}

fn normalize_package_name(input: &str) -> String {
    let mut output = String::new();
    let mut last_separator = false;
    for character in input.chars() {
        if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
            last_separator = false;
        } else if !last_separator && (character == '-' || character == '_' || !output.is_empty()) {
            output.push('-');
            last_separator = true;
        }
    }
    while output.ends_with('-') {
        output.pop();
    }
    if output.is_empty() {
        output.push_str("lift-generated");
    } else if output.as_bytes()[0].is_ascii_digit() {
        output.insert_str(0, "lift-");
    }
    output
}

struct Emitter {
    output: String,
    indent: usize,
    function_names: HashMap<String, String>,
    struct_names: HashMap<String, String>,
    parameter_modes: HashMap<BindingId, ParameterPassMode>,
    next_temp_name: u32,
}

impl Emitter {
    fn new(program: &HirProgram) -> Self {
        let function_names = program
            .declarations
            .iter()
            .filter_map(|declaration| match declaration {
                HirDeclaration::Function(function) => Some((
                    function.name.clone(),
                    generated_name("__lift_fn_", &function.name),
                )),
                _ => None,
            })
            .collect();
        let struct_names = program
            .declarations
            .iter()
            .filter_map(|declaration| match declaration {
                HirDeclaration::Struct(structure) => Some((
                    structure.name.clone(),
                    generated_name("__lift_type_", &structure.name),
                )),
                _ => None,
            })
            .collect();

        Self {
            output: String::from(
                "#![allow(dead_code, unused_variables, unused_mut, non_camel_case_types)]\n\n",
            ),
            indent: 0,
            function_names,
            struct_names,
            parameter_modes: HashMap::new(),
            next_temp_name: 0,
        }
    }

    fn emit_program(mut self, program: &HirProgram) -> String {
        for declaration in &program.declarations {
            match declaration {
                HirDeclaration::Import(import) => self.emit_import(import),
                HirDeclaration::Struct(structure) => {
                    self.line("#[derive(Clone, Debug, PartialEq)]");
                    self.line(format!("struct {} {{", self.type_name(&structure.name)));
                    self.indent += 1;
                    for field in &structure.fields {
                        self.line(format!(
                            "{}: {},",
                            field_name(&field.name),
                            self.rust_type(&field.ty)
                        ));
                    }
                    self.indent -= 1;
                    self.line("}");
                    self.line("");
                }
                HirDeclaration::Function(function) => {
                    self.emit_function(function);
                    self.line("");
                }
                HirDeclaration::Statement(_) => {}
            }
        }

        self.line("fn main() {");
        self.indent += 1;
        for declaration in &program.declarations {
            if let HirDeclaration::Statement(statement) = declaration {
                self.emit_statement(statement);
            }
        }
        self.indent -= 1;
        self.line("}");
        self.output
    }

    fn emit_function(&mut self, function: &HirFunction) {
        for parameter in &function.parameters {
            self.parameter_modes
                .insert(parameter.id, parameter.pass_mode);
        }
        let parameters = function
            .parameters
            .iter()
            .map(|parameter| {
                let mutability = if parameter.requires_mut_binding {
                    "mut "
                } else {
                    ""
                };
                format!(
                    "{mutability}{}: {}",
                    binding_name(parameter.id),
                    self.parameter_type(parameter.pass_mode, &parameter.ty)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        self.line(format!(
            "fn {}({parameters}) -> {} {{",
            self.function_name(&function.name),
            self.rust_type(&function.return_type)
        ));
        self.indent += 1;
        self.emit_block_contents(&function.body);
        self.indent -= 1;
        self.line("}");
    }

    fn emit_import(&mut self, import: &HirImport) {
        let parameter_type = |(ty, mode): (&ResolvedType, &ParameterPassMode)| match ty {
            ResolvedType::String => "&str".to_owned(),
            ResolvedType::Array(element) => format!("&[{}]", self.rust_type(element)),
            ResolvedType::Resource(_) => self.parameter_type(*mode, ty),
            _ => self.rust_type(ty),
        };
        let types = import
            .parameters
            .iter()
            .zip(&import.parameter_modes)
            .map(parameter_type)
            .collect::<Vec<_>>();
        let parameters = types
            .iter()
            .enumerate()
            .map(|(i, ty)| format!("__lift_arg_{i}: {ty}"))
            .collect::<Vec<_>>()
            .join(", ");
        let arguments = (0..types.len())
            .map(|i| format!("__lift_arg_{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let result = self.rust_type(&import.return_type);
        self.line(format!(
            "fn {}({parameters}) -> {result} {{",
            self.function_name(&import.name)
        ));
        self.indent += 1;
        // The safe function-pointer type makes rustc verify the claimed Rust signature.
        self.line(format!(
            "let __lift_ffi: fn({}) -> {result} = ::{};",
            types.join(", "),
            import.rust_path
        ));
        self.line(format!("__lift_ffi({arguments})"));
        self.indent -= 1;
        self.line("}");
        self.line("");
    }

    fn emit_block_contents(&mut self, block: &HirBlock) {
        for statement in &block.statements {
            self.emit_statement(statement);
        }
    }

    fn emit_statement(&mut self, statement: &HirStatement) {
        match statement {
            HirStatement::Variable(variable) => {
                let mutability = if variable.requires_mut_binding {
                    "mut "
                } else {
                    ""
                };
                let initializer = self.expression(&variable.initializer);
                self.line(format!(
                    "let {mutability}{} = {initializer};",
                    binding_name(variable.id)
                ));
            }
            HirStatement::Assignment(assignment) => {
                let target = self.expression(&assignment.target);
                let value = self.expression(&assignment.value);
                self.line(format!("{target} = {value};"));
            }
            HirStatement::If(statement) => self.emit_if(statement, "if "),
            HirStatement::While(statement) => {
                let condition = self.expression(&statement.condition);
                self.line(format!("while {condition} {{"));
                self.indent += 1;
                self.emit_block_contents(&statement.body);
                self.indent -= 1;
                self.line("}");
            }
            HirStatement::Return(statement) => match &statement.value {
                Some(value) => {
                    let value = self.expression(value);
                    self.line(format!("return {value};"));
                }
                None => self.line("return;"),
            },
            HirStatement::Expression(statement) => {
                let expression = self.expression(&statement.expression);
                match statement.discard_mode {
                    ExprDiscardMode::Value => self.line(format!("{expression};")),
                    ExprDiscardMode::BorrowPlace => {
                        self.line(format!("let _ = &({expression});"));
                    }
                }
            }
        }
    }

    fn emit_if(&mut self, statement: &HirIf, prefix: &str) {
        let condition = self.expression(&statement.condition);
        self.line(format!("{prefix}{condition} {{"));
        self.indent += 1;
        self.emit_block_contents(&statement.then_branch);
        self.indent -= 1;

        match &statement.else_branch {
            None => self.line("}"),
            Some(HirElseBranch::Block(block)) => {
                self.line("} else {");
                self.indent += 1;
                self.emit_block_contents(block);
                self.indent -= 1;
                self.line("}");
            }
            Some(HirElseBranch::ElseIf(next)) => {
                self.emit_if(next, "} else if ");
            }
        }
    }

    fn expression(&mut self, expression: &HirExpr) -> String {
        match &expression.kind {
            HirExprKind::Closure { captures, parameters, body } => {
                let mut statements = Vec::new();
                for (id, value) in captures {
                    let value = self.expression(value);
                    statements.push(format!("let {} = {value};", binding_name(*id)));
                }
                let parameters = parameters.iter().map(|p| {
                    self.parameter_modes.insert(p.id, p.pass_mode);
                    format!("{}{}: {}", if p.requires_mut_binding { "mut " } else { "" }, binding_name(p.id), self.rust_type(&p.ty))
                }).collect::<Vec<_>>().join(", ");
                let ResolvedType::Closure(_, result) = &expression.ty else { unreachable!() };
                let previous = std::mem::take(&mut self.output);
                self.emit_block_contents(body);
                let body = std::mem::replace(&mut self.output, previous);
                statements.push(format!("move |{parameters}| -> {} {{\n{body}\n}}", self.rust_type(result)));
                format!("{{ {} }}", statements.join(" "))
            }
            HirExprKind::ClosureCall { callee, arguments } => {
                let callee = self.expression(callee);
                let arguments = arguments.iter().map(|a| self.expression(a)).collect::<Vec<_>>().join(", ");
                format!("({callee})({arguments})")
            }
            HirExprKind::OptionNone => {
                let ResolvedType::Optional(payload) = &expression.ty else { unreachable!("validated None must have an optional type") };
                format!("::std::option::Option::<{}>::None", self.rust_type(payload))
            }
            HirExprKind::OptionSome(payload) => format!("::std::option::Option::Some({})", self.expression(payload)),
            HirExprKind::Coalesce { option, fallback, mode } => {
                let option = self.expression(option);
                let fallback = self.expression(fallback);
                let snapshot = self.temp_name();
                let payload = self.temp_name();
                let present = match mode { CoalesceMode::Value => payload.clone(), CoalesceMode::Optional => format!("::std::option::Option::Some({payload})") };
                format!("{{ let {snapshot} = {option}; match {snapshot} {{ ::std::option::Option::Some({payload}) => {present}, ::std::option::Option::None => {fallback} }} }}")
            }
            HirExprKind::IntegerLiteral(value) => {
                if *value == i64::MIN {
                    "i64::MIN".to_owned()
                } else {
                    format!("{value}_i64")
                }
            }
            HirExprKind::NumberLiteral(value) => format_number(*value),
            HirExprKind::StringLiteral(value) => {
                format!("String::from({})", rust_string_literal(value))
            }
            HirExprKind::BooleanLiteral(value) => value.to_string(),
            HirExprKind::Local { id, .. } => binding_name(*id),
            HirExprKind::StructLiteral {
                struct_name,
                fields,
            } => {
                let fields = fields
                    .iter()
                    .map(|field| {
                        format!(
                            "{}: {}",
                            field_name(&field.name),
                            self.expression(&field.value)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{} {{ {fields} }}", self.type_name(struct_name))
            }
            HirExprKind::ArrayLiteral(elements) => {
                if elements.is_empty() {
                    match &expression.ty {
                        ResolvedType::Array(element) => {
                            format!("Vec::<{}>::new()", self.rust_type(element))
                        }
                        _ => "Vec::new()".to_owned(),
                    }
                } else {
                    let elements = elements
                        .iter()
                        .map(|element| self.expression(element))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("vec![{elements}]")
                }
            }
            HirExprKind::Unary { operator, operand } => {
                let operator = match operator {
                    UnaryOperator::Negate => "-",
                    UnaryOperator::Not => "!",
                };
                format!("({operator}{})", self.expression(operand))
            }
            HirExprKind::Binary {
                left,
                operator,
                right,
            } => {
                if *operator == BinaryOperator::Add && expression.ty == ResolvedType::String {
                    let left = self.expression(left);
                    let right = self.expression(right);
                    format!("format!(\"{{}}{{}}\", {left}, {right})")
                } else if matches!(*operator, BinaryOperator::Equal | BinaryOperator::NotEqual) {
                    let left = self.equality_operand(left);
                    let right = self.equality_operand(right);
                    format!("({left} {} {right})", binary_operator(*operator))
                } else {
                    let left = self.expression(left);
                    let right = self.expression(right);
                    format!("({left} {} {right})", binary_operator(*operator))
                }
            }
            HirExprKind::FieldAccess { object, field } => {
                format!("({}).{}", self.expression(object), field_name(field))
            }
            HirExprKind::Call {
                function,
                arguments,
            } => self.call(function, arguments),
            HirExprKind::Index { object, index } => format!(
                "({})[usize::try_from({}).expect(\"Lift array index must be non-negative and fit usize\")]",
                self.expression(object),
                self.expression(index)
            ),
            HirExprKind::Grouping(inner) => format!("({})", self.expression(inner)),
            HirExprKind::Move(inner) => self.expression(inner),
            HirExprKind::Clone { expression, mode } => {
                let expression = self.expression(expression);
                clone_expression(&expression, *mode)
            }
        }
    }

    fn call(&mut self, function: &str, arguments: &[HirCallArgument]) -> String {
        if function == "$http.serve" {
            let address = self.call_argument(&arguments[0], None);
            let limit = self.call_argument(&arguments[1], None);
            if arguments.len() == 3 {
                let pool = self.call_argument(&arguments[2], None);
                return format!("::lift_web::http::serve_with_pool({address}, {limit}, {pool})");
            }
            return format!("::lift_web::http::serve({address}, {limit})");
        }
        if function.starts_with("$http.route|") {
            let fields = function.split('|').collect::<Vec<_>>();
            let handler = self.function_name(fields[1]);
            let request = self.type_name(fields[2]);
            let method = self.call_argument(&arguments[0], None);
            let path = self.call_argument(&arguments[1], None);
            let borrow = if fields[4] == "owned" { "" } else { "&" };
            let pooled = fields.get(5).copied() == Some("pool");
            let register = if pooled {
                "register_db_route"
            } else {
                "register_route"
            };
            let closure_parameters = if pooled { "request, pool" } else { "request" };
            let pool_argument = if pooled { ", pool" } else { "" };
            let (m, p, q, b, s) = (
                field_name("method"),
                field_name("path"),
                field_name("query"),
                field_name("body"),
                field_name("status"),
            );
            return format!("::lift_web::http::{register}({method}, {path}, |{closure_parameters}| {{ let input = {request} {{ {m}: request.method, {p}: request.path, {q}: request.query, {b}: request.body }}; let output = {handler}({borrow}input{pool_argument}); ::lift_web::http::Response {{ status: output.{s}, body: output.{b} }} }})");
        }
        if function == "$template.stringify" {
            let argument = self.call_argument(&arguments[0], None);
            return format!("::std::format!(\"{{}}\", {argument})");
        }
        if matches!(function, OPTION_IS_SOME_BUILTIN | OPTION_UNWRAP_BUILTIN) {
            let argument = self.call_argument(&arguments[0], None);
            return if function == OPTION_IS_SOME_BUILTIN {
                format!("({argument}).is_some()")
            } else {
                format!("::std::clone::Clone::clone(({argument}).as_ref().expect(\"option.unwrap called on None\"))")
            };
        }
        if matches!(
            function,
            RESULT_IS_OK_BUILTIN | RESULT_UNWRAP_BUILTIN | RESULT_UNWRAP_ERR_BUILTIN
        ) {
            let argument = self.call_argument(&arguments[0], None);
            if arguments[0].expression.ty.contains_resource()
                && arguments[0].pass_mode == ArgPassMode::ByValue
            {
                return if function == RESULT_UNWRAP_BUILTIN {
                    format!(
                        "({argument}).unwrap_or_else(|_| panic!(\"result.unwrap called on Err\"))"
                    )
                } else {
                    format!("({argument}).err().expect(\"result.unwrap_err called on Ok\")")
                };
            }
            return match function {
                RESULT_IS_OK_BUILTIN => format!("({argument}).is_ok()"),
                RESULT_UNWRAP_BUILTIN => format!("::std::clone::Clone::clone(({argument}).as_ref().expect(\"result.unwrap called on Err\"))"),
                RESULT_UNWRAP_ERR_BUILTIN => format!("::std::clone::Clone::clone(({argument}).as_ref().expect_err(\"result.unwrap_err called on Ok\"))"),
                _ => unreachable!(),
            };
        }
        if function == CONSOLE_LOG_BUILTIN {
            let argument = self.call_argument(&arguments[0], None);
            return format!("::lift_core::Console::log({argument}).expect(\"console.log failed to write stdout\")");
        }
        let mut temp_names = HashMap::<CallTempId, String>::new();
        let mut staged = arguments
            .iter()
            .enumerate()
            .filter_map(|(index, argument)| {
                argument
                    .pre_evaluation_temp
                    .map(|temp_id| (temp_id, index, argument))
            })
            .collect::<Vec<_>>();
        staged.sort_by_key(|(temp_id, index, _)| (temp_id.0, *index));

        if staged.is_empty()
            && !arguments
                .iter()
                .any(|argument| argument.defer_borrow_until_call)
        {
            let args = arguments
                .iter()
                .map(|argument| self.call_argument(argument, None))
                .collect::<Vec<_>>()
                .join(", ");
            return format!("{}({args})", self.function_name(function));
        }

        let mut lines = Vec::new();
        for (temp_id, _, argument) in staged {
            let value = self.staged_argument_value(argument);
            let name = self.temp_name();
            temp_names.insert(temp_id, name.clone());
            lines.push(format!("let {name} = {value};"));
        }

        let args = arguments
            .iter()
            .map(|argument| {
                let temp = argument
                    .pre_evaluation_temp
                    .and_then(|temp_id| temp_names.get(&temp_id).map(String::as_str));
                self.call_argument(argument, temp)
            })
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("{}({args})", self.function_name(function)));

        let parent_indent = "    ".repeat(self.indent);
        let child_indent = "    ".repeat(self.indent + 1);
        let body = lines
            .into_iter()
            .map(|line| format!("{child_indent}{line}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!("{{\n{body}\n{parent_indent}}}")
    }

    fn staged_argument_value(&mut self, argument: &HirCallArgument) -> String {
        match argument.pass_mode {
            ArgPassMode::ByClone => clone_expression(
                &self.expression(&argument.expression),
                argument.clone_mode.unwrap_or(CloneMode::Value),
            ),
            ArgPassMode::ByValue | ArgPassMode::ByRef => self.expression(&argument.expression),
            ArgPassMode::ByMutRef => {
                // `lift_sema` não produz temporários para um empréstimo mutável:
                // fazê-lo mudaria o valor que a função altera.
                self.expression(&argument.expression)
            }
        }
    }

    fn call_argument(&mut self, argument: &HirCallArgument, temp: Option<&str>) -> String {
        match argument.pass_mode {
            ArgPassMode::ByValue => temp
                .map(str::to_owned)
                .unwrap_or_else(|| self.expression(&argument.expression)),
            ArgPassMode::ByClone => temp.map(str::to_owned).unwrap_or_else(|| {
                clone_expression(
                    &self.expression(&argument.expression),
                    argument.clone_mode.unwrap_or(CloneMode::Value),
                )
            }),
            ArgPassMode::ByRef => {
                if let Some(temp) = temp {
                    format!("&{temp}")
                } else {
                    let expression = self.expression(&argument.expression);
                    if let Some(id) = direct_local_id(&argument.expression) {
                        if self.parameter_modes.get(&id).is_some_and(|mode| {
                            *mode == ParameterPassMode::BorrowImm
                                || *mode == ParameterPassMode::BorrowMut
                        }) {
                            return format!("&*{}", binding_name(id));
                        }
                    }
                    format!("&({expression})")
                }
            }
            ArgPassMode::ByMutRef => {
                if let Some(temp) = temp {
                    format!("&mut {temp}")
                } else {
                    let expression = self.expression(&argument.expression);
                    if let Some(id) = direct_local_id(&argument.expression) {
                        if self.parameter_modes.get(&id) == Some(&ParameterPassMode::BorrowMut) {
                            return format!("&mut *{}", binding_name(id));
                        }
                    }
                    format!("&mut ({expression})")
                }
            }
        }
    }

    fn equality_operand(&mut self, expression: &HirExpr) -> String {
        let emitted = self.expression(expression);
        let Some(id) = direct_local_id(expression) else {
            return emitted;
        };
        let Some(mode) = self.parameter_modes.get(&id).copied() else {
            return emitted;
        };
        let is_string = expression.ty == ResolvedType::String;
        match mode {
            ParameterPassMode::BorrowImm if is_string => {
                format!("({emitted}).to_owned()")
            }
            ParameterPassMode::BorrowMut if is_string => {
                format!("(*({emitted})).clone()")
            }
            ParameterPassMode::BorrowImm | ParameterPassMode::BorrowMut => {
                format!("*({emitted})")
            }
            ParameterPassMode::ByValue | ParameterPassMode::OwnedMove => emitted,
        }
    }

    fn parameter_type(&self, mode: ParameterPassMode, ty: &ResolvedType) -> String {
        match mode {
            ParameterPassMode::ByValue | ParameterPassMode::OwnedMove => self.rust_type(ty),
            ParameterPassMode::BorrowImm if *ty == ResolvedType::String => "&str".to_owned(),
            ParameterPassMode::BorrowImm => format!("&{}", self.rust_type(ty)),
            ParameterPassMode::BorrowMut => format!("&mut {}", self.rust_type(ty)),
        }
    }

    fn rust_type(&self, ty: &ResolvedType) -> String {
        match ty {
            ResolvedType::Closure(..) => unreachable!("closure types are inferred locally"),
            ResolvedType::Int => "i64".to_owned(),
            ResolvedType::Number => "f64".to_owned(),
            ResolvedType::String => "String".to_owned(),
            ResolvedType::Boolean => "bool".to_owned(),
            ResolvedType::Void => "()".to_owned(),
            ResolvedType::Struct(name) => self.type_name(name),
            ResolvedType::DbValue => "::lift_db::api::DbValue".into(),
            ResolvedType::Resource(lift_sema::ResourceKind::DbPool) => {
                "::lift_db::api::DbPool".into()
            }
            ResolvedType::Resource(lift_sema::ResourceKind::DbTransaction) => {
                "::lift_db::api::DbTransaction".into()
            }
            ResolvedType::Resource(lift_sema::ResourceKind::DbRows) => {
                "::lift_db::api::DbRows".into()
            }
            ResolvedType::Resource(lift_sema::ResourceKind::SqliteConnection) => {
                "::lift_db::resource::SqliteConnection".into()
            }
            ResolvedType::Resource(lift_sema::ResourceKind::SqliteTransaction) => {
                "::lift_db::resource::SqliteTransaction".into()
            }
            ResolvedType::Array(element) => format!("Vec<{}>", self.rust_type(element)),
            ResolvedType::Optional(payload) => {
                format!("::std::option::Option<{}>", self.rust_type(payload))
            }
            ResolvedType::Result(ok, error) => format!(
                "::std::result::Result<{}, {}>",
                self.rust_type(ok),
                self.rust_type(error)
            ),
        }
    }

    fn function_name(&self, source_name: &str) -> String {
        self.function_names
            .get(source_name)
            .cloned()
            .unwrap_or_else(|| generated_name("__lift_fn_", source_name))
    }

    fn type_name(&self, source_name: &str) -> String {
        self.struct_names
            .get(source_name)
            .cloned()
            .unwrap_or_else(|| generated_name("__lift_type_", source_name))
    }

    fn temp_name(&mut self) -> String {
        let name = format!("__lift_tmp_{}", self.next_temp_name);
        self.next_temp_name = self.next_temp_name.saturating_add(1);
        name
    }

    fn line(&mut self, text: impl AsRef<str>) {
        self.output.push_str(&"    ".repeat(self.indent));
        self.output.push_str(text.as_ref());
        self.output.push('\n');
    }
}

fn generated_name(prefix: &str, source_name: &str) -> String {
    let mut encoded = String::with_capacity(prefix.len() + source_name.len() * 2);
    encoded.push_str(prefix);
    for byte in source_name.as_bytes() {
        use std::fmt::Write as _;
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn binding_name(id: BindingId) -> String {
    format!("__lift_binding_{}", id.0)
}

fn field_name(source_name: &str) -> String {
    generated_name("__lift_field_", source_name)
}

fn direct_local_id(expression: &HirExpr) -> Option<BindingId> {
    match &expression.kind {
        HirExprKind::Local { id, .. } => Some(*id),
        HirExprKind::Grouping(inner) => direct_local_id(inner),
        _ => None,
    }
}

fn clone_expression(expression: &str, mode: CloneMode) -> String {
    match mode {
        CloneMode::Value => format!("({expression}).clone()"),
        CloneMode::BorrowedAggregate => format!("(*({expression})).clone()"),
        CloneMode::BorrowedString => format!("({expression}).to_owned()"),
    }
}

fn rust_string_literal(source_contents: &str) -> String {
    let mut decoded = String::new();
    let mut characters = source_contents.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '\\' {
            decoded.push(character);
            continue;
        }

        match characters.next() {
            Some('n') => decoded.push('\n'),
            Some('r') => decoded.push('\r'),
            Some('t') => decoded.push('\t'),
            Some('0') => decoded.push('\0'),
            Some('\\') => decoded.push('\\'),
            Some('"') => decoded.push('"'),
            Some('\'') => decoded.push('\''),
            Some(other) => decoded.push(other),
            None => decoded.push('\\'),
        }
    }
    format!("{decoded:?}")
}

fn format_number(value: f64) -> String {
    let mut output = value.to_string();
    if !output.contains('.') && !output.contains('e') && !output.contains('E') {
        output.push_str(".0");
    }
    output.push_str("_f64");
    output
}

fn binary_operator(operator: BinaryOperator) -> &'static str {
    match operator {
        BinaryOperator::Coalesce => unreachable!("validated coalesce uses its dedicated HIR node"),
        BinaryOperator::Add => "+",
        BinaryOperator::Subtract => "-",
        BinaryOperator::Multiply => "*",
        BinaryOperator::Divide => "/",
        BinaryOperator::Remainder => "%",
        BinaryOperator::Equal => "==",
        BinaryOperator::NotEqual => "!=",
        BinaryOperator::Less => "<",
        BinaryOperator::LessEqual => "<=",
        BinaryOperator::Greater => ">",
        BinaryOperator::GreaterEqual => ">=",
        BinaryOperator::And => "&&",
        BinaryOperator::Or => "||",
    }
}
