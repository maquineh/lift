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
- Arrows exigem `(params tipados): Ret => expressão/bloco`. Só `const` direto ou
  chamada imediata. ResolvedType::Closure guarda assinatura local, sem sintaxe
  de tipo público. Não é `is_value_type`, impedindo containers/Option/retorno/FFI.
  Não há cópia, captura de outra closure ou HOF. Capture discovery calcula nomes
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
