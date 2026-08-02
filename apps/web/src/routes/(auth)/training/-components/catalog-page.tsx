import type { StudentCourseListItemOutput } from "@lindaflor/shared/schemas/training";
import { keepPreviousData, useMutation, useQuery } from "@tanstack/react-query";
import { Link, getRouteApi } from "@tanstack/react-router";
import {
  BookOpen,
  CalendarCheck,
  CheckCircle,
  Loader2,
  PlayCircle,
  X,
} from "lucide-react";
import React from "react";
import { toast } from "sonner";
import { useDebounceCallback } from "usehooks-ts";

import { Badge } from "@/components/ui/badge";
import { Button, buttonVariants } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
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
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { orpc } from "@/lib/orpc";
import { cn } from "@/lib/utils";
import {
  trainingTabSchema,
  type TrainingSearch,
} from "@/routes/(auth)/training/-components/search-schema";

const routeApi = getRouteApi("/(auth)/training/");

type CourseCardProps = {
  course: StudentCourseListItemOutput;
};

function CourseThumbnail({
  title,
  thumbnailUrl,
  isCompleted = false,
}: {
  title: string;
  thumbnailUrl: string | null;
  isCompleted?: boolean;
}) {
  return (
    <div className="relative aspect-video w-full overflow-hidden bg-muted">
      {thumbnailUrl ? (
        <img
          src={thumbnailUrl}
          alt=""
          className="size-full object-cover transition-transform duration-500 group-hover:scale-105"
        />
      ) : (
        <div className="flex size-full items-center justify-center">
          <BookOpen className="size-10 text-muted-foreground/40" />
        </div>
      )}
      <div className="absolute inset-x-0 bottom-0 h-12 bg-linear-to-t from-black/40 to-transparent" />
      {isCompleted ? (
        <Badge className="absolute top-3 right-3 gap-1 border-0 bg-success/90 text-white">
          <CheckCircle className="size-3" />
          Concluído
        </Badge>
      ) : null}
      <span className="sr-only">{title}</span>
    </div>
  );
}

function CourseCard({ course }: CourseCardProps) {
  return (
    <Card className="group h-full overflow-hidden pt-0 transition-all hover:shadow-md">
      <Link
        to="/training/courses/$id"
        params={{ id: course.id }}
        className="block"
      >
        <CourseThumbnail
          title={course.title}
          thumbnailUrl={course.thumbnail_url}
          isCompleted={course.is_completed}
        />
      </Link>
      <CardHeader className="space-y-1 pb-3">
        <CardTitle className="line-clamp-1 text-base">
          <Link
            to="/training/courses/$id"
            params={{ id: course.id }}
            className="hover:underline"
          >
            {course.title}
          </Link>
        </CardTitle>
        {course.description ? (
          <CardDescription className="line-clamp-2">
            {course.description}
          </CardDescription>
        ) : null}
      </CardHeader>
      <CardContent className="flex items-center justify-between pt-1">
        {course.is_completed ? (
          course.completed_at ? (
            <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
              <CalendarCheck className="size-3.5" />
              {course.completed_at.toLocaleDateString("pt-BR", {
                day: "2-digit",
                month: "short",
                year: "numeric",
              })}
            </span>
          ) : (
            <span className="inline-flex items-center gap-1 text-xs font-medium text-success">
              <CheckCircle className="size-3.5" />
              Concluído
            </span>
          )
        ) : (
          <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
            <PlayCircle className="size-3.5" />
            Em andamento
          </span>
        )}
        <Link
          to="/training/courses/$id"
          params={{ id: course.id }}
          className={cn(
            buttonVariants({
              variant: course.is_completed ? "outline" : "default",
              size: "sm",
            }),
          )}
        >
          {course.is_completed ? "Revisar" : "Continuar"}
        </Link>
      </CardContent>
    </Card>
  );
}

