import type { Context } from "@lindaflor/api/context";
import { o } from "@lindaflor/api/middlewares";
import { authorize } from "@lindaflor/api/middlewares/authorize";
import { authorizedProcedure } from "@lindaflor/api/middlewares/authorized";
import { requireActiveOrganization } from "@lindaflor/api/middlewares/require-active-organization";
import {
  createTodo,
  deleteTodos,
  getAllTodos,
  getSelectedTodos,
  getTodoById,
  updateTodo,
} from "@lindaflor/core/todo/todos";
import { schema } from "@lindaflor/shared/schemas/todo";
import type { EnhancedRouter } from "@orpc/server";

const todoV2Routes = {
  getAll: authorizedProcedure
    .use(requireActiveOrganization())
    .use(authorize("read", "Todo"))
    .route({
      method: "GET",
      path: "/todos",
      description: "Get all todos",
      summary: "v2 GetAll",
    })
    .input(schema.v2.getAll.input)
    .output(schema.v2.getAll.output)
    .handler(async ({ input, context }) =>
      getAllTodos({
        input,
        organizationId: context.activeOrganizationId,
        clientTimezone: context.client.timezone,
      }),
    ),

  getById: authorizedProcedure
    .route({
      method: "GET",
      path: "/todos/{id}",
      description: "Get todos by ID",
      summary: "v2 GetById",
    })
    .input(schema.v2.getById.input)
    .output(schema.v2.getById.output)
    .handler(async ({ input, context }) =>
      getTodoById({ input, ability: context.ability }),
    ),

  create: authorizedProcedure
    .use(authorize("create", "Todo"))
    .use(requireActiveOrganization())
    .route({
      method: "POST",
      path: "/todos",
      description: "Create a todo",
      summary: "v2 Create",
    })
    .input(schema.v2.create.input)
    .output(schema.v2.create.output)
    .handler(async ({ input, context }) =>
      createTodo({
        input,
        organizationId: context.activeOrganizationId,
      }),
    ),

  update: authorizedProcedure
    .route({
      method: "PATCH",
      path: "/todos",
      description: "Update a todo",
      summary: "v2 Update",
    })
    .input(schema.v2.update.input)
    .output(schema.v2.update.output)
    .handler(async ({ input, context }) =>
      updateTodo({ input, ability: context.ability }),
    ),

  delete: authorizedProcedure
    .route({
      method: "DELETE",
      path: "/todos",
      description: "Delete a todo",
      summary: "v2 Delete",
    })
    .input(schema.v2.delete.input)
    .output(schema.v2.delete.output)
    .handler(async ({ input, context }) =>
      deleteTodos({ input, ability: context.ability }),
    ),

  getSelected: authorizedProcedure
    .use(requireActiveOrganization())
    .use(authorize("read", "Todo"))
    .route({
      method: "GET",
      path: "/todos/selected",
      description: "Get selected todos",
      summary: "v2 GetSelected",
    })
    .input(schema.v2.getSelected.input)
    .output(schema.v2.getSelected.output)
    .handler(async ({ input, context }) =>
      getSelectedTodos({
        input,
        organizationId: context.activeOrganizationId,
        clientTimezone: context.client.timezone,
      }),
    ),
};

type TodoV2Routes = typeof todoV2Routes;

type TodoV2Router = EnhancedRouter<
  TodoV2Routes,
  Context,
  Context,
  Record<never, never>
>;

type TodoRouter = {
  v2: TodoV2Router;
};

function createTodoV2Router(routes: TodoV2Routes): TodoV2Router {
  return o.prefix("/v2").tag("Todo").router(routes);
}

export const todoRouter: TodoRouter = {
  v2: createTodoV2Router(todoV2Routes),
};
