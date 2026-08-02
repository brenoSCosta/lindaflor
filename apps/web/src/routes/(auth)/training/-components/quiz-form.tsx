import type {
  TrainingQuizAttemptOutput,
  TrainingQuizOutput,
} from "@lindaflor/shared/schemas/training";
import { CheckCircle, XCircle } from "lucide-react";
import { useMemo, useState } from "react";
import { z } from "zod";

import { useAppForm } from "@/components/form/hooks";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { cn } from "@/lib/utils";

type QuizFormProps = {
  quiz: TrainingQuizOutput;
  latestAttempt: TrainingQuizAttemptOutput | null;
  onSubmit: (answers: Record<string, string>) => void;
  isPending: boolean;
};

export function QuizForm({
  quiz,
  latestAttempt,
  onSubmit,
  isPending,
}: QuizFormProps) {
  const [retryRequested, setRetryRequested] = useState(false);

  const defaultValues = useMemo(
    () =>
      Object.fromEntries(quiz.questions.map((question) => [question.id, ""])),
    [quiz.questions],
  );

  const schema = useMemo(
    () =>
      z.object(
        Object.fromEntries(
          quiz.questions.map((question) => [
            question.id,
            z.string().min(1, "Selecione uma resposta"),
          ]),
        ),
      ),
    [quiz.questions],
  );

  const form = useAppForm({
    defaultValues,
    validators: {
      onSubmit: schema,
    },
    onSubmit: ({ value }) => {
      onSubmit(value);
    },
  });

  if (latestAttempt && !retryRequested) {
    return (
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            {latestAttempt.passed ? (
              <CheckCircle className="size-5 text-success" />
            ) : (
              <XCircle className="size-5 text-destructive" />
            )}
            Resultado do quiz
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <p className="text-sm">
            Pontuação:{" "}
            <span className="font-semibold">{latestAttempt.score}%</span>
          </p>
          <p className="text-sm text-muted-foreground">
            {latestAttempt.passed
              ? "Parabéns, você foi aprovado!"
              : "Você não atingiu a nota mínima necessária."}
          </p>
          <Button
            type="button"
            variant="outline"
            onClick={() => setRetryRequested(true)}
          >
            Tentar novamente
          </Button>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{quiz.title}</CardTitle>
        {quiz.description ? (
          <p className="text-sm text-muted-foreground">{quiz.description}</p>
        ) : null}
      </CardHeader>
      <CardContent>
        <form
          action={async () => {
            await form.handleSubmit();
          }}
        >
          <form.AppForm>
            <div className="space-y-6">
              {quiz.questions.map((question) => (
                <form.AppField key={question.id} name={question.id}>
                  {(field) => {
                    const isInvalid =
                      field.state.meta.isTouched && !field.state.meta.isValid;
                    return (
                      <field.Field>
                        <div className="space-y-3">
                          <p className="font-medium">{question.text}</p>
                          <field.RadioGroup>
                            {question.options.map((option) => (
                              <label
                                key={option.id}
                                htmlFor={option.id}
                                className={cn(
                                  "flex cursor-pointer items-center gap-3 rounded-md border p-3 transition-colors hover:bg-muted/50",
                                  isInvalid && "border-destructive",
                                )}
                              >
                                <field.RadioGroupItem
                                  value={option.id}
                                  id={option.id}
                                />
                                <span className="text-sm">{option.text}</span>
                              </label>
                            ))}
                          </field.RadioGroup>
                          <field.Error />
                        </div>
                      </field.Field>
                    );
                  }}
                </form.AppField>
              ))}
              <form.Subscribe>
                {({ canSubmit }) => (
                  <Button type="submit" disabled={!canSubmit || isPending}>
                    {isPending ? "Enviando..." : "Enviar respostas"}
                  </Button>
                )}
              </form.Subscribe>
            </div>
          </form.AppForm>
        </form>
      </CardContent>
    </Card>
  );
}
