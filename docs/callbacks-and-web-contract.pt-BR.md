# Contrato de callbacks e framework HTTP

[English](callbacks-and-web-contract.md)

## Estado e objetivo

Este é um contrato de projeto para as próximas fases de linguagem/runtime.
Callbacks que escapam, `truxt_web` e bindings Axum ainda não foram implementados.
O objetivo é uma experiência de rotas parecida com Express, mantendo Axum,
Tokio e tipos específicos de Rust atrás de um adaptador Truxt.

## Primeiro modelo de handler

O primeiro handler público é síncrono e owned:

```text
Handler = Fn(Request) -> Response
```

`Request` e `Response` são DTOs próprios de Truxt, não aliases dos generics do
Axum. Um `Request` inicial contém método, caminho, query bruta, cabeçalhos
repetidos e body UTF-8 com limite. `Response` contém status validado, cabeçalhos
repetidos e body texto owned. UTF-8 inválido, cabeçalho/status inválido, body
excessivo, falha de bind, 404 e 405 terão erros definidos; o adaptador não pode
entrar em panic nem descartar bytes silenciosamente.

O primeiro incremento de rotas deve cobrir rotas estáticas GET/POST, respostas
texto, 404/405 e limite de body. Conversão de DTO JSON, streaming, multipart,
WebSockets, middleware geral e handlers Truxt async ficam em incrementos próprios.

## Regras para callbacks que escapam

O contrato da callable é reutilizável e imutável (`Fn`), não `FnMut` nem `FnOnce`:

- Registrar uma rota transfere a posse do callback ao servidor.
- Capturas são snapshots owned. O callback não pode reter empréstimo do frame
  que o registrou.
- O callback pode ser chamado repetidas vezes e concorrentemente. Capturas
  precisam satisfazer `Send + Sync + 'static` na fronteira Rust do adaptador.
- Handles de recursos — por exemplo transação, stream do body ou conexão mutable
  — não se tornam clonáveis só para caber no armazenamento do callback.
- O adaptador pode compartilhar um handler imutável por `Arc`; isso compartilha
  uma callable, não clona recursos capturados.
- O HIR registra o modo de callable que escapa e a posse das capturas. Codegen
  segue essas decisões, sem inferir ownership uma segunda vez.

Essas regras são intencionalmente mais estreitas que todas as closures Rust e
callbacks TypeScript/JavaScript. Truxt documentará o subconjunto suportado e
rejeitará capturas incompatíveis com diagnósticos na fonte.

## Comportamento do adaptador Axum

Axum exige que handlers e futures retornadas atendam requisitos de `Send` e
lifetime; uma closure comum também precisa de `Clone + Send + Sync + 'static`.
O adaptador Truxt normalizará o handler internamente. Ele lerá e limitará o body
da requisição de forma assíncrona, construirá o DTO owned, executará o handler
síncrono em capacidade bloqueante limitada e converterá o DTO de resposta em
resposta HTTP. Consulte o [contrato de handler do Axum](https://docs.rs/axum/latest/axum/handler/trait.Handler.html).

Trabalho bloqueante precisa de limite explícito de concorrência. Tokio documenta
que uma tarefa `spawn_blocking` não pode ser abortada depois que começa. Um
timeout HTTP não pode ser descrito como cancelamento de query de banco ativa ou
rollback da transação. Consulte [Tokio `spawn_blocking`](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html).

## Fronteira com banco

`truxt_web` e `truxt_db` continuam adaptadores separados. Pool/configuração só
serão compartilhados entre requisições por um handle compartilhado explícito;
adquire-se conexão para a operação/requisição e transações permanecem não
clonáveis e com escopo definido. Erro de banco continua `DbError`; a aplicação
escolhe o mapeamento 404/409/500. Drivers Oracle, SQLite e PostgreSQL não precisam
ser todos async para usar o mesmo contrato síncrono de handler.

## Testes de aceite antes de considerar utilizável

- Callback registrado num escopo e chamado após seu retorno, mais de uma vez,
  sem capturas pendentes.
- Capturas mutáveis/emprestadas e recursos não compartilháveis são rejeitados
  pela semântica Truxt antes de um erro opaco do compilador Rust.
- Duas requisições concorrentes chamam um handler imutável sem mutação de estado
  compartilhado nem data race.
- GET texto, POST echo, 404/405, UTF-8 inválido, body excessivo, status/cabeçalho
  inválido e porta ocupada produzem os resultados documentados.
- Rota síncrona lenta não bloqueia I/O async não relacionado; sobrecarga e
  desligamento têm limites observáveis.
- Timeout HTTP é testado separadamente de cancelamento de banco e rollback.

O [guia oficial de LSP do VS Code](https://code.visualstudio.com/api/language-extensions/language-server-extension-guide)
e o [guia DAP](https://code.visualstudio.com/api/extension-guides/debugger-extension)
seguem relevantes para as trilhas separadas de editor e debugger.