function CourseSkeleton() {
  return (
    <Card className="h-full overflow-hidden pt-0">
      <Skeleton className="aspect-video w-full rounded-none" />
      <CardHeader>
        <Skeleton className="h-5 w-3/4" />
        <Skeleton className="h-4 w-full" />
        <Skeleton className="h-4 w-2/3" />
      </CardHeader>
      <CardContent>
        <Skeleton className="h-8 w-24" />
      </CardContent>
    </Card>
  );
}

function CourseGridSkeleton() {
  return (
    <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
      {[
        "skeleton-a",
        "skeleton-b",
        "skeleton-c",
        "skeleton-d",
        "skeleton-e",
        "skeleton-f",
      ].map((key) => (
        <CourseSkeleton key={key} />
      ))}
    </div>
  );
}

type AvailableCourseCardProps = CourseCardProps & {
  pending: boolean;
  onEnroll: (courseId: string) => void;
};

function AvailableCourseCard({
  course,
  pending,
  onEnroll,
}: AvailableCourseCardProps) {
  return (
    <Card className="group h-full overflow-hidden pt-0 transition-all hover:shadow-md">
      <CourseThumbnail
        title={course.title}
        thumbnailUrl={course.thumbnail_url}
      />
      <CardHeader className="space-y-1 pb-3">
        <CardTitle className="line-clamp-1 text-base">{course.title}</CardTitle>
        {course.description ? (
          <CardDescription className="line-clamp-2">
            {course.description}
          </CardDescription>
        ) : null}
      </CardHeader>
      <CardContent className="mt-auto">
        <Button
          className="w-full"
          disabled={pending}
          onClick={() => onEnroll(course.id)}
        >
          {pending ? <Loader2 className="size-4 animate-spin" /> : null}
          Matricular-se
        </Button>
      </CardContent>
    </Card>
  );
}

function CatalogEmptyState({
  icon,
  title,
  description,
  action,
}: {
  icon: React.ReactNode;
  title: string;
  description: string;
  action?: React.ReactNode;
}) {
  return (
    <Card>
      <CardContent className="py-8">
        <Empty>
          <EmptyMedia variant="icon">{icon}</EmptyMedia>
          <EmptyTitle>{title}</EmptyTitle>
          <EmptyDescription>{description}</EmptyDescription>
          {action}
        </Empty>
      </CardContent>
    </Card>
  );
}

function PaginationControls({
  search,
  totalPages,
  onNavigate,
}: {
  search: TrainingSearch;
  totalPages: number;
  onNavigate: (search: TrainingSearch) => void;
}) {
  if (totalPages <= 1) return null;

  return (
    <div className="mt-8 flex items-center justify-center gap-2">
      <Button
        variant="outline"
        size="sm"
        disabled={search.pageIndex <= 1}
        onClick={() =>
          onNavigate({ ...search, pageIndex: search.pageIndex - 1 })
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
          onNavigate({ ...search, pageIndex: search.pageIndex + 1 })
        }
      >
        Próxima
      </Button>
    </div>
  );
}

