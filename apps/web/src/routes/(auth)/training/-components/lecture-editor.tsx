import { useMutation } from "@tanstack/react-query";
import {
  FileText,
  HelpCircle,
  Link as LinkIcon,
  PlayCircle,
  Plus,
  Trash2,
  X,
} from "lucide-react";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import {
  FileUpload,
  FileUploadDescription,
  FileUploadDropzone,
  FileUploadDropzoneIcon,
  FileUploadTitle,
} from "@/components/ui/file-upload";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { orpc } from "@/lib/orpc";
import { cn } from "@/lib/utils";
import type { CourseFormInstance } from "@/routes/(auth)/training/-components/course-form-types";
import {
  isLectureType,
  type LectureType,
} from "@/routes/(auth)/training/-components/training-form-utils";

const padNumber = (value: number) => String(value + 1).padStart(2, "0");

const TYPE_OPTIONS: {
  value: LectureType;
  label: string;
  icon: typeof PlayCircle;
  placeholder: string;
  field: "youtube_url" | "file_key" | "url";
}[] = [
  {
    value: "video",
    label: "Vídeo",
    icon: PlayCircle,
    placeholder: "https://www.youtube.com/watch?v=...",
    field: "youtube_url",
  },
  {
    value: "pdf",
    label: "PDF",
    icon: FileText,
    placeholder: "Chave do arquivo PDF",
    field: "file_key",
  },
  {
    value: "link",
    label: "Link",
    icon: LinkIcon,
    placeholder: "https://exemplo.com/conteudo",
    field: "url",
  },
];

