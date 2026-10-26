//! Conservative last-use moves on validated HIR, with per-body CFG liveness.
//!
//! PT-BR: Moves conservadores de último uso no HIR validado, com liveness por CFG.

use std::collections::{HashMap, HashSet};

use crate::{
    ArgPassMode, BindingId, CloneMode, HirDeclaration, HirElseBranch, HirExpr, HirExprKind, HirIf,
    HirParameter, HirProgram, HirStatement, ParameterPassMode,
};

type NodeId = usize;

#[derive(Default)]
struct Node {
    uses: HashSet<BindingId>,
    defs: HashSet<BindingId>,
    occurrences: HashMap<BindingId, usize>,
    successors: Vec<NodeId>,
    live_in: HashSet<BindingId>,
    live_out: HashSet<BindingId>,
}

struct Point {
    node: NodeId,
    shape: Shape,
}

enum Shape {
    Leaf,
    If {
        then_branch: Vec<Point>,
        else_branch: Option<ElsePoints>,
    },
    While(Vec<Point>),
}

enum ElsePoints {
    Block(Vec<Point>),
    If(Box<Point>),
}

#[derive(Default)]
struct Graph {
    nodes: Vec<Node>,
    owned: HashSet<BindingId>,
}

/// Runs after successful checking only; never changes the accepted source set.
/// PT-BR: Executa após validação completa; não altera o conjunto fonte aceito.
pub(crate) fn optimize(program: &mut HirProgram) {
    for declaration in &mut program.declarations {
        if let HirDeclaration::Function(function) = declaration {
            let mut graph = Graph::for_parameters(&function.parameters);
            let (entry, points) = graph.sequence(function.body.statements.iter(), None);
            graph.solve();
            let reachable = graph.reachable(entry);
            rewrite_sequence(&mut function.body.statements, &points, &graph, &reachable);
        }
    }

    let mut graph = Graph::default();
    let (entry, points) = graph.sequence(
        program.declarations.iter().filter_map(|declaration| {
            if let HirDeclaration::Statement(statement) = declaration {
                Some(statement)
            } else {
                None
            }
        }),
        None,
    );
    graph.solve();
    let reachable = graph.reachable(entry);
    let statements = program.declarations.iter_mut().filter_map(|declaration| {
        if let HirDeclaration::Statement(statement) = declaration {
            Some(statement)
        } else {
            None
        }
    });
    for (statement, point) in statements.zip(&points) {
        rewrite_statement(statement, point, &graph, &reachable);
    }
}

impl Graph {
    fn for_parameters(parameters: &[HirParameter]) -> Self {
        Self {
            owned: parameters
                .iter()
                .filter(|parameter| {
                    matches!(
                        parameter.pass_mode,
                        ParameterPassMode::ByValue | ParameterPassMode::OwnedMove
                    )
                })
                .map(|parameter| parameter.id)
                .collect(),
            ..Self::default()
        }
    }

