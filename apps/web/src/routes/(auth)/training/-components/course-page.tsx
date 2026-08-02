import type {
  TrainingCourseDetailOutput,
  TrainingLectureOutput,
} from "@lindaflor/shared/schemas/training";
import { useMutation, useQuery } from "@tanstack/react-query";
import { Link, getRouteApi } from "@tanstack/react-router";
import {
  ArrowLeft,
  Award,
  BookOpen,
  CheckCircle,
  CircleCheck,
  Download,
  ExternalLink,
  FileText,
  Lock,
  PanelRightOpen,
  PlayCircle,
  UserPlus,
} from "lucide-react";
import { useMemo, useState } from "react";

import { Button, buttonVariants } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Sheet, SheetContent, SheetTrigger } from "@/components/ui/sheet";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { orpc } from "@/lib/orpc";
import { cn } from "@/lib/utils";
import {
  CourseSidebar,
  CourseSidebarContent,
} from "@/routes/(auth)/training/-components/course-sidebar";
import { QuizForm } from "@/routes/(auth)/training/-components/quiz-form";
import { YouTubePlayer } from "@/routes/(auth)/training/-components/youtube-player";

const routeApi = getRouteApi("/(auth)/training/courses/$id");

const padNumber = (value: number) => String(value + 1).padStart(2, "0");

type FlatLecture = TrainingLectureOutput & {
  progress: { status: "not_started" | "in_progress" | "completed" } | null;
  quiz:
    | {
        id: string;
        title: string;
        description?: string | null;
        passing_score: number;
        questions: { id: string; text: string }[];
      }
    | undefined;
  latest_attempt: {
    id: string;
    score: number;
    passed: boolean;
  } | null;
};

type LectureLocation = {
  sectionIndex: number;
  lectureIndex: number;
  sectionTitle: string;
  moduleTitle: string;
  moduleDescription?: string;
};

function findLectureLocation(
  sections: TrainingCourseDetailOutput["sections"],
  lectureId: string,
): LectureLocation | null {
  for (const [sectionIndex, section] of sections.entries()) {
    for (const module of section.modules) {
      for (const [lectureIndex, lecture] of module.lectures.entries()) {
        if (lecture.id === lectureId) {
          return {
            sectionIndex,
            lectureIndex,
            sectionTitle: section.title,
            moduleTitle: module.title,
            moduleDescription: module.description ?? undefined,
          };
        }
      }
    }
  }
  return null;
}

