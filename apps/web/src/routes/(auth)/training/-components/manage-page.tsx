import { subject } from "@lindaflor/shared/lib/ability/subjects";
import type { TrainingCourseOutput } from "@lindaflor/shared/schemas/training";
import { keepPreviousData, useMutation, useQuery } from "@tanstack/react-query";
import { getRouteApi, Link } from "@tanstack/react-router";
import { BookOpen, Plus, Trash2, Users, X } from "lucide-react";
import React from "react";
import { toast } from "sonner";
import { useDebounceCallback } from "usehooks-ts";

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button, buttonVariants } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  Empty,
  EmptyDescription,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import {
  InputGroup,
  InputGroupButton,
  InputGroupInput,
} from "@/components/ui/input-group";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { useIsMobile } from "@/hooks/use-mobile";
import { Can } from "@/lib/ability";
import { orpc } from "@/lib/orpc";
import { CourseEnrollmentsDialog } from "@/routes/(auth)/training/-components/course-enrollments-dialog";

const routeApi = getRouteApi("/(auth)/training/manage");

function DeleteCourseButton({
  course,
  onDeleted,
}: {
  course: TrainingCourseOutput;
  onDeleted: () => void;
}) {
  const deleteMutation = useMutation(
    orpc.training.v1.courses.delete.mutationOptions({
      onSuccess: () => {
        toast.success("Curso excluído");
        onDeleted();
      },
      onError: (error) => toast.error(error.message),
    }),
  );

  return (
    <Can I="delete" this={subject("Training", course)}>
      <AlertDialog>
        <AlertDialogTrigger
          render={
            <Button variant="ghost" size="sm" aria-label="Excluir curso">
              <Trash2 className="size-4 text-destructive" />
            </Button>
          }
        />
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Excluir curso</AlertDialogTitle>
            <AlertDialogDescription>
              Tem certeza que deseja excluir &quot;{course.title}&quot;? Esta
              ação não pode ser desfeita.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancelar</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              disabled={deleteMutation.isPending}
              onClick={() => deleteMutation.mutate({ id: course.id })}
            >
              Excluir
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Can>
  );
}