export function LectureEditor({
  form,
  sectionIndex,
  moduleIndex,
  lectureIndex,
  sectionTitle,
  moduleTitle,
}: {
  form: CourseFormInstance;
  sectionIndex: number;
  moduleIndex: number;
  lectureIndex: number;
  sectionTitle: string;
  moduleTitle: string;
}) {
  const rawType = form.getFieldValue(
    `sections[${sectionIndex}].modules[${moduleIndex}].lectures[${lectureIndex}].type`,
  );
  const currentType = isLectureType(rawType) ? rawType : "video";
  const typeMeta =
    TYPE_OPTIONS.find((t) => t.value === currentType) ?? TYPE_OPTIONS[0];

  return (
    <div className="mx-auto max-w-3xl space-y-6">
      <div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
        <span>Seção {padNumber(sectionIndex)}</span>
        <span aria-hidden>•</span>
        <span className="line-clamp-1">{sectionTitle || "Seção"}</span>
        <span aria-hidden>•</span>
        <span className="line-clamp-1">{moduleTitle || "Módulo"}</span>
        <span aria-hidden>•</span>
        <span>Aula {padNumber(lectureIndex)}</span>
      </div>

      <form.AppField
        name={`sections[${sectionIndex}].modules[${moduleIndex}].lectures[${lectureIndex}].title`}
      >
        {(field) => (
          <field.Field>
            <field.Label className="sr-only">Título da aula</field.Label>
            <field.Input
              data-tour="lecture-title"
              placeholder="Título da aula"
            />
            <field.Error />
          </field.Field>
        )}
      </form.AppField>

      <Tabs defaultValue="content" className="w-full">
        <TabsList variant="line" className="w-full justify-start">
          <TabsTrigger value="content">Conteúdo</TabsTrigger>
          <TabsTrigger value="quiz">Quiz</TabsTrigger>
        </TabsList>

        <TabsContent
          value="content"
          data-tour="lecture-content"
          className="space-y-4 pt-4"
        >
          <div className="space-y-1.5">
            <span className="text-xs font-medium text-muted-foreground">
              Tipo de aula
            </span>
            <div className="flex gap-0.5 rounded-lg border bg-muted/50 p-0.5">
              {TYPE_OPTIONS.map((option) => {
                const isActive = currentType === option.value;
                const Icon = option.icon;
                return (
                  <button
                    key={option.value}
                    type="button"
                    onClick={() =>
                      form.setFieldValue(
                        `sections[${sectionIndex}].modules[${moduleIndex}].lectures[${lectureIndex}].type`,
                        option.value,
                      )
                    }
                    aria-pressed={isActive}
                    className={cn(
                      "flex flex-1 items-center justify-center gap-1.5 rounded-md px-3 py-2 text-sm font-medium transition-all",
                      isActive
                        ? "bg-background text-foreground shadow-sm"
                        : "text-muted-foreground hover:text-foreground",
                    )}
                  >
                    <Icon className="size-4" />
                    {option.label}
                  </button>
                );
              })}
            </div>
          </div>

          {currentType === "pdf" ? (
            <PdfUploadField
              form={form}
              sectionIndex={sectionIndex}
              moduleIndex={moduleIndex}
              lectureIndex={lectureIndex}
            />
          ) : (
            <form.AppField
              name={`sections[${sectionIndex}].modules[${moduleIndex}].lectures[${lectureIndex}].${typeMeta.field}`}
            >
              {(field) => (
                <field.Field>
                  <field.Label className="text-xs font-medium text-muted-foreground">
                    {typeMeta.label === "Vídeo"
                      ? "URL do YouTube"
                      : "URL externa"}
                  </field.Label>
                  <div className="relative">
                    <typeMeta.icon className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
                    <field.Input
                      className="pl-9"
                      placeholder={typeMeta.placeholder}
                    />
                  </div>
                  <field.Error />
                </field.Field>
              )}
            </form.AppField>
          )}
        </TabsContent>

        <TabsContent value="quiz" className="pt-4">
          <form.Subscribe
            selector={(state) =>
              state.values.sections[sectionIndex]?.modules[moduleIndex]
                ?.lectures[lectureIndex]?.quiz
            }
          >
            {(quiz) => {
              if (!quiz) {
                return (
                  <Card>
                    <CardContent className="flex flex-col items-center justify-center gap-2 py-10 text-center">
                      <HelpCircle className="size-6 text-muted-foreground" />
                      <h3 className="font-medium">Sem quiz</h3>
                      <p className="max-w-sm text-sm text-muted-foreground">
                        Esta aula não possui quiz avaliativo.
                      </p>
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        className="mt-2"
                        onClick={() => {
                          form.setFieldValue(
                            `sections[${sectionIndex}].modules[${moduleIndex}].lectures[${lectureIndex}].quiz`,
                            {
                              title: "",
                              description: "",
                              passing_score: 70,
                              questions: [],
                            },
                          );
                        }}
                      >
                        <Plus className="mr-1 size-4" />
                        Adicionar quiz
                      </Button>
                    </CardContent>
                  </Card>
                );
              }

              const questionCount = quiz.questions?.length ?? 0;

              return (
                <div data-testid="lecture-quiz" className="space-y-4">
                  <div className="flex items-center justify-between rounded-lg border border-primary/20 bg-primary/5 px-3 py-2">
                    <span className="flex items-center gap-1.5 text-sm font-medium text-primary">
                      <HelpCircle className="size-4" />
                      Quiz avaliativo ({questionCount} perguntas)
                    </span>
                    <button
                      type="button"
                      onClick={() => {
                        form.setFieldValue(
                          `sections[${sectionIndex}].modules[${moduleIndex}].lectures[${lectureIndex}].quiz`,
                          undefined,
                        );
                      }}
                      className="flex items-center gap-1 text-xs font-medium text-destructive transition-colors hover:text-destructive/80"
                    >
                      <X className="size-3" />
                      Remover quiz
                    </button>
                  </div>

                  <div className="space-y-4">
                    <div className="grid grid-cols-1 gap-4 sm:grid-cols-[1fr_auto]">
                      <form.AppField
                        name={`sections[${sectionIndex}].modules[${moduleIndex}].lectures[${lectureIndex}].quiz.title`}
                      >
                        {(field) => (
                          <field.Field>
                            <field.Label>Título do quiz</field.Label>
                            <field.Input
                              className="h-9"
                              placeholder="Título do quiz"
                            />
                            <field.Error />
                          </field.Field>
                        )}
                      </form.AppField>

                      <form.AppField
                        name={`sections[${sectionIndex}].modules[${moduleIndex}].lectures[${lectureIndex}].quiz.passing_score`}
                      >
                        {(field) => (
                          <field.Field>
                            <field.Label>Nota mínima (%)</field.Label>
                            <field.NumberInput
                              className="h-9 w-24"
                              min={0}
                              max={100}
                            />
                            <field.Error />
                          </field.Field>
                        )}
                      </form.AppField>
                    </div>

                    <form.AppField
                      name={`sections[${sectionIndex}].modules[${moduleIndex}].lectures[${lectureIndex}].quiz.description`}
                    >
                      {(field) => (
                        <field.Field>
                          <field.Label>Descrição</field.Label>
                          <field.Textarea
                            className="min-h-16"
                            placeholder="Descrição opcional do quiz"
                          />
                          <field.Error />
                        </field.Field>
                      )}
                    </form.AppField>

                    <form.AppField
                      name={`sections[${sectionIndex}].modules[${moduleIndex}].lectures[${lectureIndex}].quiz.questions`}
                      mode="array"
                    >
                      {(questionsField) => {
                        const questions = questionsField.state.value ?? [];
                        return (
                          <div className="space-y-2">
                            <div className="flex items-center justify-between">
                              <span className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                                Perguntas ({questions.length})
                              </span>
                              <Button
                                type="button"
                                variant="outline"
                                size="sm"
                                className="h-7 text-xs"
                                onClick={() =>
                                  questionsField.pushValue({
                                    text: "",
                                    sort_order: questions.length,
                                    options: [
                                      {
                                        text: "",
                                        is_correct: false,
                                        sort_order: 0,
                                      },
                                      {
                                        text: "",
                                        is_correct: false,
                                        sort_order: 1,
                                      },
                                    ],
                                  })
                                }
                              >
                                <Plus className="mr-1 size-3" />
                                Adicionar pergunta
                              </Button>
                            </div>

                            {questions.map((question, questionIndex) => (
                              <QuizQuestionFields
                                key={question.id ?? questionIndex}
                                form={form}
                                sectionIndex={sectionIndex}
                                moduleIndex={moduleIndex}
                                lectureIndex={lectureIndex}
                                questionIndex={questionIndex}
                                onRemove={() =>
                                  questionsField.removeValue(questionIndex)
                                }
                              />
                            ))}
                          </div>
                        );
                      }}
                    </form.AppField>
                  </div>
                </div>
              );
            }}
          </form.Subscribe>
        </TabsContent>
      </Tabs>
    </div>
  );
}

