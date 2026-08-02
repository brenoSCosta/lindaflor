import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { Link, getRouteApi } from "@tanstack/react-router";
import { FileText, Plus, X } from "lucide-react";
import React from "react";
import { useDebounceCallback } from "usehooks-ts";

import { Button, buttonVariants } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  InputGroup,
  InputGroupButton,
  InputGroupInput,
} from "@/components/ui/input-group";
import { Skeleton } from "@/components/ui/skeleton";
import { orpc } from "@/lib/orpc";
import { cn } from "@/lib/utils";

const routeApi = getRouteApi("/(auth)/curriculum/");

function CurriculumCard({
  curriculum,
}: {
  curriculum: {
    id: string;
    name: string;
    headline: string;
    skills: string[];
    submitted_at: Date;
  };
}) {
  return (
    <Link to="/curriculum/$id" params={{ id: curriculum.id }}>
      <Card className="h-full transition-colors hover:bg-muted/30">
        <CardHeader className="pb-3">
          <CardTitle className="line-clamp-1 text-base">
            {curriculum.name}
          </CardTitle>
          <p className="line-clamp-1 text-sm text-muted-foreground">
            {curriculum.headline}
          </p>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="flex flex-wrap gap-1.5">
            {curriculum.skills.slice(0, 5).map((skill) => (
              <span
                key={skill}
                className="inline-flex rounded-md bg-secondary px-2 py-0.5 text-xs font-medium text-secondary-foreground"
              >
                {skill}
              </span>
            ))}
            {curriculum.skills.length > 5 && (
              <span className="inline-flex rounded-md bg-muted px-2 py-0.5 text-xs text-muted-foreground">
                +{curriculum.skills.length - 5}
              </span>
            )}
          </div>
          <p className="text-xs text-muted-foreground">
            Enviado em{" "}
            {new Date(curriculum.submitted_at).toLocaleDateString("pt-BR")}
          </p>
        </CardContent>
      </Card>
    </Link>
  );
}

function CurriculumSkeleton() {
  return (
    <Card className="h-full">
      <CardHeader className="pb-3">
        <Skeleton className="h-5 w-3/4" />
        <Skeleton className="h-4 w-1/2" />
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="flex flex-wrap gap-1.5">
          <Skeleton className="h-5 w-16" />
          <Skeleton className="h-5 w-20" />
          <Skeleton className="h-5 w-14" />
        </div>
        <Skeleton className="h-4 w-32" />
      </CardContent>
    </Card>
  );
}

export function CurriculumListPage() {
  const { search, pageIndex } = routeApi.useSearch();
  const navigate = routeApi.useNavigate();
  const {
    data: { curriculums = [], meta } = {},
    isLoading,
    isPlaceholderData,
  } = useQuery(
    orpc.curriculum.v1.list.queryOptions({
      input: {
        pagination: { pageIndex, pageSize: 9 },
        search,
      },
      placeholderData: keepPreviousData,
      select: (result) => ({
        curriculums: result.data,
        meta: result.meta,
      }),
    }),
  );

  const [searchValue, setSearchValue] = React.useState(search ?? "");
  const debouncedNavigate = useDebounceCallback((value: string) => {
    void navigate({
      search: { search: value.trim() || undefined, pageIndex: 1 },
    });
  }, 300);

  return (
    <div className="mx-auto w-full max-w-6xl px-4 py-8 sm:px-6 lg:px-8">
      <div className="mb-6 flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div className="space-y-1">
          <h1 className="text-2xl font-bold">Banco de talentos</h1>
          <p className="text-sm text-muted-foreground">
            Encontre profissionais pelos currículos enviados.
          </p>
        </div>
        <Link
          to="/curriculum/submit"
          className={buttonVariants({ variant: "default" })}
        >
          <Plus className="size-4" />
          Enviar currículo
        </Link>
      </div>

      <InputGroup className="w-full md:max-w-sm mb-6">
        <InputGroupInput
          placeholder="Buscar por nome, cargo ou habilidades"
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

      {isLoading && !isPlaceholderData ? (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {[
            "skeleton-a",
            "skeleton-b",
            "skeleton-c",
            "skeleton-d",
            "skeleton-e",
            "skeleton-f",
          ].map((key) => (
            <CurriculumSkeleton key={key} />
          ))}
        </div>
      ) : curriculums.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-xl border bg-muted/30 py-16 text-center">
          <FileText className="mb-3 size-10 text-muted-foreground" />
          <h3 className="text-lg font-medium">Nenhum currículo encontrado</h3>
          <p className="mt-1 max-w-sm text-sm text-muted-foreground">
            {search
              ? "Tente ajustar os termos da busca."
              : "Seja o primeiro a enviar seu currículo."}
          </p>
          {!search && (
            <Link
              to="/curriculum/submit"
              className={cn(buttonVariants({ variant: "default" }), "mt-4")}
            >
              Enviar currículo
            </Link>
          )}
        </div>
      ) : (
        <>
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {curriculums.map((curriculum) => (
              <CurriculumCard key={curriculum.id} curriculum={curriculum} />
            ))}
          </div>

          {meta && meta.totalPages > 1 && (
            <div className="mt-8 flex items-center justify-center gap-2">
              <Button
                variant="outline"
                size="sm"
                disabled={pageIndex <= 1}
                onClick={() =>
                  void navigate({
                    search: { search, pageIndex: pageIndex - 1 },
                  })
                }
              >
                Anterior
              </Button>
              <span className="text-sm text-muted-foreground">
                Página {pageIndex} de {meta.totalPages}
              </span>
              <Button
                variant="outline"
                size="sm"
                disabled={pageIndex >= meta.totalPages}
                onClick={() =>
                  void navigate({
                    search: { search, pageIndex: pageIndex + 1 },
                  })
                }
              >
                Próxima
              </Button>
            </div>
          )}
        </>
      )}
    </div>
  );
}