export function TrainingCatalogPage() {
  const search = routeApi.useSearch();
  const navigate = routeApi.useNavigate();
  const tab = search.tab ?? "my";

  const {
    data: { items: myCourses = [], meta: myMeta } = {},
    refetch: myCoursesRefetch,
    isLoading: myCoursesIsLoading,
    isPlaceholderData: myCoursesIsPlaceholderData,
    isError: myCoursesIsError,
    error: myCoursesError,
  } = useQuery(
    orpc.training.v1.courses.list.queryOptions({
      input: {
        search: search.search,
        pageIndex: search.pageIndex,
        pageSize: search.pageSize,
        filter: "enrolled",
      },
      placeholderData: keepPreviousData,
      select: (result) => ({
        items: result.data,
        meta: result.meta,
      }),
    }),
  );

  const {
    data: { items: publishedCourses = [], meta: availableMeta } = {},
    refetch: availableCoursesRefetch,
    isLoading: availableCoursesIsLoading,
    isPlaceholderData: availableCoursesIsPlaceholderData,
    isError: availableCoursesIsError,
    error: availableCoursesError,
  } = useQuery(
    orpc.training.v1.courses.list.queryOptions({
      input: {
        search: search.search,
        pageIndex: search.pageIndex,
        pageSize: search.pageSize,
        filter: "available",
      },
      placeholderData: keepPreviousData,
      select: (result) => ({
        items: result.data,
        meta: result.meta,
      }),
    }),
  );

  const enrollMutation = useMutation(
    orpc.training.v1.enrollments.selfEnroll.mutationOptions({
      onSuccess: async () => {
        toast.success("Matrícula realizada");
        await myCoursesRefetch();
        await availableCoursesRefetch();
      },
      onError: (error) => toast.error(error.message),
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

  return (
    <div className="mx-auto w-full max-w-6xl px-4 py-8 sm:px-6 lg:px-8">
      <div className="mb-8 space-y-1">
        <h1 className="text-2xl font-bold tracking-tight">Treinamentos</h1>
        <p className="text-sm text-muted-foreground">
          Explore os cursos disponíveis para você.
        </p>
      </div>

      <InputGroup className="w-full md:max-w-sm mb-6">
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

      <Tabs
        value={tab}
        className="flex flex-col"
        onValueChange={(value) => {
          const nextTab = trainingTabSchema.parse(value);
          void navigate({
            search: {
              ...search,
              tab: nextTab,
              pageIndex: 1,
            },
          });
        }}
      >
        <TabsList className="mb-6 grid w-full max-w-md grid-cols-2">
          <TabsTrigger value="my">Meus cursos</TabsTrigger>
          <TabsTrigger value="available">Disponíveis</TabsTrigger>
        </TabsList>

        <TabsContent value="my">
          {myCoursesIsLoading && !myCoursesIsPlaceholderData ? (
            <CourseGridSkeleton />
          ) : myCoursesIsError ? (
            <CatalogEmptyState
              icon={<BookOpen className="size-4" />}
              title="Não foi possível carregar seus cursos"
              description={myCoursesError.message}
            />
          ) : myCourses.length === 0 ? (
            <CatalogEmptyState
              icon={<BookOpen className="size-4" />}
              title="Nenhum curso encontrado"
              description={
                search.search
                  ? "Tente ajustar os termos da busca."
                  : "Você ainda não tem cursos disponíveis."
              }
              action={
                !search.search ? (
                  <Link
                    to="/training"
                    search={{ ...search, tab: "available" }}
                    className={cn(buttonVariants({ size: "sm" }))}
                  >
                    Ver cursos disponíveis
                  </Link>
                ) : null
              }
            />
          ) : (
            <>
              <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
                {myCourses.map((course) => (
                  <CourseCard key={course.id} course={course} />
                ))}
              </div>
              <PaginationControls
                search={search}
                totalPages={myMeta?.totalPages ?? 1}
                onNavigate={(nextSearch) =>
                  void navigate({ search: nextSearch })
                }
              />
            </>
          )}
        </TabsContent>

        <TabsContent value="available">
          {availableCoursesIsLoading && !availableCoursesIsPlaceholderData ? (
            <CourseGridSkeleton />
          ) : availableCoursesIsError ? (
            <CatalogEmptyState
              icon={<BookOpen className="size-4" />}
              title="Não foi possível carregar cursos disponíveis"
              description={availableCoursesError.message}
            />
          ) : publishedCourses.length === 0 ? (
            <CatalogEmptyState
              icon={<BookOpen className="size-4" />}
              title="Nenhum curso disponível"
              description={
                search.search
                  ? "Tente ajustar os termos da busca."
                  : "Não há cursos publicados no momento."
              }
            />
          ) : (
            <>
              <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
                {publishedCourses.map((course) => (
                  <AvailableCourseCard
                    key={course.id}
                    course={course}
                    pending={enrollMutation.isPending}
                    onEnroll={(courseId) =>
                      enrollMutation.mutate({ course_id: courseId })
                    }
                  />
                ))}
              </div>
              <PaginationControls
                search={search}
                totalPages={availableMeta?.totalPages ?? 1}
                onNavigate={(nextSearch) =>
                  void navigate({ search: nextSearch })
                }
              />
            </>
          )}
        </TabsContent>
      </Tabs>
    </div>
  );
}