    fn add(&mut self, node: Node) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(node);
        id
    }

    fn expression_node(&mut self, expression: &HirExpr) -> NodeId {
        let mut node = Node::default();
        observe(expression, &mut node);
        self.add(node)
    }

    fn sequence<'a>(
        &mut self,
        statements: impl DoubleEndedIterator<Item = &'a HirStatement>,
        next: Option<NodeId>,
    ) -> (Option<NodeId>, Vec<Point>) {
        let mut entry = next;
        let mut points = Vec::new();
        for statement in statements.rev() {
            let point = self.statement(statement, entry);
            entry = Some(point.node);
            points.push(point);
        }
        points.reverse();
        (entry, points)
    }

    fn conditional(&mut self, conditional: &HirIf, next: Option<NodeId>) -> Point {
        let node = self.expression_node(&conditional.condition);
        let mut complete = Node::default();
        observe_conditional(conditional, &mut complete);
        self.nodes[node].occurrences = complete.occurrences;
        let (then_entry, then_branch) =
            self.sequence(conditional.then_branch.statements.iter(), next);
        let (else_entry, else_branch) = match &conditional.else_branch {
            Some(HirElseBranch::Block(block)) => {
                let (entry, points) = self.sequence(block.statements.iter(), next);
                (entry, Some(ElsePoints::Block(points)))
            }
            Some(HirElseBranch::ElseIf(nested)) => {
                let point = self.conditional(nested, next);
                (Some(point.node), Some(ElsePoints::If(Box::new(point))))
            }
            None => (next, None),
        };
        self.nodes[node].successors = then_entry.into_iter().chain(else_entry).collect();
        Point {
            node,
            shape: Shape::If {
                then_branch,
                else_branch,
            },
        }
    }

    fn statement(&mut self, statement: &HirStatement, next: Option<NodeId>) -> Point {
        if let HirStatement::If(conditional) = statement {
            return self.conditional(conditional, next);
        }
        if let HirStatement::While(loop_statement) = statement {
            let node = self.expression_node(&loop_statement.condition);
            let mut complete = Node::default();
            observe_statement(statement, &mut complete);
            self.nodes[node].occurrences = complete.occurrences;
            let (body_entry, body) =
                self.sequence(loop_statement.body.statements.iter(), Some(node));
            self.nodes[node].successors = body_entry.into_iter().chain(next).collect();
            return Point {
                node,
                shape: Shape::While(body),
            };
        }
        let mut node = Node::default();
        match statement {
            HirStatement::Variable(variable) => {
                observe(&variable.initializer, &mut node);
                node.defs.insert(variable.id);
                self.owned.insert(variable.id);
            }
            HirStatement::Assignment(assignment) => {
                observe(&assignment.value, &mut node);
                if let HirExprKind::Local { id, .. } = assignment.target.kind {
                    node.defs.insert(id);
                    *node.occurrences.entry(id).or_default() += 1;
                } else {
                    observe(&assignment.target, &mut node);
                }
            }
            HirStatement::Expression(statement) => observe(&statement.expression, &mut node),
            HirStatement::Return(statement) => {
                if let Some(value) = &statement.value {
                    observe(value, &mut node);
                }
            }
            HirStatement::If(_) | HirStatement::While(_) => unreachable!(),
        }
        // EN: Return has no fallthrough edge, including syntactically later code.
        // PT-BR: Return não tem sucessor, inclusive para código posterior na fonte.
        if !matches!(statement, HirStatement::Return(_)) {
            node.successors.extend(next);
        }
        let mut complete = Node::default();
        observe_statement(statement, &mut complete);
        node.occurrences = complete.occurrences;
        Point {
            node: self.add(node),
            shape: Shape::Leaf,
        }
    }

    fn solve(&mut self) {
        loop {
            let mut changed = false;
            for index in (0..self.nodes.len()).rev() {
                let live_out = self.nodes[index]
                    .successors
                    .iter()
                    .flat_map(|successor| self.nodes[*successor].live_in.iter().copied())
                    .collect::<HashSet<_>>();
                let mut live_in = live_out
                    .difference(&self.nodes[index].defs)
                    .copied()
                    .collect::<HashSet<_>>();
                live_in.extend(self.nodes[index].uses.iter().copied());
                if live_in != self.nodes[index].live_in || live_out != self.nodes[index].live_out {
                    self.nodes[index].live_in = live_in;
                    self.nodes[index].live_out = live_out;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    fn reachable(&self, entry: Option<NodeId>) -> HashSet<NodeId> {
        let mut reachable = HashSet::new();
        let mut pending = entry.into_iter().collect::<Vec<_>>();
        while let Some(node) = pending.pop() {
            if reachable.insert(node) {
                pending.extend(self.nodes[node].successors.iter().copied());
            }
        }
        reachable
    }
}

// EN: Count the complete source statement, independently of its CFG uses/defs.
// PT-BR: Conta o statement inteiro, separadamente dos usos/defs de cada ponto CFG.
fn observe_statement(statement: &HirStatement, node: &mut Node) {
    match statement {
        HirStatement::Variable(variable) => {
            *node.occurrences.entry(variable.id).or_default() += 1;
            observe(&variable.initializer, node);
        }
        HirStatement::Assignment(assignment) => {
            observe(&assignment.target, node);
            observe(&assignment.value, node);
        }
        HirStatement::Expression(statement) => observe(&statement.expression, node),
        HirStatement::Return(statement) => {
            if let Some(value) = &statement.value {
                observe(value, node);
            }
        }
        HirStatement::If(conditional) => observe_conditional(conditional, node),
        HirStatement::While(statement) => {
            observe(&statement.condition, node);
            for statement in &statement.body.statements {
                observe_statement(statement, node);
            }
        }
    }
}

fn observe_conditional(conditional: &HirIf, node: &mut Node) {
    observe(&conditional.condition, node);
    for statement in &conditional.then_branch.statements {
        observe_statement(statement, node);
    }
    match &conditional.else_branch {
        Some(HirElseBranch::Block(block)) => {
            for statement in &block.statements {
                observe_statement(statement, node);
            }
        }
        Some(HirElseBranch::ElseIf(conditional)) => observe_conditional(conditional, node),
        None => {}
    }
}

fn observe(expression: &HirExpr, node: &mut Node) {
    match &expression.kind {
        HirExprKind::Closure { captures, .. } => {
            for (_, value) in captures {
                observe(value, node);
            }
        }
        HirExprKind::ClosureCall { callee, arguments } => {
            observe(callee, node);
            for argument in arguments {
                observe(argument, node);
            }
        }

        HirExprKind::OptionNone => {}
        HirExprKind::OptionSome(payload) => observe(payload, node),
        // EN: Both paths are possible; keep the RHS live conservatively.
        // PT-BR: Ambos os caminhos são possíveis; RHS conta como uso conservador.
        HirExprKind::Coalesce {
            option, fallback, ..
        } => {
            observe(option, node);
            observe(fallback, node);
        }
        HirExprKind::Local { id, .. } => {
            node.uses.insert(*id);
            *node.occurrences.entry(*id).or_default() += 1;
        }
        HirExprKind::StructLiteral { fields, .. } => {
            for field in fields {
                observe(&field.value, node);
            }
        }
        HirExprKind::ArrayLiteral(elements) => {
            for element in elements {
                observe(element, node);
            }
        }
        HirExprKind::Unary { operand, .. }
        | HirExprKind::Grouping(operand)
        | HirExprKind::Clone {
            expression: operand,
            ..
        }
        | HirExprKind::Move(operand) => observe(operand, node),
        HirExprKind::Binary { left, right, .. } => {
            observe(left, node);
            observe(right, node);
        }
        HirExprKind::FieldAccess { object, .. } => observe(object, node),
        HirExprKind::Index { object, index } => {
            observe(object, node);
            observe(index, node);
        }
        HirExprKind::Call { arguments, .. } => {
            for argument in arguments {
                observe(&argument.expression, node);
            }
        }
        HirExprKind::IntegerLiteral(_)
        | HirExprKind::NumberLiteral(_)
        | HirExprKind::StringLiteral(_)
        | HirExprKind::BooleanLiteral(_) => {}
    }
}

fn rewrite_sequence(
    statements: &mut [HirStatement],
    points: &[Point],
    graph: &Graph,
    reachable: &HashSet<NodeId>,
) {
    for (statement, point) in statements.iter_mut().zip(points) {
        rewrite_statement(statement, point, graph, reachable);
    }
}

fn rewrite_conditional(
    conditional: &mut HirIf,
    point: &Point,
    graph: &Graph,
    reachable: &HashSet<NodeId>,
) {
    let Shape::If {
        then_branch,
        else_branch,
    } = &point.shape
    else {
        unreachable!()
    };
    if reachable.contains(&point.node) {
        rewrite_expr(
            &mut conditional.condition,
            &graph.nodes[point.node],
            &graph.owned,
        );
    }
    rewrite_sequence(
        &mut conditional.then_branch.statements,
        then_branch,
        graph,
        reachable,
    );
    match (&mut conditional.else_branch, else_branch) {
        (Some(HirElseBranch::Block(block)), Some(ElsePoints::Block(points))) => {
            rewrite_sequence(&mut block.statements, points, graph, reachable)
        }
        (Some(HirElseBranch::ElseIf(conditional)), Some(ElsePoints::If(point))) => {
            rewrite_conditional(conditional, point, graph, reachable)
        }
        (None, None) => {}
        _ => unreachable!(),
    }
}

fn rewrite_statement(
    statement: &mut HirStatement,
    point: &Point,
    graph: &Graph,
    reachable: &HashSet<NodeId>,
) {
    if let HirStatement::If(conditional) = statement {
        rewrite_conditional(conditional, point, graph, reachable);
        return;
    }
    if let HirStatement::While(loop_statement) = statement {
        let Shape::While(body) = &point.shape else {
            unreachable!()
        };
        if reachable.contains(&point.node) {
            rewrite_expr(
                &mut loop_statement.condition,
                &graph.nodes[point.node],
                &graph.owned,
            );
        }
        rewrite_sequence(&mut loop_statement.body.statements, body, graph, reachable);
        return;
    }
    if !reachable.contains(&point.node) {
        return;
    }
    let node = &graph.nodes[point.node];
    match statement {
        HirStatement::Variable(variable) => {
            rewrite_expr(&mut variable.initializer, node, &graph.owned)
        }
        HirStatement::Assignment(assignment) => {
            rewrite_expr(&mut assignment.value, node, &graph.owned)
        }
        HirStatement::Expression(statement) => {
            rewrite_expr(&mut statement.expression, node, &graph.owned)
        }
        HirStatement::Return(statement) => {
            if let Some(value) = &mut statement.value {
                rewrite_expr(value, node, &graph.owned);
            }
        }
        HirStatement::If(_) | HirStatement::While(_) => unreachable!(),
    }
}

fn rewrite_expr(expression: &mut HirExpr, node: &Node, owned: &HashSet<BindingId>) {
    if let HirExprKind::Clone {
        expression: operand,
        mode: CloneMode::Value,
    } = &expression.kind
    {
        if let HirExprKind::Local { id, .. } = operand.kind {
            if owned.contains(&id)
                && node.occurrences.get(&id) == Some(&1)
                && !node.live_out.contains(&id)
            {
                expression.kind = HirExprKind::Move(operand.clone());
            }
        }
        return;
    }
    match &mut expression.kind {
        HirExprKind::StructLiteral { fields, .. } => {
            for field in fields {
                rewrite_expr(&mut field.value, node, owned);
            }
        }
        HirExprKind::ArrayLiteral(elements) => {
            for element in elements {
                rewrite_expr(element, node, owned);
            }
        }
        HirExprKind::Unary { operand, .. } | HirExprKind::Grouping(operand) => {
            rewrite_expr(operand, node, owned)
        }
        HirExprKind::Binary { left, right, .. } => {
            rewrite_expr(left, node, owned);
            rewrite_expr(right, node, owned);
        }
        HirExprKind::Call { arguments, .. } => {
            for argument in arguments {
                if argument.pass_mode != ArgPassMode::ByClone {
                    rewrite_expr(&mut argument.expression, node, owned);
                }
            }
        }
        // EN: Projections, borrowed clones, and existing moves remain untouched.
        // PT-BR: Projeções, clones emprestados e moves existentes ficam intactos.
        _ => {}
    }
}