function QuizQuestionFields({
  form,
  sectionIndex,
  moduleIndex,
  lectureIndex,
  questionIndex,
  onRemove,
}: {
  form: CourseFormInstance;
  sectionIndex: number;
  moduleIndex: number;
  lectureIndex: number;
  questionIndex: number;
  onRemove: () => void;
}) {
  return (
    <div className="space-y-2 rounded-lg border bg-background p-3">
      <div className="flex items-start gap-2">
        <span className="mt-1.5 flex size-6 shrink-0 items-center justify-center rounded bg-muted text-xs font-bold tabular-nums text-muted-foreground">
          {padNumber(questionIndex)}
        </span>
        <form.AppField
          name={`sections[${sectionIndex}].modules[${moduleIndex}].lectures[${lectureIndex}].quiz.questions[${questionIndex}].text`}
        >
          {(field) => (
            <field.Field className="min-w-0 flex-1">
              <field.Label className="sr-only">
                Pergunta {questionIndex + 1}
              </field.Label>
              <field.Input
                className="h-8 border-0 bg-transparent px-0 text-sm font-medium focus-visible:ring-0"
                placeholder="Enunciado da pergunta"
              />
              <field.Error />
            </field.Field>
          )}
        </form.AppField>
        <Button
          variant="ghost"
          size="icon-xs"
          onClick={onRemove}
          aria-label="Remover pergunta"
          className="shrink-0"
        >
          <Trash2 className="size-3.5 text-destructive" />
        </Button>
      </div>

      <form.AppField
        name={`sections[${sectionIndex}].modules[${moduleIndex}].lectures[${lectureIndex}].quiz.questions[${questionIndex}].options`}
        mode="array"
      >
        {(optionsField) => {
          const options = optionsField.state.value ?? [];
          return (
            <div className="ml-8 space-y-1.5">
              {options.map((option, optionIndex) => (
                <QuizOptionFields
                  key={option.id ?? optionIndex}
                  form={form}
                  sectionIndex={sectionIndex}
                  moduleIndex={moduleIndex}
                  lectureIndex={lectureIndex}
                  questionIndex={questionIndex}
                  optionIndex={optionIndex}
                  onRemove={() => optionsField.removeValue(optionIndex)}
                />
              ))}
              <button
                type="button"
                onClick={() =>
                  optionsField.pushValue({
                    text: "",
                    is_correct: false,
                    sort_order: options.length,
                  })
                }
                className="text-xs text-muted-foreground transition-colors hover:text-foreground"
              >
                + Adicionar opção
              </button>
            </div>
          );
        }}
      </form.AppField>
    </div>
  );
}

