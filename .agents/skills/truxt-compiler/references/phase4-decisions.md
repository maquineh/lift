# Result/I/O e Option v1

English / Português do Brasil: `docs/result-and-io.md` e `docs/result-and-io.pt-BR.md` na raiz do projeto.

Este incremento adiciona o tipo especial `Result<T, E>` à AST/parser/HIR/codegen,
sem generics gerais. O tipo pode aparecer em anotações, campos, arrays,
parâmetros e retornos Lift; aninhamento é aceito na linguagem. `void` é permitido
no sucesso, nunca no erro. Ciclos de tamanho direto por Result são rejeitados.

`result.is_ok(value)` empresta e retorna boolean. `result.unwrap(value)` e
`result.unwrap_err(value)` emprestam e clonam o payload para um valor owned.
Argumentos são avaliados uma vez; o Result continua disponível. Unwrap na
variante errada causa panic; não há prova de fluxo de variante. O namespace
`result` só atua quando não está sombreado.

Não há construtores Result, pattern matching, operador de propagação `?`,
`try/catch`, unions ou propagação automática. Valores Result vêm de imports ou de funções Lift que
recebem/retornam esses valores. O ownership continua conservador.

FFI admite Result somente no retorno, com sucesso primitivo/array raso/void e
erro primitivo/array raso. Result aninhado e parâmetros Result ficam fora do FFI.
O ponteiro safe do wrapper deixa rustc confirmar a assinatura exata.

`lift_core::io` retorna `Result<_, String>` em arguments/read_stdin/read_file/
write_file. Argumentos excluem argv[0] e exigem Unicode válido; stdin lê até EOF;
arquivos são UTF-8 e caminhos relativos usam o CWD do chamador. Escrita cria ou
trunca sem criar pais, não é atômica. Erros carregam operação/caminho/erro nativo.
console.log mantém panic em falha de stdout.

Testes de parser/sema/codegen e E2E validam tipos, empréstimos, extração de cópia,
sucesso void, erros tratáveis de arquivo/UTF-8, unwrap incorreto e I/O. Não provam
toda combinação possível de HIR e tipos externos.

## Option e coalescência v1

- T? gera Option<T>. void? e Option diretamente aninhado são rejeitados; T?[] é array de opcionais e T[]? é opcional de array. Ciclos diretos Node? são rejeitados antes do codegen, enquanto array fornece indireção.
- null é literal reservado e ausência única None, somente com contexto inferível; não há valor undefined. const x = null é erro. null ?? 42 infere do RHS imediato; contexto esperado não atravessa o lhs não-null de outro coalesce, então const x: int = null ?? null ?? 1 permanece rejeitado.
- option.some constrói Some com payload owned explícito, sem lift T→T?. option.is_some empresta e testa; option.unwrap empresta/clona payload e causa panic em None. Narrowing de fluxo foi acrescentado depois; ver a seção de campos opcionais/narrowing v1.
- Campo field: T? continua obrigatório; field?: T e omissão entram no incremento posterior, não no v1. Igualdade de opcional fornece contexto ao literal null.
- T? ?? T retorna T; T? ?? T? retorna T?, permitindo cadeia. ?? associa à esquerda, com precedência abaixo de ||; mistura sem parênteses com &&/|| é rejeitada. LHS é avaliada uma vez e RHS somente no caso None; false/0/string vazia em Some não ativam fallback.
- HIR OptionSome/OptionNone/Coalesce registra owned payload/snapshot e modo Value/Optional. LHS place é clonado pelas regras de valor existentes; Rust usa match owned, sem closure e com RHS somente no braço None. Codegen não infere ownership.
- Liveness observa os dois operandos conservadoramente e preserva clones dentro dos novos nós. Nenhuma regra de aliasing é relaxada.
- Option estava totalmente fora do FFI no v1: parâmetros, retornos e ocorrências em Array/Result eram rejeitados. O incremento de structs externas/Option (`phase5-decisions.md`) passou a aceitar `T?` direto em parâmetro/retorno; Option dentro de Array/Result continua fora. Não há optional chaining, pattern match, propagação automática ou undefined.
- Testes parser/sema/codegen/E2E comprovam os caminhos exercitados, incluindo contadores lazy/exactly-once, cadeias, payloads false/0/"", campos, null sem contexto, ciclo Node? e rejeição de Option aninhado/struct local no FFI. Guias bilíngues: docs/option.md e docs/option.pt-BR.md.