export function TrainingCoursePage() {
  const { id: courseId } = routeApi.useParams();
  const loaderData = routeApi.useLoaderData();
  const queryOptions =
    loaderData.data ??
    orpc.training.v1.courses.get.queryOptions({
      input: { id: courseId },
    });
  const { data, refetch, isLoading } = useQuery(queryOptions);
  const [selectedLectureId, setSelectedLectureId] = useState<string | null>(
    null,
  );
  const [sidebarOpen, setSidebarOpen] = useState(false);

  const markLectureProgress = useMutation(
    orpc.training.v1.lectures.progress.mark.mutationOptions({
      onSuccess: () => {
        void refetch();
      },
    }),
  );

  const selfEnroll = useMutation(
    orpc.training.v1.enrollments.selfEnroll.mutationOptions({
      onSuccess: () => {
        void refetch();
      },
    }),
  );

  const submitQuizAttempt = useMutation(
    orpc.training.v1.quizzes.attempts.submit.mutationOptions({
      onSuccess: () => {
        void refetch();
      },
    }),
  );

  const allLectures = useMemo(
    () =>
      data?.sections.flatMap((section) =>
        section.modules.flatMap((module) => module.lectures),
      ) ?? [],
    [data?.sections],
  );

  const selectedLecture = useMemo(() => {
    if (!allLectures.length) return null;
    return (
      allLectures.find((lecture) => lecture.id === selectedLectureId) ??
      allLectures[0]
    );
  }, [allLectures, selectedLectureId]);

  const selectedLocation =
    data && selectedLecture
      ? findLectureLocation(data.sections, selectedLecture.id)
      : null;

  const completedCount = useMemo(
    () =>
      allLectures.filter((lecture) => lecture.progress?.status === "completed")
        .length,
    [allLectures],
  );

  const allCompleted =
    allLectures.length > 0 && completedCount === allLectures.length;

  const handleMarkComplete = () => {
    if (
      !selectedLecture ||
      selectedLecture.progress?.status === "completed" ||
      markLectureProgress.isPending
    ) {
      return;
    }

    markLectureProgress.mutate({
      lecture_id: selectedLecture.id,
      status: "completed",
    });
  };

  if (isLoading) {
    return <TrainingCourseSkeleton />;
  }

  if (!data) {
    return (
      <div className="mx-auto w-full max-w-3xl px-4 py-16 text-center">
        <BookOpen className="mx-auto mb-4 size-12 text-muted-foreground" />
        <h1 className="text-xl font-bold">Curso não encontrado</h1>
        <p className="mt-2 text-sm text-muted-foreground">
          O curso solicitado não está disponível.
        </p>
        <Link
          to="/training"
          search={{ pageIndex: 1, pageSize: 12 }}
          className={cn(buttonVariants({ variant: "outline" }), "mt-4")}
        >
          Voltar para cursos
        </Link>
      </div>
    );
  }

  const course = data;
  const isEnrolled = course.enrolled;
  const lectureCompleted = selectedLecture?.progress?.status === "completed";
  const quizPassed = selectedLecture?.latest_attempt?.passed === true;
  const overviewDescription =
    selectedLocation?.moduleDescription ?? course.description ?? undefined;
  const objectives =
    selectedLecture?.quiz?.questions.map((question) => ({
      id: question.id,
      text: question.text,
    })) ?? [];

  return (
    <Sheet open={sidebarOpen} onOpenChange={setSidebarOpen}>
      <div className="flex flex-col lg:grid lg:h-full lg:grid-cols-[1fr_380px] lg:grid-rows-1">
        <main className="lg:h-full lg:overflow-y-auto">
          <div className="mx-auto w-full max-w-4xl space-y-5 p-4 sm:p-6 lg:p-8">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <Link
                to="/training"
                search={{ pageIndex: 1, pageSize: 12 }}
                className={cn(
                  buttonVariants({ variant: "ghost", size: "sm" }),
                  "inline-flex items-center",
                )}
              >
                <ArrowLeft className="mr-1 size-4" />
                Voltar para cursos
              </Link>
              <div className="flex items-center gap-2">
                <SheetTrigger
                  render={(props) => (
                    <Button
                      variant="outline"
                      size="sm"
                      className="lg:hidden"
                      {...props}
                    >
                      <PanelRightOpen className="mr-1 size-4" />
                      Conteúdo
                    </Button>
                  )}
                />
                {allCompleted ? (
                  <Link
                    to="/training/courses/$id/certificate"
                    params={{ id: courseId }}
                    className={cn(
                      buttonVariants({ variant: "outline", size: "sm" }),
                      "inline-flex items-center",
                    )}
                  >
                    <Award className="mr-1 size-4" />
                    Certificado
                  </Link>
                ) : null}
              </div>
            </div>

            {selectedLecture && selectedLocation ? (
              <LectureHero
                lecture={selectedLecture}
                completed={lectureCompleted}
                onComplete={handleMarkComplete}
                isPending={markLectureProgress.isPending}
                isEnrolled={isEnrolled}
                onEnroll={() => selfEnroll.mutate({ course_id: courseId })}
                isEnrolling={selfEnroll.isPending}
              />
            ) : (
              <Card>
                <CardContent className="py-12 text-center">
                  <BookOpen className="mx-auto mb-3 size-10 text-muted-foreground" />
                  <h3 className="font-medium">Nenhuma aula disponível</h3>
                  <p className="text-sm text-muted-foreground">
                    Este curso ainda não possui conteúdo.
                  </p>
                </CardContent>
              </Card>
            )}

            {selectedLecture && selectedLocation ? (
              <div className="space-y-1">
                <div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                  <span>Seção {padNumber(selectedLocation.sectionIndex)}</span>
                  <span aria-hidden>•</span>
                  <span>{selectedLocation.moduleTitle}</span>
                  <span aria-hidden>•</span>
                  <span>Aula {padNumber(selectedLocation.lectureIndex)}</span>
                </div>
                <h1 className="text-2xl font-bold tracking-tight">
                  {selectedLecture.title}
                </h1>
                {lectureCompleted ? (
                  <p className="inline-flex items-center gap-1 text-sm font-medium text-success">
                    <CircleCheck className="size-4" />
                    Aula concluída
                    {selectedLecture.latest_attempt
                      ? selectedLecture.latest_attempt.passed
                        ? " • quiz aprovado"
                        : " • quiz reprovado"
                      : ""}
                  </p>
                ) : (
                  <p className="text-sm text-muted-foreground">
                    {lectureTypeHint(selectedLecture.type)}
                  </p>
                )}
              </div>
            ) : null}

            {selectedLecture ? (
              <Tabs defaultValue="overview" className="w-full">
                <TabsList variant="line" className="w-full justify-start">
                  <TabsTrigger value="overview">Visão geral</TabsTrigger>
                  <TabsTrigger value="quiz">Quiz</TabsTrigger>
                </TabsList>

                <TabsContent value="overview" className="space-y-4 pt-4">
                  {overviewDescription ? (
                    <p className="text-sm leading-relaxed text-muted-foreground">
                      {overviewDescription}
                    </p>
                  ) : (
                    <p className="text-sm text-muted-foreground">
                      Nenhuma descrição disponível para esta aula.
                    </p>
                  )}

                  {objectives.length > 0 ? (
                    <div className="space-y-2">
                      <h3 className="text-sm font-semibold">
                        O que você vai aprender
                      </h3>
                      <ul className="space-y-1.5">
                        {objectives.map((objective) => (
                          <li
                            key={objective.id}
                            className="flex items-start gap-2 text-sm"
                          >
                            {quizPassed || lectureCompleted ? (
                              <CircleCheck className="mt-0.5 size-4 shrink-0 text-success" />
                            ) : (
                              <CheckCircle className="mt-0.5 size-4 shrink-0 text-muted-foreground/50" />
                            )}
                            <span className="text-muted-foreground">
                              {objective.text}
                            </span>
                          </li>
                        ))}
                      </ul>
                    </div>
                  ) : null}
                </TabsContent>

                <TabsContent value="quiz" className="pt-4">
                  {selectedLecture.quiz && lectureCompleted ? (
                    <QuizForm
                      key={`${selectedLecture.quiz.id}-${selectedLecture.latest_attempt?.id ?? "new"}`}
                      quiz={selectedLecture.quiz}
                      latestAttempt={selectedLecture.latest_attempt}
                      onSubmit={(answers) => {
                        const quiz = selectedLecture.quiz;
                        if (!quiz) return;
                        submitQuizAttempt.mutate({
                          quiz_id: quiz.id,
                          answers,
                        });
                      }}
                      isPending={submitQuizAttempt.isPending}
                    />
                  ) : selectedLecture.quiz ? (
                    <Card>
                      <CardContent className="flex flex-col items-center justify-center gap-2 py-10 text-center">
                        <Lock className="size-6 text-muted-foreground" />
                        <h3 className="font-medium">Quiz bloqueado</h3>
                        <p className="max-w-sm text-sm text-muted-foreground">
                          Conclua a aula para liberar o quiz.
                        </p>
                      </CardContent>
                    </Card>
                  ) : (
                    <Card>
                      <CardContent className="flex flex-col items-center justify-center gap-2 py-10 text-center">
                        <BookOpen className="size-6 text-muted-foreground" />
                        <h3 className="font-medium">Sem quiz</h3>
                        <p className="text-sm text-muted-foreground">
                          Esta aula não possui quiz avaliativo.
                        </p>
                      </CardContent>
                    </Card>
                  )}
                </TabsContent>
              </Tabs>
            ) : null}

            {allCompleted && !allLectures.some((lecture) => lecture.quiz) ? (
              <Card>
                <CardContent className="py-8 text-center">
                  <CheckCircle className="mx-auto mb-3 size-10 text-success" />
                  <h3 className="font-medium">Curso concluído</h3>
                  <p className="text-sm text-muted-foreground">
                    Parabéns! Você concluiu todas as aulas deste curso.
                  </p>
                </CardContent>
              </Card>
            ) : null}
          </div>
        </main>

        <CourseSidebar
          course={course}
          selectedLectureId={selectedLecture?.id ?? null}
          onSelectLecture={setSelectedLectureId}
        />
      </div>
      <SheetContent
        side="right"
        className="flex w-87.5 flex-col p-0 sm:max-w-sm"
      >
        <CourseSidebarContent
          course={course}
          selectedLectureId={selectedLecture?.id ?? null}
          onSelectLecture={(id) => {
            setSelectedLectureId(id);
            setSidebarOpen(false);
          }}
        />
      </SheetContent>
    </Sheet>
  );
}

