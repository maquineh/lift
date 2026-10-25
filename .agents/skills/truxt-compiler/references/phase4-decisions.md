# Incremento limitado de Result e I/O

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

Não há construtores, pattern matching, `?`, `try/catch`, unions, optionals ou
propagação automática. Valores Result vêm de imports ou de funções Truxt que
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