## Campos opcionais e narrowing v1 (incremento posterior)

Guia público: `docs/option.md` e `docs/option.pt-BR.md`. Este incremento não
introduz generics. O FFI no v1 mantinha Option fora de imports/returns; `T?` direto foi acrescentado no incremento de structs externas/Option.

- AST: `InterfaceField.optional` registra o `?` após o nome. O parser aceita
  `campo?: T` e mantém `campo: T?` como tipo opcional obrigatório. `campo?: T?`
  e `campo?: void` são rejeitados na semântica.
- HIR: `HirStructField.optional` separa omitibilidade do tipo. `campo?: T` gera
  `Option<T>`, igual a `T?`; o object literal pode omitir o campo e o HIR
  preenche `OptionNone` ao final, preservando a ordem da fonte para os demais.
  Não há lift implícito `T`→`T?`: `campo: "Ada"` continua erro.
- Narrowing: `HirStatement::Narrow { id, source, ty }` declara binding derivado
  somente leitura. Só estreitam bindings imutáveis (`const`, parâmetros e
  captures de closure); `let` fica fora. O codegen lê o payload com
  `Option::as_ref` (com `&*source` para parâmetro emprestado) e clona uma vez.
  O source nunca é movido, e o liveness trata `Narrow` como def do derivado e
  uso da origem, preservando o clone obrigatório.
- Fatos de fluxo: `option.is_some(x)`, `x != null`/`null != x` (ramo verdadeiro)
  e `x == null`/`null == x` (ramo falso), com `!`, `&&` (fatos verdadeiros) e
  `||` (fatos falsos). Apenas nome simples; projeções como `user.nome` não
  estreitam. A própria condição não é estreitada: `x != null && x > 0` rejeita o
  operando direito; use um `if` aninhado ou `option.unwrap`.
- Continuação: `if` com um braço que sempre retorna propaga os fatos opostos ao
  restante do bloco; cadeias `else if` recebem os fatos falsos e, quando há
  fatos, o braço vira `HirElseBranch::Block` com os `Narrow` antes do `if`
  interno. `while` aplica os fatos verdadeiros ao corpo e não persiste depois.
  Fatos de caminhos alternativos são interseccionados; fatos sobre bindings de
  blocos internos são descartados por não resolverem mais para o mesmo id.
- Mutação: atribuição direta ou por campo/índice com raiz estreitada e
  argumentos `ByMutRef` com raiz estreitada são rejeitados antes do Rust; um
  clone mutado seria uma alteração silenciosa de cópia. O tipo estreitado é
  somente leitura.
- Omissão de campo e narrowing aparecem em `examples/option.lift`, nos testes de
  parser/sema/codegen e no E2E da CLI; o limite de condição composta é
  documentado e testado como rejeição.
## Construção e propagação de erros v2

Guia público: `docs/errors.md` e `docs/errors.pt-BR.md`. Exemplo executado:
`examples/errors.lift`. Substitui a lacuna "Result sem construtores, `?` ou try/catch".

- Construtores `result.ok(valor)`/`result.err(erro)` exigem contexto `Result<T, E>`
  (anotação, retorno ou argumento). Sem contexto, o construtor é rejeitado antes do
  codegen, inclusive quando usado direto sob `?`. `result.ok()` sem argumento só vale
  com sucesso `void`. Namespace `result` precisa estar livre.
- HIR: `HirExprKind::ResultOk(Option<Box<HirExpr>>)`, `ResultErr`, `TryPropagate
  { expression, target }`, `HirStatement::Throw { value, target }`,
  `HirStatement::TryCatch { label: TryId, body, catch_id, catch_ty, catch_mut,
  returns, catch_body }`. `PropagateTarget` é `Function` ou `Try(TryId)`.
