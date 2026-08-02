# apps/web — Agent guidance

Frontend conventions for React + Vite under `apps/web`. Load skills from the root Skills Index when working here (`web-design-system`, `tanstack-form`, `tanstack-query-best-practices`, `tanstack-table`, `tanstack-router-best-practices`).

## Route components layout

- Feature UI lives next to the route in a `-components/` folder (e.g. `routes/(auth)/arqueacao/-components/`).
- **kebab-case** filenames. Named exports only. Import via `@/…` — never relative `./` / `../`.
- Do **not** add barrel `index.ts` re-exports under `-components/`. Import each module by its file path.

## Mutation co-location

Mutating UI owns its `useMutation`. The parent owns **queries, selection/mode state, and composition** — not the write calls.

| Layer           | Owns                                                                                                              |
| --------------- | ----------------------------------------------------------------------------------------------------------------- |
| Parent / panel  | `useQuery`, selected ids, create-vs-detail mode, draft sync that must survive across children, composing children |
| Create form     | `create` mutation + `useAppForm`                                                                                  |
| Update form     | `update` (and related deletes on the same surface) + `useAppForm`                                                 |
| Editor / action | Domain write (e.g. `replacePoints`) + local edit state needed to submit                                           |

Do **not** pass `onSave` / `onDelete` callbacks that only forward to a parent mutation. Move the mutation into the child. Pass thin lifecycle callbacks when the parent must update selection after success (`onCreated(id)`, `onDeleted()`).

### Split create and update

Do **not** use an `isEditing` flag on one form. Split into `CreateXForm` / `UpdateXForm` (or equivalent), each with its own `useAppForm` and mutation. See `web-design-system` → forms rule and `cadastros/-components/tank-form-dialog.tsx`.

### Cache after writes

Prefer `orpc.*.v1.*.mutationOptions({ onSuccess, onError })`.

- Toast success / error with `sonner`.
- Invalidate with `queryClient.invalidateQueries({ queryKey: orpc.*.key(…) })`.
- Use `queryClient.setQueryData` when the mutation response should replace a stale detail immediately (then still invalidate related lists).

Shared invalidate helpers per feature are fine (e.g. `invalidate-tank-calibrations.ts`) when several mutation owners touch the same keys.

## Example: arqueação

| File                                                     | Role                                  |
| -------------------------------------------------------- | ------------------------------------- |
| `tank-calibration-panel.tsx`                             | Queries + selection + composition     |
| `create-certificate-form.tsx`                            | `create` mutation                     |
| `calibration-meta-form.tsx`                              | `update` + `delete` mutations         |
| `calibration-points-editor.tsx`                          | `replacePoints` mutation + CSV import |
| `tank-hero-card.tsx` / `certificate-history-sidebar.tsx` | Presentational / local filter UI only |
