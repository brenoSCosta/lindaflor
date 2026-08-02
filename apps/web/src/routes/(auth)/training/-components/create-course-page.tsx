import { createCourseInput } from "@lindaflor/shared/schemas/training";
import { useMutation } from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { ArrowLeft } from "lucide-react";
import { toast } from "sonner";

import { useAppForm } from "@/components/form/hooks";
import { buttonVariants } from "@/components/ui/button";
import { orpc } from "@/lib/orpc";
import { cn } from "@/lib/utils";
import { CourseEditor } from "@/routes/(auth)/training/-components/course-editor";
import { courseFormDefaultValues } from "@/routes/(auth)/training/-components/training-form-utils";

export function CreateCoursePage() {
  const navigate = useNavigate();

  const createMutation = useMutation(
    orpc.training.v1.courses.create.mutationOptions({
      onSuccess: (course) => {
        toast.success("Curso criado");
        void navigate({
          to: "/training/courses/$id/manage",
          params: { id: course.id },
        });
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  const form = useAppForm({
    defaultValues: courseFormDefaultValues,
    validators: {
      onSubmit: createCourseInput,
    },
    onSubmit: ({ value }) => {
      createMutation.mutate(value);
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
          autoStart
          form={form}
          header={
            <div className="flex flex-wrap items-center justify-between gap-2">
              <Link
                to="/training/manage"
                search={{ pageIndex: 1, pageSize: 12 }}
                className={cn(
                  buttonVariants({ variant: "ghost", size: "sm" }),
                  "inline-flex items-center",
                )}
              >
                <ArrowLeft className="mr-1 size-4" />
                Voltar para gerenciamento
              </Link>
            </div>
          }
          footer={
            <form.Button loadingText="Criando curso" className="w-full">
              Criar curso
            </form.Button>
          }
        />
      </form.AppForm>
    </form>
  );
}