- `?`: operando precisa ser `Result<T, E>`; alvo é o `try` interno cujo catch tem
  exatamente `E`, senão a função com retorno `Result<_, E>` exato, senão
  `sema.InvalidPropagation`. Sem conversão `From`. Place Result clonável vira
  `Clone` (origem preservada); Result com recurso opaco vira `Move` (afim).
- `throw valor;` é statement divergente e usa a mesma resolução de alvo; exige
  `;` e tipo de valor igual ao erro do alvo.
- `try { } catch (erro: E) { }`: E explícito e tipo de valor. Blocos têm escopos
  separados; sombreamento é rejeitado. Binding do catch é owned e mutável em
  campos (`Err(mut binding)` no Rust). `return` no corpo retorna da função e
  ignora o catch; `?`/`throw` no catch miram alvo externo. Sem `finally`,
  múltiplos catches, catch sem tipo ou valor de expressão.
- Codegen: bloco rotulado `'lift_try_N` com valor `Result<(), E>`; `?`/`throw`
  no corpo usam `break 'label Err(e)`; `match` despacha o catch; quando o
  statement garante retorno, emite `unreachable!()` para fechar a divergência.
- Garantia de retorno: `block_guarantees_exit` (return/throw/if dois braços) e
  `block_guarantees_return(block, try_depth)`; um try/catch garante retorno se o
  corpo sempre retorna ou se sempre sai e o catch retorna. Narrowing usa "exit".
- Liveness conservadora: cada try/catch é um ponto único; usos dos dois blocos
  contam e clones internos não viram moves neste incremento.
- Testes: `crates/lift_sema/tests/errors_contract.rs`, parser/codegen e E2E de
  `examples/errors.lift`, além dos casos negativos de contexto, alvo, mismatch,
  sombreamento, catch void e falta de retorno.
## Match de Option/Result v3

Contrato público: `docs/match*.md`; exemplo `examples/match.lift`; teste
`crates/lift_sema/tests/match_contract.rs`.

- Sintaxe `match expressão { some(nome) => { ... } none => { ... } }` e
  `ok(nome)`/`err(nome)` (com `ok()` para sucesso `void`). Braços só em bloco,
  `=>` obrigatório, vírgula opcional. `match` é statement, não expressão.
- Lexer reserva `match`; `some`, `none`, `ok`, `err` são identificadores
  contextuais nos patterns. AST `MatchStatement`/`MatchArm`/`MatchPattern`.
- Sema exige `Option<T>`/`Result<T,E>`, exatamente `some`+`none` ou `ok`+`err`
  (sem duplicata/ausente), pattern compatível, `ok()` só para void e
  `ok(nome)` só para não-void, sem sombrear nomes ativos. Erros usam
  `sema.InvalidMatch`. Scrutinee com recurso opaco é rejeitado.
- Scrutinee é sintetizado uma vez e materializado owned (clone quando place);
  payload entra no binding como owned. Binding é `Const` (sem reatribuição),
  mas campos podem ser mutados e o pattern Rust recebe `mut`.
- Fluxo: match garante retorno se todos os braços retornam/lançam; `return`,
  `throw`, `?`, `break`/`continue` funcionam nos braços com os alvos usuais.
- HIR `HirMatch`/`HirMatchArm`/`HirMatchPattern`/`HirMatchBinding`. Liveness trata
  o match como ponto conservador (sem rewrite interno). Recursos analisam cada
  braço a partir do mesmo estado e interseccionam a disponibilidade.
- Codegen `match` com `Some(binding)`, `None`, `Ok(binding)`, `Ok(())`,
  `Err(binding)`, usando `mut` quando `requires_mut`.
- Testes: parser (patterns/rejeições), semântica (tipos, exaustividade, clones,
  mutação, retorno, laço, throw, recursos), codegen e E2E check/build/run.