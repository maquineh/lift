# Incremento: métodos, closures e templates v1

Contrato bilíngue detalhado e exemplos: `docs/methods-closures-templates.md` e
`docs/methods-closures-templates.pt-BR.md` na raiz. Rust/Cargo MSRV **1.94.1**,
herdado por todos os crates e declarado nos manifests do programa/core extraídos.

- AST preserva ImplDeclaration, ClosureExpression e partes de Template. Logos
  usa callback contextual recursivo para backticks/interpolações, balanceando
  chaves com tokens (strings/comentários não alteram profundidade); spans internos
  recebem deslocamentos de bytes e SourceId recursivamente.
- `impl Nominal { [function] metodo(args): Ret { ... } }` normaliza na semântica
  para funções internas `$method:Nominal:metodo`, impossíveis como identificadores
  fonte. O primeiro parâmetro é `this: Nominal`. Duplicatas/target desconhecido
  usam validação comum. Chamadas por tipo nominal inserem receiver antes de
  `resolve_call_aliasing`; codegen recebe chamadas normais com modos explícitos.
  Receiver nunca vira OwnedMove: `return this` clona; mutação usa BorrowMut.
- Inferência de modos propaga resumos de métodos até ponto fixo. Receiver que é
  parâmetro nominal direto seleciona resumo exato; para locais/projeções sem tipo
  no walker de resumos, une conservadoramente métodos com o mesmo nome. Isso pode
  exigir mutabilidade adicional, sem relaxar aliasing.
- Arrows exigem `(params tipados): Ret => expressão/bloco`. O incremento
  higher-order (seção final) permite `const`/`let`, valores de função em
  parâmetros/retornos e tipo público `(T) => R`; a arrow continua sem inferência
  de parâmetros. ResolvedType::Closure agora é aceito em parâmetro/retorno/variável
  e continua fora de campos/arrays/Option/Result. Capture discovery calcula nomes
  livres lexicalmente, respeitando parâmetros e declarações de cada arrow/bloco;
  só nomes livres transitivos de arrows aninhadas passam à arrow externa.
- HIR Closure guarda corpo tipado, parâmetros e captures `(BindingId, HirExpr)`
  com inicializadores owned explícitos; cada capture recebe ID novo. Captura é
  snapshot na construção (Copy/CloneMode), diferente do binding vivo TypeScript.
  BodyChecker separado rejeita escrita/forwarding mutável dos captures. Parâmetros
  da closure são owned, com resumos de mutação direta/transitiva antes de checar
  o corpo e flags Rust mut apropriadas. Retornos/argumentos clonam captures para
  preservar chamadas repetidas da closure `move` gerada.
- HirExprKind::ClosureCall conserva callee e argumentos owned; codegen emite
  chamada Rust à closure. Walkers de ownership/liveness observam só initializers
  dos captures no CFG externo, nunca o corpo adiado. O corpo mantém clones
  conservadores. Descoberta de dependência console também examina o corpo.
- Template aceita apenas string/int/number/boolean. Cada interpolação executa e
  converte imediatamente uma vez, em ordem, para String owned. HIR usa chamada
  reservada `$template.stringify` (um argumento ByRef) e concatenação de String;
  cada format! Rust tem formato constante. Não há referências guardadas entre
  interpolações. Formatação segue Rust Display. Escapes comuns + crase/dólar de
  identidade; Unicode escapes, tagged templates e regras JS de continuação de
  linha não são implementados. Newlines literais são preservados.
- Contratos sema cobrem mutação/receiver, captura lexical, clones/IDs, rejeições,
  aliasing e SourceId/UTF-8 aninhados. E2E `methods_closures_and_templates_compile_and_run`
  usa Cargo check e executa Rust gerado, comparando stdout do exemplo combinado.
  Não é garantia de compatibilidade TypeScript integral, callbacks ou borrow
  inference geral. Alterações anteriores de Option/liveness permanecem ativas.

## Incremento higher-order/callbacks v2

Contrato público: `docs/higher-order*.md`; exemplo `examples/higher-order.lift`;
teste `crates/lift_sema/tests/higher_order_contract.rs`.

- Nova sintaxe de tipo `(T, U) => R` (`TypeExpressionKind::Function`), aceita em
  parâmetro, retorno, anotação de variável e parâmetro de closure. `void` só como
  resultado; parâmetro `void` rejeitado. Function types não aparecem em
  campos/arrays/Option/Result (posição Field), que exigiriam Clone/PartialEq.
- Valor de função gera Rust `::std::rc::Rc<dyn Fn(args owned) -> ret owned>`.
  Parâmetros de closure são owned (sem empréstimo), então um callback pode ser
  chamado com valor já possuído. `is_value_type` continua falso; `contains_closure`
  orienta modos. Cópias usam Clone do Rc (semântica de valor Lift); parâmetro de
  função em função Lift é ByValue; FFI também ByValue.
- HIR novo `HirExprKind::FunctionValue { name, parameters, return_type }` para
  função nomeada em posição de valor; o wrapper chama a função alvo com o modo
  Lift (reempresta `&str`/array/struct, `&mut` quando BorrowMut) e retorna owned.
  Funções com recursos na assinatura são rejeitadas como valor.
- ClosureCall aceita callee Local/Rc, closure direta, grouping e chamada imediata
  do retorno de outra chamada (`make()(3)`). O codegen chama `(callee)(args)`.
- Captures continuam snapshots somente leitura; capture de outra closure usa
  Clone do Rc. Mutar capture (binding ou campo) é rejeitado. Captures de recurso
  continuam rejeitadas.
- Liveness/ownership: callbacks não são afins; clones Rc podem virar move no
  último uso sem efeito observável. Não há equalidade/ordenação de funções.
- FFI (ver `phase5-decisions.md`): parâmetro callback vira `&dyn Fn` com payloads
  owned; retorno callback vira `Box<dyn Fn>` convertido para Rc no wrapper.
