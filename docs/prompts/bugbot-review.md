# Bugbot Review — lindaflor

Prompt reutilizável para revisar implementações locais com o Bugbot: bugs, padrões do projeto, anti-patterns de React (`useEffect`), e duplicação.

## Uso rápido

1. Rode as verificações automáticas:

```bash
bun run check && bun run check-types && bun test
```

2. Cole o [prompt completo](#prompt-completo) no chat do Cursor.
3. (Opcional) Peça para corrigir achados `critical` e `important`.
4. Se mexeu em auth/API, rode também o [Security Review](#security-review-opcional).

## Prompt completo

```
/review-bugbot

Custom Instructions:
Revise o diff contra as convenções do lindaflor. Leia AGENTS.md, apps/web/AGENTS.md e packages/api/AGENTS.md quando o diff tocar nessas áreas.

## Prioridade 1 — Bugs e regressões
- Lógica quebrada, edge cases, null/undefined não tratado
- Race conditions, estado stale, cache incorreto após mutation
- N+1 queries, loops O(n²) desnecessários
- Validação faltando em input dinâmico (JSON, query params, event args)

## Prioridade 2 — Padrões obrigatórios do projeto
- Sem try/catch novo → effect (Effect.tryPromise, Effect.catchAll)
- Sem console.* → effect para logging/erros
- Auth só via CASL — nunca check custom
- API só via oRPC + ORPCError + schemas Zod v4
- Sem imports relativos → @lindaflor/... ou @/
- Named exports, kebab-case, sem barrel index.ts, sem editar *.gen.ts
- Sem unsafe `as` em valores dinâmicos → Zod ou type guard

## Prioridade 3 — Web (apps/web)
- Mutation co-location: child com useMutation, parent só queries/seleção/composição
- Create/update em forms separados; useAppForm + mutationOptions
- Toast sonner; invalidate com orpc.*.key() ou helper compartilhado
- UI em routes/.../-components/

### useEffect — flagrar quando:
- Fetch de dados → useQuery ou route loader
- Sync props→state → valor derivado, defaultValues, ou key={id}
- Pós-mutation side-effect → onSuccess/onError, não useEffect
- Vários useEffect encadeados → sugerir refactor

### useEffect — ok só para subscriptions com cleanup ou libs imperativas

## Prioridade 4 — API (packages/api, apps/server)
- Output enrichment no schema, não no handler
- ORPCError correto; subject() completo para CASL
- Effect para I/O falível

## Prioridade 5 — Duplicação e reinvenção
- Código que já existe globalmente recriado em cada arquivo
- Helpers, hooks, invalidate, form setup, patterns duplicados
- Apontar utilitário existente no monorepo e sugerir reutilizar

## Formato
Severidade (critical/important/minor), arquivo:linha, problema, fix concreto.
Ignore nits que oxlint já pega, salvo se indicarem bug ou violação de padrão.
```

## Variantes

### Só mudanças não commitadas

```
/review-bugbot — review uncommitted changes

[cole o Custom Instructions acima]
```

### Review + corrigir

Adicione ao final do prompt:

```
Depois do review, corrija todos os achados critical e important. Rode bun run check, bun run check-types e bun test ao final.
```

### Web UI — incluir e2e

Se o diff tocou `apps/web`:

```
Depois das correções, rode também bun run test-e2e.
```

## Security Review (opcional)

Use quando o diff tocar auth, permissões, API handlers, upload ou dados sensíveis:

```
/review-security

Custom Instructions:
Foque em auth bypass, CASL mal configurado, input sem Zod, secrets expostos, IDOR, SQL injection, upload inseguro.
```

## O que cada camada pega

| Camada                                   | Pega                                           |
| ---------------------------------------- | ---------------------------------------------- |
| `bun run check` / `check-types` / `test` | Estilo, tipos, testes quebrados                |
| Bugbot (este prompt)                     | Bugs, padrões, `useEffect` abusivo, duplicação |
| Security Review                          | Vulnerabilidades em auth/API                   |

## Fluxo recomendado

```
implementou
  → bun run check && bun run check-types && bun test
  → @docs/prompts/bugbot-review.md (ou cole o prompt)
  → corrigir critical/important
  → /review-security (se auth/API)
  → commit / PR
```
