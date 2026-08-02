import { useQuery } from "@tanstack/react-query";
import { Link, getRouteApi } from "@tanstack/react-router";
import { Award, BookOpen, Download, ShieldCheck } from "lucide-react";

import { buttonVariants } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { authClient } from "@/lib/auth-client";
import { orpc } from "@/lib/orpc";
import { cn } from "@/lib/utils";

const routeApi = getRouteApi("/(auth)/training/courses/$id/certificate");

export function TrainingCertificatePage() {
  const { id: courseId } = routeApi.useParams();
  const { data: session } = authClient.useSession();

  const { data, isLoading } = useQuery(
    orpc.training.v1.courses.certificate.get.queryOptions({
      input: { id: courseId },
    }),
  );

  if (isLoading) {
    return (
      <main className="mx-auto w-full max-w-3xl px-4 py-8 sm:px-6 lg:px-8">
        <Skeleton className="mb-6 h-8 w-48" />
        <Skeleton className="h-80 w-full" />
      </main>
    );
  }

  if (!data) {
    return (
      <main className="mx-auto w-full max-w-3xl px-4 py-16 text-center">
        <BookOpen className="mx-auto mb-4 size-12 text-muted-foreground" />
        <h1 className="text-xl font-bold">Certificado não encontrado</h1>
        <Link
          to="/training"
          search={{ pageIndex: 1, pageSize: 12 }}
          className={cn(buttonVariants({ variant: "outline" }), "mt-4")}
        >
          Voltar para cursos
        </Link>
      </main>
    );
  }

  const { course, completed_at, is_completed } = data;
  const recipientName = session?.user?.name || session?.user?.email || null;
  const formattedDate = completed_at
    ? completed_at.toLocaleDateString("pt-BR", {
        day: "2-digit",
        month: "long",
        year: "numeric",
      })
    : null;
  const certificateId = `${course.id.slice(0, 8).toUpperCase()}-${completed_at ? completed_at.getFullYear() : ""}`;

  return (
    <main className="mx-auto w-full max-w-3xl px-4 py-8 sm:px-6 lg:px-8">
      <div className="mb-6 flex items-center justify-between">
        <Link
          to="/training/courses/$id"
          params={{ id: courseId }}
          className={cn(
            buttonVariants({ variant: "ghost", size: "sm" }),
            "inline-flex items-center",
          )}
        >
          Voltar para o curso
        </Link>
        {is_completed ? (
          <button
            type="button"
            onClick={() => window.print()}
            className={cn(
              buttonVariants({ variant: "outline", size: "sm" }),
              "inline-flex items-center",
            )}
          >
            <Download className="mr-1 size-4" />
            Imprimir
          </button>
        ) : null}
      </div>

      <Card className="overflow-hidden border-2 p-0">
        <div className="h-2 w-full bg-linear-to-r from-primary via-secondary to-primary" />
        <CardContent className="p-8 sm:p-12">
          {is_completed ? (
            <div className="flex flex-col items-center gap-6 text-center">
              <div className="flex size-20 items-center justify-center rounded-full bg-primary/10">
                <Award className="size-10 text-primary" />
              </div>

              <div className="space-y-1">
                <p className="text-xs font-semibold uppercase tracking-[0.2em] text-muted-foreground">
                  Certificado de conclusão
                </p>
                <h1 className="font-display text-3xl font-bold tracking-tight">
                  {course.title}
                </h1>
              </div>

              <div className="space-y-1">
                <p className="text-sm text-muted-foreground">
                  Este certificado atesta que
                </p>
                <p className="font-display text-xl font-semibold">
                  {recipientName ?? "Participante"}
                </p>
                <p className="text-sm text-muted-foreground">
                  concluiu com êxito todas as aulas e avaliações deste curso.
                </p>
              </div>

              {course.description ? (
                <p className="max-w-xl text-sm leading-relaxed text-muted-foreground">
                  {course.description}
                </p>
              ) : null}

              <div className="mt-4 flex w-full max-w-md flex-col items-center gap-6 border-t pt-8 sm:flex-row sm:justify-between sm:gap-0">
                <div className="space-y-1 text-center sm:text-left">
                  <p className="text-xs uppercase tracking-wide text-muted-foreground">
                    Conclusão
                  </p>
                  <p className="text-sm font-medium">{formattedDate ?? "—"}</p>
                </div>
                <div className="flex flex-col items-center gap-2">
                  <ShieldCheck className="size-6 text-primary" />
                  <div className="text-center">
                    <p className="text-xs font-semibold uppercase tracking-wide">
                      Verificação
                    </p>
                    <p className="text-xs text-muted-foreground">
                      {certificateId}
                    </p>
                  </div>
                </div>
                <div className="space-y-1 text-center sm:text-right">
                  <p className="text-xs uppercase tracking-wide text-muted-foreground">
                    Assinatura
                  </p>
                  <p className="text-sm font-medium">Coordenação</p>
                </div>
              </div>
            </div>
          ) : (
            <div className="flex flex-col items-center gap-4 py-8 text-center">
              <div className="flex size-16 items-center justify-center rounded-full bg-muted">
                <Award className="size-8 text-muted-foreground" />
              </div>
              <h2 className="text-lg font-medium">Certificado bloqueado</h2>
              <p className="max-w-sm text-sm text-muted-foreground">
                Conclua todas as aulas e os quizzes para liberar o certificado
                deste curso.
              </p>
              <Link
                to="/training/courses/$id"
                params={{ id: courseId }}
                className={cn(buttonVariants({ size: "sm" }), "mt-2")}
              >
                Continuar curso
              </Link>
            </div>
          )}
        </CardContent>
      </Card>
    </main>
  );
}
