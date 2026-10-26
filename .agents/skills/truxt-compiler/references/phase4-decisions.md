# Result/I/O e Option v1

English / Português do Brasil: `docs/result-and-io.md` e `docs/result-and-io.pt-BR.md` na raiz do projeto.

Este incremento adiciona o tipo especial `Result<T, E>` à AST/parser/HIR/codegen,
sem generics gerais. O tipo pode aparecer em anotações, campos, arrays,
parâmetros e retornos Truxt; aninhamento é aceito na linguagem. `void` é permitido
no sucesso, nunca no erro. Ciclos de tamanho direto por Result são rejeitados.

`result.is_ok(value)` empresta e retorna boolean. `result.unwrap(value)` e
`result.unwrap_err(value)` emprestam e clonam o payload para um valor owned.
Argumentos são avaliados uma vez; o Result continua disponível. Unwrap na
variante errada causa panic; não há prova de fluxo de variante. O namespace
`result` só atua quando não está sombreado.

Não há construtores Result, pattern matching, operador de propagação `?`,
`try/catch`, unions ou propagação automática. Valores Result vêm de imports ou de funções Truxt que
recebem/retornam esses valores. O ownership continua conservador.

FFI admite Result somente no retorno, com sucesso primitivo/array raso/void e
erro primitivo/array raso. Result aninhado e parâmetros Result ficam fora do FFI.
O ponteiro safe do wrapper deixa rustc confirmar a assinatura exata.

`truxt_core::io` retorna `Result<_, String>` em arguments/read_stdin/read_file/
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
- option.some constrói Some com payload owned explícito, sem lift T→T?. option.is_some empresta e testa; option.unwrap empresta/clona payload e causa panic em None. Não há narrowing.
- Campo field: T? continua obrigatório; field?: T e omissão não são suportados. Igualdade de opcional fornece contexto ao literal null, sem refinamento de fluxo.
- T? ?? T retorna T; T? ?? T? retorna T?, permitindo cadeia. ?? associa à esquerda, com precedência abaixo de ||; mistura sem parênteses com &&/|| é rejeitada. LHS é avaliada uma vez e RHS somente no caso None; false/0/string vazia em Some não ativam fallback.
- HIR OptionSome/OptionNone/Coalesce registra owned payload/snapshot e modo Value/Optional. LHS place é clonado pelas regras de valor existentes; Rust usa match owned, sem closure e com RHS somente no braço None. Codegen não infere ownership.
- Liveness observa os dois operandos conservadoramente e preserva clones dentro dos novos nós. Nenhuma regra de aliasing é relaxada.
- Option está totalmente fora do FFI: parâmetros, retornos e ocorrências em Array/Result são rejeitados. Não há optional chaining, pattern match, propagação automática ou undefined.
- Testes parser/sema/codegen/E2E comprovam os caminhos exercitados, incluindo contadores lazy/exactly-once, cadeias, payloads false/0/"", campos, null sem contexto, ciclo Node? e rejeição FFI. Guias bilíngues: docs/option.md e docs/option.pt-BR.md.