function QuizOptionFields({
  form,
  sectionIndex,
  moduleIndex,
  lectureIndex,
  questionIndex,
  optionIndex,
  onRemove,
}: {
  form: CourseFormInstance;
  sectionIndex: number;
  moduleIndex: number;
  lectureIndex: number;
  questionIndex: number;
  optionIndex: number;
  onRemove: () => void;
}) {
  return (
    <div className="flex items-center gap-2">
      <form.AppField
        name={`sections[${sectionIndex}].modules[${moduleIndex}].lectures[${lectureIndex}].quiz.questions[${questionIndex}].options[${optionIndex}].is_correct`}
      >
        {(field) => {
          const isCorrect = field.state.value === true;
          return (
            <button
              type="button"
              onClick={() => {
                field.handleChange(!isCorrect);
                field.handleBlur();
              }}
              aria-pressed={isCorrect}
              aria-label="Marcar como resposta correta"
              className={cn(
                "flex size-5 shrink-0 items-center justify-center rounded-full border-2 transition-colors",
                isCorrect
                  ? "border-success bg-success text-white"
                  : "border-muted-foreground/40 hover:border-muted-foreground",
              )}
            >
              {isCorrect ? (
                <svg
                  viewBox="0 0 24 24"
                  fill="none"
                  className="size-3"
                  aria-hidden
                >
                  <path
                    d="M5 13l4 4L19 7"
                    stroke="currentColor"
                    strokeWidth="3"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  />
                </svg>
              ) : null}
            </button>
          );
        }}
      </form.AppField>

      <form.AppField
        name={`sections[${sectionIndex}].modules[${moduleIndex}].lectures[${lectureIndex}].quiz.questions[${questionIndex}].options[${optionIndex}].text`}
      >
        {(field) => (
          <field.Field className="min-w-0 flex-1">
            <field.Label className="sr-only">Opção</field.Label>
            <field.Input
              className="h-8"
              placeholder={`Opção ${optionIndex + 1}`}
            />
            <field.Error />
          </field.Field>
        )}
      </form.AppField>

      <Button
        variant="ghost"
        size="icon-xs"
        onClick={onRemove}
        aria-label="Remover opção"
        className="shrink-0"
      >
        <Trash2 className="size-3 text-destructive" />
      </Button>
    </div>
  );
}

