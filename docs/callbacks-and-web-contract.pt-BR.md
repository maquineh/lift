# Contrato de callbacks e framework HTTP

[English](callbacks-and-web-contract.md)

## Estado e objetivo

Este documento descreve o alvo geral da API HTTP. A PoC de notas agora inclui
`lift_web` com Axum/Tokio atrás de um adaptador Rust, DTOs owned `Request` e
`Response`, handlers síncronos nomeados e rotas estáticas `GET /health`,
`GET /notes` e `POST /notes`. Esses DTOs, handlers e registros agora são
declarados em Lift, com bridges geradas para um adaptador Rust genérico.
Veja [o subconjunto implementado](resources-and-http.pt-BR.md).
Closures que escapam e callbacks FFI arbitrários continuam fora.

## Primeiro modelo de handler

O primeiro handler público é síncrono e owned:

```text
Handler = Fn(Request) -> Response
```

`Request` e `Response` são DTOs próprios de Lift, não aliases dos generics do
Axum. O `Request` implementado contém método, caminho, query bruta e body UTF-8
com limite. `Response` contém status (200..599) e body texto owned; o adaptador
define text/plain UTF-8. Cabeçalhos repetidos pertencem ao alvo geral futuro.
A PoC retorna 400 para UTF-8 inválido, 413 para body excessivo, `Result` para
falha de bind e 404/405 do roteador. Status inválido da resposta retorna 500.

O incremento da PoC cobre rotas estáticas, respostas texto, 404/405 e limite de
16 KiB para body. UTF-8 inválido e body acima do limite retornam 400/413; falha
de bind vira erro `Result`. Conversão de DTO JSON, streaming, multipart,
WebSockets, middleware geral, callbacks arbitrários e handlers async ficam
em incrementos próprios.

## Regras para callbacks que escapam

O contrato futuro de closure que escapa é reutilizável e imutável (`Fn`), não
`FnMut` nem `FnOnce`; as bridges de handlers nomeados atuais não têm capturas:

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
callbacks TypeScript/JavaScript. Lift documentará o subconjunto suportado e
rejeitará capturas incompatíveis com diagnósticos na fonte.

## Comportamento do adaptador Axum

Axum exige que handlers e futures retornadas atendam requisitos de `Send` e
lifetime; uma closure comum também precisa de `Clone + Send + Sync + 'static`.
O adaptador Lift normalizará o handler internamente. Ele lerá e limitará o body
da requisição de forma assíncrona, construirá o DTO owned, executará o handler
síncrono em capacidade bloqueante limitada e converterá o DTO de resposta em
resposta HTTP. Consulte o [contrato de handler do Axum](https://docs.rs/axum/latest/axum/handler/trait.Handler.html).

Trabalho bloqueante precisa de limite explícito de concorrência. Tokio documenta
que uma tarefa `spawn_blocking` não pode ser abortada depois que começa. Um
timeout HTTP não pode ser descrito como cancelamento de query de banco ativa ou
rollback da transação. Consulte [Tokio `spawn_blocking`](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html).

## Fronteira com banco

`lift_web` e `lift_db` são adaptadores separados. Na PoC, `NoteStore` é
compartilhado explicitamente por `Arc<Mutex<_>>`; um semáforo limita a oito os
handlers síncronos ativos. Transações permanecem dentro do `lift_db` e são
consumidas no commit. A camada HTTP não devolve detalhes internos do SQLite.
Drivers Oracle, SQLite e PostgreSQL não precisam ser todos async para usar o
mesmo contrato síncrono de handler.

## Testes de aceite antes de considerar utilizável

- Callback registrado num escopo e chamado após seu retorno, mais de uma vez,
  sem capturas pendentes.
- Capturas mutáveis/emprestadas e recursos não compartilháveis são rejeitados
  pela semântica Lift antes de um erro opaco do compilador Rust.
- Duas requisições concorrentes chamam um handler imutável sem mutação de estado
  compartilhado nem data race.
- GET texto, POST echo, 404/405, UTF-8 inválido, body excessivo, status/cabeçalho
  inválido e porta ocupada produzem os resultados documentados.
- Rota síncrona lenta não bloqueia I/O async não relacionado; sobrecarga e
  desligamento têm limites observáveis.
- Timeout HTTP é testado separadamente de cancelamento de banco e rollback.

O serviço executável e os comandos de demonstração estão em
[`examples/notes-service`](../examples/notes-service/README.pt-BR.md). Os handlers
da PoC ainda são código Rust do adaptador, não funções/callbacks definidos em
Lift.

O [guia oficial de LSP do VS Code](https://code.visualstudio.com/api/language-extensions/language-server-extension-guide)
e o [guia DAP](https://code.visualstudio.com/api/extension-guides/debugger-extension)
seguem relevantes para as trilhas separadas de editor e debugger.
