import { useMutation, useQuery } from "@tanstack/react-query";
import { Link, getRouteApi } from "@tanstack/react-router";
import {
  ArrowLeft,
  Calendar,
  FileText,
  Mail,
  Phone,
  Trash2,
} from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

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
import { Avatar, AvatarFallback } from "@/components/ui/avatar";
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
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/skeleton";
import { Spinner } from "@/components/ui/spinner";
import { Time } from "@/components/ui/time";
import { orpc, queryClient } from "@/lib/orpc";
import { cn } from "@/lib/utils";

const routeApi = getRouteApi("/(auth)/curriculum/$id");

function getInitials(name: string): string {
  const words = name.trim().split(/\s+/);
  const first = words[0]?.[0] ?? "";
  const second = words[1]?.[0] ?? words[0]?.[1] ?? "";
  return `${first}${second}`.toUpperCase();
}

function formatFileSize(bytes: number): string {
  const units = ["B", "KB", "MB", "GB"];
  let size = bytes;
  let unitIndex = 0;
  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex++;
  }
  return `${size.toLocaleString("pt-BR", { maximumFractionDigits: 1 })} ${units[unitIndex]}`;
}

export function CurriculumDetailPage() {
  const queryOptions = routeApi.useLoaderData({ select: (data) => data.data });
  const { data: curriculum, isLoading } = useQuery(queryOptions);
  const { data: downloadData } = useQuery(
    orpc.curriculum.v1.getDownloadUrl.queryOptions({
      input: { id: curriculum?.id ?? "" },
      enabled: !!curriculum?.id,
    }),
  );

  if (isLoading) {
    return <CurriculumDetailSkeleton />;
  }

  if (!curriculum) {
    return (
      <div className="mx-auto w-full max-w-6xl px-4 py-8 sm:px-6 lg:px-8">
        <div className="mb-6">
          <Link
            to="/curriculum"
            search={{ pageIndex: 1 }}
            className={cn(
              buttonVariants({ variant: "ghost", size: "sm" }),
              "pl-0",
            )}
          >
            <ArrowLeft className="mr-1 size-4" />
            Voltar para currículos
          </Link>
        </div>
        <Empty className="py-16">
          <EmptyHeader>
            <EmptyMedia variant="icon">
              <FileText className="size-5" />
            </EmptyMedia>
            <EmptyTitle>Currículo não encontrado</EmptyTitle>
            <EmptyDescription>
              O currículo solicitado não existe ou não está mais disponível.
            </EmptyDescription>
          </EmptyHeader>
          <Link
            to="/curriculum"
            search={{ pageIndex: 1 }}
            className={cn(buttonVariants({ variant: "outline" }), "mt-2")}
          >
            Voltar para currículos
          </Link>
        </Empty>
      </div>
    );
  }

  return (
    <div className="mx-auto w-full max-w-6xl px-4 py-8 sm:px-6 lg:px-8">
      <div className="mb-6 flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div className="flex items-start gap-4">
          <Avatar size="lg" className="size-14 text-lg">
            <AvatarFallback>{getInitials(curriculum.name)}</AvatarFallback>
          </Avatar>
          <div>
            <h1 className="text-2xl font-bold tracking-tight">
              {curriculum.name}
            </h1>
            <p className="text-muted-foreground">{curriculum.headline}</p>
            <Badge variant="secondary" className="mt-2">
              <Calendar className="size-3" />
              Enviado em <Time date={curriculum.submitted_at} />
            </Badge>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <DeleteCurriculumDialog
            id={curriculum.id}
            fileName={curriculum.file_name}
          />
        </div>
      </div>

      <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.25fr)]">
        <div className="space-y-6">
          <Card>
            <CardHeader>
              <CardTitle>Contato</CardTitle>
            </CardHeader>
            <CardContent className="space-y-3">
              <a
                href={`mailto:${curriculum.email}`}
                className="flex items-center gap-2 text-sm text-muted-foreground transition-colors hover:text-foreground"
              >
                <Mail className="size-4 text-muted-foreground" />
                {curriculum.email}
              </a>
              {curriculum.phone && (
                <a
                  href={`tel:${curriculum.phone}`}
                  className="flex items-center gap-2 text-sm text-muted-foreground transition-colors hover:text-foreground"
                >
                  <Phone className="size-4 text-muted-foreground" />
                  {curriculum.phone}
                </a>
              )}
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <Calendar className="size-4" />
                Enviado em <Time date={curriculum.submitted_at} />
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>Habilidades</CardTitle>
            </CardHeader>
            <CardContent>
              {curriculum.skills.length > 0 ? (
                <div className="flex flex-wrap gap-2">
                  {curriculum.skills.map((skill) => (
                    <Badge key={skill} variant="secondary">
                      {skill}
                    </Badge>
                  ))}
                </div>
              ) : (
                <p className="text-sm text-muted-foreground">
                  Nenhuma habilidade informada.
                </p>
              )}
            </CardContent>
          </Card>

          {curriculum.summary && (
            <Card>
              <CardHeader>
                <CardTitle>Resumo profissional</CardTitle>
              </CardHeader>
              <CardContent>
                <p className="whitespace-pre-wrap text-sm leading-relaxed text-muted-foreground">
                  {curriculum.summary}
                </p>
              </CardContent>
            </Card>
          )}
        </div>

        <Card>
          <CardHeader>
            <CardTitle>Currículo em PDF</CardTitle>
            <CardDescription className="flex items-center gap-2">
              <FileText className="size-4" />
              {curriculum.file_name} · {formatFileSize(curriculum.file_size)}
            </CardDescription>
          </CardHeader>
          <CardContent>
            {downloadData?.url ? (
              <div className="overflow-hidden rounded-lg border">
                <iframe
                  src={downloadData.url}
                  title={curriculum.file_name}
                  className="min-h-96 w-full"
                  sandbox="allow-scripts"
                />
              </div>
            ) : (
              <div className="flex min-h-96 flex-col items-center justify-center gap-2 rounded-lg border bg-muted/30 text-muted-foreground">
                <Spinner className="size-6" />
                <p className="text-sm">Carregando visualização…</p>
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

function DeleteCurriculumDialog({
  id,
  fileName,
}: {
  id: string;
  fileName: string;
}) {
  const [open, setOpen] = useState(false);
  const navigate = routeApi.useNavigate();
  const mutation = useMutation(
    orpc.curriculum.v1.delete.mutationOptions({
      onSuccess: async () => {
        toast.success("Currículo excluído com sucesso");
        await queryClient.invalidateQueries({
          queryKey: orpc.curriculum.v1.list.key(),
        });
        setOpen(false);
        void navigate({ to: "/curriculum", search: { pageIndex: 1 } });
      },
      onError: (error) => {
        toast.error(error.message);
      },
    }),
  );

  return (
    <AlertDialog open={open} onOpenChange={setOpen}>
      <AlertDialogTrigger
        render={(props) => (
          <Button variant="destructive" size="sm" {...props}>
            <Trash2 className="size-4" />
            Excluir
          </Button>
        )}
      />
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Excluir currículo?</AlertDialogTitle>
          <AlertDialogDescription>
            Tem certeza que deseja excluir o currículo{" "}
            <span className="font-medium text-foreground">{fileName}</span>?
            Esta ação não pode ser desfeita.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancelar</AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            disabled={mutation.isPending}
            onClick={() => mutation.mutate({ id })}
          >
            {mutation.isPending && <Spinner className="mr-1 size-4" />}
            Excluir
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

function CurriculumDetailSkeleton() {
  return (
    <div className="mx-auto w-full max-w-6xl px-4 py-8 sm:px-6 lg:px-8">
      <div className="mb-6 flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div className="flex items-start gap-4">
          <Skeleton className="size-14 rounded-full" />
          <div className="space-y-2">
            <Skeleton className="h-8 w-64" />
            <Skeleton className="h-5 w-48" />
            <Skeleton className="h-5 w-32" />
          </div>
        </div>
        <div className="flex items-center gap-2">
          <Skeleton className="h-8 w-28" />
          <Skeleton className="h-8 w-24" />
        </div>
      </div>
      <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.25fr)]">
        <div className="space-y-6">
          <Card>
            <CardHeader>
              <Skeleton className="h-5 w-20" />
            </CardHeader>
            <CardContent className="space-y-3">
              <Skeleton className="h-4 w-full" />
              <Skeleton className="h-4 w-2/3" />
              <Skeleton className="h-4 w-1/2" />
            </CardContent>
          </Card>
          <Card>
            <CardHeader>
              <Skeleton className="h-5 w-24" />
            </CardHeader>
            <CardContent>
              <div className="flex flex-wrap gap-2">
                <Skeleton className="h-5 w-16" />
                <Skeleton className="h-5 w-20" />
                <Skeleton className="h-5 w-14" />
              </div>
            </CardContent>
          </Card>
        </div>
        <Card>
          <CardHeader>
            <Skeleton className="h-5 w-32" />
            <Skeleton className="h-4 w-48" />
          </CardHeader>
          <CardContent>
            <Skeleton className="min-h-96 w-full" />
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