function lectureTypeHint(type: "video" | "pdf" | "link") {
  if (type === "video") return "Assista ao vídeo para concluir a aula.";
  if (type === "pdf") return "Leia o material e marque como concluída.";
  return "Acesse o link e marque como concluída.";
}

type LectureHeroProps = {
  lecture: FlatLecture;
  completed: boolean;
  onComplete: () => void;
  isPending: boolean;
  isEnrolled: boolean;
  onEnroll: () => void;
  isEnrolling: boolean;
};

function LectureHero({
  lecture,
  completed,
  onComplete,
  isPending,
  isEnrolled,
  onEnroll,
  isEnrolling,
}: LectureHeroProps) {
  if (lecture.type === "video") {
    if (!lecture.youtube_url) {
      return (
        <Card>
          <CardContent className="flex flex-col items-center justify-center gap-2 py-12 text-center">
            <FileText className="size-8 text-muted-foreground" />
            <p className="text-sm text-destructive">
              URL do YouTube inválida. Atualize a aula no gerenciamento.
            </p>
          </CardContent>
        </Card>
      );
    }
    return (
      <Card className="overflow-hidden pt-0">
        <YouTubePlayer
          youtubeUrl={lecture.youtube_url}
          onComplete={isEnrolled ? onComplete : () => {}}
          isEnrolled={isEnrolled}
          isEnrolling={isEnrolling}
          onEnroll={onEnroll}
        />
      </Card>
    );
  }

  if (lecture.type === "link") {
    return (
      <Card className="overflow-hidden">
        <CardContent className="flex flex-col items-center justify-center gap-4 py-12 text-center">
          <div className="flex size-12 items-center justify-center rounded-full bg-muted">
            <ExternalLink className="size-6 text-muted-foreground" />
          </div>
          <div className="space-y-1">
            <h3 className="font-medium">Link externo</h3>
            {lecture.url ? (
              <a
                href={lecture.url}
                target="_blank"
                rel="noopener noreferrer"
                className={cn(
                  buttonVariants({ variant: "default" }),
                  "inline-flex items-center",
                )}
              >
                <ExternalLink className="mr-1 size-4" />
                Abrir link
              </a>
            ) : (
              <p className="text-sm text-muted-foreground">
                URL não informada.
              </p>
            )}
          </div>
          {!isEnrolled ? (
            <Button onClick={onEnroll} disabled={isEnrolling} size="sm">
              <UserPlus />
              {isEnrolling ? "Inscrevendo…" : "Inscrever-se"}
            </Button>
          ) : !completed ? (
            <Button
              onClick={onComplete}
              disabled={isPending}
              variant="outline"
              size="sm"
            >
              <PlayCircle />
              Marcar como concluída
            </Button>
          ) : (
            <CompletedBadge />
          )}
        </CardContent>
      </Card>
    );
  }

  return (
    <Card className="overflow-hidden">
      <CardContent className="flex flex-col items-center justify-center gap-4 py-12 text-center">
        <div className="flex size-12 items-center justify-center rounded-full bg-muted">
          <FileText className="size-6 text-muted-foreground" />
        </div>
        <div className="space-y-1">
          <h3 className="font-medium">Material em PDF</h3>
          {lecture.file_name ? (
            <p className="text-sm text-muted-foreground">{lecture.file_name}</p>
          ) : lecture.file_key ? (
            <p className="text-sm text-muted-foreground">
              Arquivo disponível para download.
            </p>
          ) : (
            <p className="text-sm text-muted-foreground">
              Arquivo não informado.
            </p>
          )}
        </div>
        {isEnrolled && lecture.file_key ? (
          <DownloadPdfButton lectureId={lecture.id} />
        ) : null}
        {!isEnrolled ? (
          <Button onClick={onEnroll} disabled={isEnrolling} size="sm">
            <UserPlus />
            {isEnrolling ? "Inscrevendo…" : "Inscrever-se"}
          </Button>
        ) : !completed ? (
          <Button
            onClick={onComplete}
            disabled={isPending}
            variant="outline"
            size="sm"
          >
            <PlayCircle />
            Marcar como concluída
          </Button>
        ) : (
          <CompletedBadge />
        )}
      </CardContent>
    </Card>
  );
}

