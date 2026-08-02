import type { ErrorComponentProps } from "@tanstack/react-router";
import { AlertCircle, ArrowLeft, RotateCcw } from "lucide-react";
import React from "react";

import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";

function handleBack() {
  window.history.back();
}

function handleRetry() {
  window.location.reload();
}

export function ErrorComponent({ error, reset, info }: ErrorComponentProps) {
  const message =
    typeof error === "string"
      ? error
      : error instanceof Error
        ? error.message
        : "Ocorreu um erro inesperado.";

  return (
    <div className="flex min-h-[50vh] items-center justify-center p-4">
      <Card className="w-full max-w-md">
        <CardHeader className="gap-4">
          <div className="flex items-center gap-3">
            <div className="flex size-10 items-center justify-center rounded-full bg-destructive/10">
              <AlertCircle className="size-5 text-destructive" />
            </div>
            <div>
              <CardTitle>Algo deu errado</CardTitle>
              <CardDescription>
                Não foi possível carregar esta página.
              </CardDescription>
            </div>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-sm text-muted-foreground">{message}</p>
          {((error instanceof Error && error.stack != null) ||
            info?.componentStack != null) && (
            <details className="rounded-lg border bg-muted/50">
              <summary className="cursor-pointer px-4 py-2 text-sm font-medium">
                Detalhes técnicos
              </summary>
              <pre className="max-h-48 overflow-auto p-4 text-xs text-muted-foreground">
                {error instanceof Error && error.stack != null
                  ? error.stack
                  : info?.componentStack}
              </pre>
            </details>
          )}
        </CardContent>
        <CardFooter className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end">
          <Button
            variant="outline"
            onClick={handleBack}
            className="w-full sm:w-auto"
          >
            <ArrowLeft className="mr-2 size-4" />
            Voltar
          </Button>
          <Button
            onClick={() => {
              reset();
              handleRetry();
            }}
            className="w-full sm:w-auto"
          >
            <RotateCcw className="mr-2 size-4" />
            Tentar novamente
          </Button>
        </CardFooter>
      </Card>
    </div>
  );
}