function PdfUploadField({
  form,
  sectionIndex,
  moduleIndex,
  lectureIndex,
}: {
  form: CourseFormInstance;
  sectionIndex: number;
  moduleIndex: number;
  lectureIndex: number;
}) {
  const [uploadError, setUploadError] = useState<string | null>(null);

  const uploadMutation = useMutation(
    orpc.training.v1.lectures.pdf.upload.mutationOptions({
      onSuccess: async (data) => {
        setUploadError(null);
        form.setFieldValue(
          `sections[${sectionIndex}].modules[${moduleIndex}].lectures[${lectureIndex}].file_key`,
          data.file_key,
        );
        form.setFieldValue(
          `sections[${sectionIndex}].modules[${moduleIndex}].lectures[${lectureIndex}].file_name`,
          data.file_name,
        );
        form.setFieldValue(
          `sections[${sectionIndex}].modules[${moduleIndex}].lectures[${lectureIndex}].file_size`,
          data.file_size,
        );
        form.setFieldValue(
          `sections[${sectionIndex}].modules[${moduleIndex}].lectures[${lectureIndex}].mime_type`,
          data.mime_type,
        );
      },
      onError: (error) => {
        setUploadError(
          error instanceof Error ? error.message : "Falha ao enviar arquivo.",
        );
      },
    }),
  );

  return (
    <div className="space-y-2">
      <form.Subscribe
        selector={(state) =>
          state.values.sections[sectionIndex]?.modules[moduleIndex]?.lectures[
            lectureIndex
          ]?.file_name
        }
      >
        {(fileName) => {
          if (fileName) {
            return (
              <div className="flex items-center justify-between gap-3 rounded-lg border bg-muted/40 px-3 py-2.5">
                <div className="flex min-w-0 items-center gap-2">
                  <FileText className="size-4 shrink-0 text-muted-foreground" />
                  <div className="min-w-0">
                    <p className="truncate text-sm font-medium">{fileName}</p>
                    <p className="text-[0.6875rem] text-muted-foreground">
                      PDF enviado
                    </p>
                  </div>
                </div>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  aria-label="Remover arquivo"
                  disabled={uploadMutation.isPending}
                  onClick={() => {
                    form.setFieldValue(
                      `sections[${sectionIndex}].modules[${moduleIndex}].lectures[${lectureIndex}].file_key`,
                      undefined,
                    );
                    form.setFieldValue(
                      `sections[${sectionIndex}].modules[${moduleIndex}].lectures[${lectureIndex}].file_name`,
                      undefined,
                    );
                    form.setFieldValue(
                      `sections[${sectionIndex}].modules[${moduleIndex}].lectures[${lectureIndex}].file_size`,
                      undefined,
                    );
                    form.setFieldValue(
                      `sections[${sectionIndex}].modules[${moduleIndex}].lectures[${lectureIndex}].mime_type`,
                      undefined,
                    );
                    setUploadError(null);
                  }}
                >
                  <X className="size-3.5 text-destructive" />
                </Button>
              </div>
            );
          }
          return (
            <FileUpload
              maxFiles={1}
              accept="application/pdf"
              onFileAccept={async (details) => {
                const file = details.files[0];
                if (!file) return;
                uploadMutation.mutate({ file });
              }}
            >
              <FileUploadDropzone>
                <FileUploadDropzoneIcon>
                  <FileText className="size-5" />
                </FileUploadDropzoneIcon>
                <FileUploadTitle>
                  {uploadMutation.isPending
                    ? "Enviando…"
                    : "Arraste um PDF ou clique para selecionar"}
                </FileUploadTitle>
                <FileUploadDescription>Apenas PDF · 10MB</FileUploadDescription>
              </FileUploadDropzone>
            </FileUpload>
          );
        }}
      </form.Subscribe>
      {uploadError ? (
        <p className="text-xs text-destructive">{uploadError}</p>
      ) : null}
    </div>
  );
}