function DownloadPdfButton({ lectureId }: { lectureId: string }) {
  const { refetch, isFetching } = useQuery(
    orpc.training.v1.lectures.pdf.download.queryOptions({
      input: { lecture_id: lectureId },
      enabled: false,
    }),
  );

  const handleOpen = async () => {
    const result = await refetch();
    if (result.data?.url) {
      window.open(result.data.url, "_blank", "noopener");
    }
  };

  return (
    <Button onClick={handleOpen} disabled={isFetching}>
      <Download className="mr-1 size-4" />
      {isFetching ? "Abrindo…" : "Abrir material"}
    </Button>
  );
}

function CompletedBadge() {
  return (
    <span className="inline-flex items-center gap-1 text-sm font-medium text-success">
      <CircleCheck className="size-4" />
      Aula concluída
    </span>
  );
}

function TrainingCourseSkeleton() {
  return (
    <div className="flex flex-col lg:grid lg:h-full lg:grid-cols-[1fr_380px] lg:grid-rows-1">
      <main className="lg:h-full lg:overflow-y-auto">
        <div className="mx-auto max-w-4xl space-y-5 p-4 sm:p-6 lg:p-8">
          <Skeleton className="h-8 w-40" />
          <Skeleton className="aspect-video w-full rounded-xl" />
          <div className="space-y-2">
            <Skeleton className="h-4 w-48" />
            <Skeleton className="h-7 w-2/3" />
            <Skeleton className="h-4 w-1/2" />
          </div>
        </div>
      </main>
      <aside className="hidden border-l bg-card px-5 py-4 lg:block">
        <Skeleton className="mb-2 h-4 w-32" />
        <Skeleton className="mb-4 h-3 w-40" />
        <Skeleton className="mb-6 h-2 w-full" />
        <div className="space-y-2">
          <Skeleton className="h-12 w-full" />
          <Skeleton className="h-12 w-full" />
          <Skeleton className="h-12 w-full" />
        </div>
      </aside>
    </div>
  );
}