export function ManagePage() {
  const search = routeApi.useSearch();
  const navigate = routeApi.useNavigate();
  const [enrollmentCourse, setEnrollmentCourse] =
    React.useState<TrainingCourseOutput | null>(null);

  const {
    data: { courses = [], totalPages = 1 } = {},
    refetch,
    isLoading,
    isPlaceholderData,
  } = useQuery(
    orpc.training.v1.courses.list.queryOptions({
      input: {
        search: search.search,
        pageIndex: search.pageIndex,
        pageSize: search.pageSize,
      },
      placeholderData: keepPreviousData,
      select: (result) => ({
        courses: result.data,
        totalPages: result.meta.totalPages,
      }),
    }),
  );

  const [searchValue, setSearchValue] = React.useState(search.search ?? "");
  const debouncedNavigate = useDebounceCallback((value: string) => {
    void navigate({
      search: {
        ...search,
        pageIndex: 1,
        search: value.trim() || undefined,
      },
    });
  }, 300);
  const isMobile = useIsMobile();

  return (
    <main className="mx-auto w-full max-w-7xl px-4 py-8 sm:px-6 lg:px-8">
      <CourseEnrollmentsDialog
        course={enrollmentCourse}
        open={enrollmentCourse !== null}
        onOpenChange={(open) => {
          if (!open) {
            setEnrollmentCourse(null);
          }
        }}
      />
      <div className="mb-6 flex flex-col gap-4 lg:flex-row lg:items-end lg:justify-between">
        <div className="space-y-1">
          <h1 className="text-2xl font-bold">Gerenciar treinamentos</h1>
          <p className="text-sm text-muted-foreground">
            Crie cursos, organize módulos e controle matrículas.
          </p>
        </div>

        <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
          <InputGroup className="w-full md:max-w-sm">
            <InputGroupInput
              placeholder="Buscar curso"
              value={searchValue}
              onChange={(e) => {
                setSearchValue(e.target.value);
                debouncedNavigate(e.target.value);
              }}
            />
            {searchValue && (
              <InputGroupButton
                size="icon-xs"
                onClick={() => {
                  setSearchValue("");
                  debouncedNavigate("");
                }}
                aria-label="Limpar busca"
              >
                <X className="size-4" />
              </InputGroupButton>
            )}
          </InputGroup>

          <Can I="create" a="Training">
            <Link to="/training/courses/new" className={buttonVariants()}>
              <Plus className="size-4" />
              Novo curso
            </Link>
          </Can>
        </div>
      </div>

      {isLoading && !isPlaceholderData ? (
        <div className="space-y-3">
          <Skeleton className="h-10 w-full" />
          <Skeleton className="h-10 w-full" />
          <Skeleton className="h-10 w-full" />
        </div>
      ) : courses.length === 0 ? (
        <Card>
          <CardContent className="py-8">
            <Empty>
              <EmptyMedia variant="icon">
                <BookOpen className="size-4" />
              </EmptyMedia>
              <EmptyTitle>Nenhum curso encontrado</EmptyTitle>
              <EmptyDescription>
                Crie o primeiro treinamento ou ajuste os filtros.
              </EmptyDescription>
            </Empty>
          </CardContent>
        </Card>
      ) : (
        <>
          {isMobile ? (
            <div className="grid gap-3">
              {courses.map((course) => (
                <Card key={course.id}>
                  <CardContent className="space-y-3 p-4">
                    <div className="space-y-1">
                      <p className="font-medium">{course.title}</p>
                      {course.description ? (
                        <p className="line-clamp-2 text-sm text-muted-foreground">
                          {course.description}
                        </p>
                      ) : null}
                      <Badge
                        variant={course.is_published ? "secondary" : "default"}
                      >
                        {course.is_published ? "Publicado" : "Rascunho"}
                      </Badge>
                    </div>
                    <div className="flex flex-wrap items-center gap-2">
                      <Can I="manage" a="TrainingEnrollment">
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => setEnrollmentCourse(course)}
                        >
                          <Users className="size-4" />
                          Matrículas
                        </Button>
                      </Can>
                      <Link
                        to="/training/courses/$id/manage"
                        params={{ id: course.id }}
                        className={buttonVariants({ size: "sm" })}
                      >
                        Editar
                      </Link>
                      <DeleteCourseButton
                        course={course}
                        onDeleted={() => void refetch()}
                      />
                    </div>
                  </CardContent>
                </Card>
              ))}
            </div>
          ) : (
            <div className="overflow-x-auto rounded-md border">
              <Table className="min-w-xl">
                <TableHeader>
                  <TableRow>
                    <TableHead>Título</TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead className="text-right">Ações</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {courses.map((course) => (
                    <TableRow key={course.id}>
                      <TableCell>
                        <div>
                          <p className="font-medium">{course.title}</p>
                          {course.description ? (
                            <p className="line-clamp-1 text-sm text-muted-foreground">
                              {course.description}
                            </p>
                          ) : null}
                        </div>
                      </TableCell>
                      <TableCell>
                        <Badge
                          variant={
                            course.is_published ? "secondary" : "default"
                          }
                        >
                          {course.is_published ? "Publicado" : "Rascunho"}
                        </Badge>
                      </TableCell>
                      <TableCell className="text-right">
                        <div className="flex flex-wrap items-center justify-end gap-2">
                          <Can I="manage" a="TrainingEnrollment">
                            <Button
                              variant="outline"
                              size="sm"
                              onClick={() => setEnrollmentCourse(course)}
                            >
                              <Users className="size-4" />
                              Matrículas
                            </Button>
                          </Can>
                          <Link
                            to="/training/courses/$id/manage"
                            params={{ id: course.id }}
                            className={buttonVariants({ size: "sm" })}
                          >
                            Editar
                          </Link>
                          <DeleteCourseButton
                            course={course}
                            onDeleted={() => void refetch()}
                          />
                        </div>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          )}

          {totalPages > 1 && (
            <div className="mt-8 flex items-center justify-center gap-3">
              <Button
                variant="outline"
                size="sm"
                disabled={search.pageIndex <= 1}
                onClick={() =>
                  void navigate({
                    search: { ...search, pageIndex: search.pageIndex - 1 },
                  })
                }
              >
                Anterior
              </Button>
              <span className="text-sm text-muted-foreground">
                Página {search.pageIndex} de {totalPages}
              </span>
              <Button
                variant="outline"
                size="sm"
                disabled={search.pageIndex >= totalPages}
                onClick={() =>
                  void navigate({
                    search: { ...search, pageIndex: search.pageIndex + 1 },
                  })
                }
              >
                Próxima
              </Button>
            </div>
          )}
        </>
      )}
    </main>
  );
}
