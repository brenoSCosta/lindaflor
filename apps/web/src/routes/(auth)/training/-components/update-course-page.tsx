import { createCourseInput } from "@lindaflor/shared/schemas/training";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Link, getRouteApi } from "@tanstack/react-router";
import { ArrowLeft, BookOpen } from "lucide-react";
import { useMemo } from "react";
import { toast } from "sonner";

import { useAppForm } from "@/components/form/hooks";
import { Badge } from "@/components/ui/badge";
import { buttonVariants } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  Empty,
  EmptyDescription,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/skeleton";
import { orpc } from "@/lib/orpc";
import { cn } from "@/lib/utils";
import { CourseEditor } from "@/routes/(auth)/training/-components/course-editor";
import {
  courseFormDefaultValues,
  type CourseFormValues,
} from "@/routes/(auth)/training/-components/training-form-utils";

const routeApi = getRouteApi("/(auth)/training/courses/$id/manage");

type UpdateCourseFormProps = {
  courseId: string;
  initialValues: CourseFormValues;
  isPublished: boolean;
};

function UpdateCourseForm({
  courseId,
  initialValues,
  isPublished,
}: UpdateCourseFormProps) {
  const updateMutation = useMutation(
    orpc.training.v1.courses.update.mutationOptions({
      onSuccess: () => {
        toast.success("Curso atualizado");
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  const form = useAppForm({
    defaultValues: initialValues,
    validators: {
      onSubmit: createCourseInput,
    },
    onSubmit: ({ value }) => {
      updateMutation.mutate({ id: courseId, ...value });
    },
  });

  return (
    <form
      action={async () => {
        await form.handleSubmit();
      }}
    >
      <form.AppForm>
        <CourseEditor
          form={form}
          header={
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="flex items-center gap-2">
                <Link
                  to="/training/manage"
                  search={{ pageIndex: 1, pageSize: 12 }}
                  className={cn(
                    buttonVariants({ variant: "ghost", size: "sm" }),
                    "inline-flex items-center",
                  )}
                >
                  <ArrowLeft className="mr-1 size-4" />
                  Voltar
                </Link>
                <Badge variant={isPublished ? "secondary" : "default"}>
                  {isPublished ? "Publicado" : "Rascunho"}
                </Badge>
              </div>
            </div>
          }
          footer={
            <form.Button loadingText="Salvando alterações" className="w-full">
              Salvar alterações
            </form.Button>
          }
        />
      </form.AppForm>
    </form>
  );
}

export function UpdateCoursePage() {
  const { id: courseId } = routeApi.useParams();

  const { data, isLoading } = useQuery(
    orpc.training.v1.courses.get.queryOptions({
      input: { id: courseId },
    }),
  );

  const initialValues = useMemo(
    () => (data ? createCourseInput.parse(data) : courseFormDefaultValues),
    [data],
  );

  if (isLoading) {
    return (
      <div className="flex flex-col lg:grid lg:h-dvh lg:grid-cols-[1fr_380px] lg:grid-rows-1">
        <main className="lg:h-full lg:overflow-y-auto">
          <div className="mx-auto max-w-4xl space-y-5 p-4 sm:p-6 lg:p-8">
            <Skeleton className="h-8 w-40" />
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-32 w-full" />
          </div>
        </main>
        <aside className="hidden border-l bg-card lg:block">
          <Skeleton className="m-4 h-64" />
        </aside>
      </div>
    );
  }

  if (!data) {
    return (
      <main className="mx-auto w-full max-w-3xl px-4 py-16 text-center">
        <Card>
          <CardContent className="py-8">
            <Empty>
              <EmptyMedia variant="icon">
                <BookOpen className="size-4" />
              </EmptyMedia>
              <EmptyTitle>Curso não encontrado</EmptyTitle>
              <EmptyDescription>
                O curso solicitado não existe ou você não tem permissão.
              </EmptyDescription>
            </Empty>
          </CardContent>
        </Card>
        <Link
          to="/training/manage"
          search={{ pageIndex: 1, pageSize: 12 }}
          className={cn(buttonVariants({ variant: "outline" }), "mt-4")}
        >
          Voltar
        </Link>
      </main>
    );
  }

  return (
    <UpdateCourseForm
      key={courseId}
      courseId={courseId}
      initialValues={initialValues}
      isPublished={data.is_published}
    />
  );
}
