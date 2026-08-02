import {
  BookOpen,
  ChevronRight,
  Layers,
  Plus,
  Presentation,
  Trash2,
} from "lucide-react";
import type { ReactNode } from "react";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import {
  CourseEditorSidebar,
  type Selection,
} from "@/routes/(auth)/training/-components/course-editor-sidebar";
import { CourseEditorTour } from "@/routes/(auth)/training/-components/course-editor-tour";
import type { CourseFormInstance } from "@/routes/(auth)/training/-components/course-form-types";
import { LectureEditor } from "@/routes/(auth)/training/-components/lecture-editor";

const padNumber = (value: number) => String(value + 1).padStart(2, "0");

export function CourseEditor({
  form,
  header,
  footer,
  autoStart = false,
}: {
  form: CourseFormInstance;
  header: ReactNode;
  footer: ReactNode;
  autoStart?: boolean;
}) {
  const [selection, setSelection] = useState<Selection>({ kind: "course" });

  return (
    <div className="flex flex-col lg:grid lg:h-[94.5dvh] lg:grid-cols-[1fr_380px] lg:grid-rows-1">
      <main className="lg:h-full lg:overflow-y-auto">
        <div className="mx-auto w-full max-w-4xl space-y-5 p-4 sm:p-6 lg:p-8">
          <div className="flex flex-wrap items-center justify-between gap-2">
            {header}
            <CourseEditorTour autoStart={autoStart} />
          </div>

          {selection.kind === "course" ? (
            <CourseMetadataEditor form={form} onSelect={setSelection} />
          ) : null}

          {selection.kind === "section" ? (
            <SectionEditor
              form={form}
              sectionIndex={selection.sectionIndex}
              onSelect={setSelection}
            />
          ) : null}

          {selection.kind === "module" ? (
            <ModuleEditor
              form={form}
              sectionIndex={selection.sectionIndex}
              moduleIndex={selection.moduleIndex}
              onSelect={setSelection}
            />
          ) : null}

          {selection.kind === "lecture" ? (
            <form.Subscribe
              selector={(state) => {
                const section = state.values.sections[selection.sectionIndex];
                const module = section?.modules[selection.moduleIndex];
                return {
                  sectionTitle: section?.title ?? "",
                  moduleTitle: module?.title ?? "",
                };
              }}
            >
              {({ sectionTitle, moduleTitle }) => (
                <LectureEditor
                  form={form}
                  sectionIndex={selection.sectionIndex}
                  moduleIndex={selection.moduleIndex}
                  lectureIndex={selection.lectureIndex}
                  sectionTitle={sectionTitle}
                  moduleTitle={moduleTitle}
                />
              )}
            </form.Subscribe>
          ) : null}
        </div>
      </main>

      <CourseEditorSidebar
        form={form}
        selection={selection}
        onSelect={setSelection}
        footer={footer}
      />
    </div>
  );
}

function CourseMetadataEditor({
  form,
  onSelect,
}: {
  form: CourseFormInstance;
  onSelect: (selection: Selection) => void;
}) {
  return (
    <div className="space-y-6">
      <div className="space-y-1">
        <h2 className="text-xl font-bold tracking-tight">
          Informações do curso
        </h2>
        <p className="text-sm text-muted-foreground">
          Defina o título, a descrição e o status de publicação.
        </p>
      </div>

      <div
        id="course-editor-info"
        className="space-y-4 rounded-xl border bg-card p-4"
      >
        <form.AppField name="title">
          {(field) => (
            <field.Field>
              <field.Label>Título</field.Label>
              <field.Input placeholder="Título do curso" />
              <field.Error />
            </field.Field>
          )}
        </form.AppField>

        <form.AppField name="description">
          {(field) => (
            <field.Field>
              <field.Label>Descrição</field.Label>
              <field.Textarea placeholder="Descrição opcional" />
              <field.Error />
            </field.Field>
          )}
        </form.AppField>

        <form.AppField name="is_published">
          {(field) => (
            <field.Field orientation="horizontal">
              <div className="flex items-center gap-3">
                <field.Switch />
                <div className="space-y-0.5">
                  <field.Label>Publicado</field.Label>
                  <p className="text-xs text-muted-foreground">
                    Cursos publicados ficam visíveis para os alunos.
                  </p>
                </div>
              </div>
              <field.Error />
            </field.Field>
          )}
        </form.AppField>
      </div>

      <form.AppField name="sections" mode="array">
        {(sectionsField) => {
          const sections = sectionsField.state.value;
          return (
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <h3 className="text-sm font-semibold">
                  Seções ({sections.length})
                </h3>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  id="course-editor-add-section"
                  onClick={() => {
                    const newIndex = sections.length;
                    sectionsField.pushValue({
                      title: "",
                      description: "",
                      sort_order: newIndex,
                      modules: [],
                    });
                    onSelect({ kind: "section", sectionIndex: newIndex });
                  }}
                >
                  <Plus className="mr-1 size-4" />
                  Adicionar seção
                </Button>
              </div>

              {sections.length === 0 ? (
                <div className="rounded-lg border border-dashed py-8 text-center">
                  <BookOpen className="mx-auto mb-2 size-8 text-muted-foreground" />
                  <p className="text-sm text-muted-foreground">
                    Clique em &quot;Adicionar seção&quot; para começar.
                  </p>
                </div>
              ) : (
                <div className="space-y-2">
                  {sections.map((section, sectionIndex) => (
                    <button
                      key={section.id ?? sectionIndex}
                      type="button"
                      onClick={() =>
                        onSelect({ kind: "section", sectionIndex })
                      }
                      className="flex w-full items-center gap-3 rounded-lg border bg-card p-3 text-left transition-colors hover:border-primary/30 hover:bg-muted/30"
                    >
                      <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-primary/10 text-xs font-bold tabular-nums text-primary">
                        {padNumber(sectionIndex)}
                      </span>
                      <div className="min-w-0 flex-1 space-y-0.5">
                        <p className="line-clamp-1 text-sm font-medium">
                          {section.title || "Seção sem título"}
                        </p>
                        <p className="text-xs text-muted-foreground">
                          {section.modules?.length ?? 0} módulo(s)
                        </p>
                      </div>
                      <ChevronRight className="size-4 shrink-0 text-muted-foreground" />
                    </button>
                  ))}
                </div>
              )}
            </div>
          );
        }}
      </form.AppField>
    </div>
  );
}

function SectionEditor({
  form,
  sectionIndex,
  onSelect,
}: {
  form: CourseFormInstance;
  sectionIndex: number;
  onSelect: (selection: Selection) => void;
}) {
  return (
    <div className="space-y-6">
      <form.Subscribe
        selector={(state) => state.values.sections[sectionIndex]?.title ?? ""}
      >
        {(sectionTitle) => (
          <div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
            <span>Seção {padNumber(sectionIndex)}</span>
            <span aria-hidden>•</span>
            <span className="line-clamp-1">{sectionTitle || "Sem título"}</span>
          </div>
        )}
      </form.Subscribe>

      <form.AppField name={`sections[${sectionIndex}].title`}>
        {(field) => (
          <field.Field>
            <field.Label className="sr-only">Título da seção</field.Label>
            <field.Input
              data-tour="section-title"
              placeholder="Título da seção"
            />
            <field.Error />
          </field.Field>
        )}
      </form.AppField>

      <form.AppField name={`sections[${sectionIndex}].description`}>
        {(field) => (
          <field.Field>
            <field.Label className="text-xs font-medium text-muted-foreground">
              Descrição
            </field.Label>
            <field.Textarea
              className="min-h-16"
              placeholder="Descrição opcional da seção"
            />
            <field.Error />
          </field.Field>
        )}
      </form.AppField>

      <form.AppField name={`sections[${sectionIndex}].modules`} mode="array">
        {(modulesField) => {
          const modules = modulesField.state.value;
          return (
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <h3 className="text-sm font-semibold">
                  Módulos ({modules.length})
                </h3>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  id="course-editor-add-module"
                  onClick={() => {
                    const newIndex = modules.length;
                    modulesField.pushValue({
                      title: "",
                      description: "",
                      sort_order: newIndex,
                      lectures: [],
                    });
                    onSelect({
                      kind: "module",
                      sectionIndex,
                      moduleIndex: newIndex,
                    });
                  }}
                >
                  <Plus className="mr-1 size-4" />
                  Adicionar módulo
                </Button>
              </div>

              {modules.length === 0 ? (
                <div className="rounded-lg border border-dashed py-8 text-center">
                  <Layers className="mx-auto mb-2 size-8 text-muted-foreground" />
                  <p className="text-sm text-muted-foreground">
                    Adicione um módulo para organizar as aulas desta seção.
                  </p>
                </div>
              ) : (
                <div className="space-y-2">
                  {modules.map((module, moduleIndex) => (
                    <div
                      key={module.id ?? moduleIndex}
                      className="flex items-center gap-2 rounded-lg border bg-card p-3"
                    >
                      <button
                        type="button"
                        onClick={() =>
                          onSelect({
                            kind: "module",
                            sectionIndex,
                            moduleIndex,
                          })
                        }
                        className="flex flex-1 items-center gap-3 text-left"
                      >
                        <Layers className="size-4 shrink-0 text-muted-foreground" />
                        <div className="min-w-0 flex-1 space-y-0.5">
                          <p className="line-clamp-1 text-sm font-medium">
                            {module.title || "Módulo sem título"}
                          </p>
                          <p className="text-xs text-muted-foreground">
                            {module.lectures?.length ?? 0} aula(s)
                          </p>
                        </div>
                      </button>
                      <Button
                        variant="ghost"
                        size="icon-xs"
                        onClick={() => modulesField.removeValue(moduleIndex)}
                        aria-label="Remover módulo"
                        className="shrink-0"
                      >
                        <Trash2 className="size-4 text-destructive" />
                      </Button>
                    </div>
                  ))}
                  <button
                    type="button"
                    onClick={() => {
                      const newIndex = modules.length;
                      modulesField.pushValue({
                        title: "",
                        description: "",
                        sort_order: newIndex,
                        lectures: [],
                      });
                      onSelect({
                        kind: "module",
                        sectionIndex,
                        moduleIndex: newIndex,
                      });
                    }}
                    className="flex w-full items-center justify-center gap-1 rounded-lg border border-dashed py-2 text-xs text-muted-foreground transition-colors hover:border-primary/40 hover:text-foreground"
                  >
                    <Plus className="size-3.5" />
                    Adicionar outro módulo
                  </button>
                </div>
              )}
            </div>
          );
        }}
      </form.AppField>

      <Button
        variant="ghost"
        size="sm"
        onClick={() => onSelect({ kind: "course" })}
        className="text-xs"
      >
        <ChevronRight className="mr-1 size-3 rotate-180" />
        Voltar para informações do curso
      </Button>
    </div>
  );
}

function ModuleEditor({
  form,
  sectionIndex,
  moduleIndex,
  onSelect,
}: {
  form: CourseFormInstance;
  sectionIndex: number;
  moduleIndex: number;
  onSelect: (selection: Selection) => void;
}) {
  return (
    <div className="space-y-6">
      <form.Subscribe
        selector={(state) => ({
          moduleTitle:
            state.values.sections[sectionIndex]?.modules[moduleIndex]?.title ??
            "",
        })}
      >
        {({ moduleTitle }) => (
          <div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
            <span>Seção {padNumber(sectionIndex)}</span>
            <span aria-hidden>•</span>
            <span className="line-clamp-1">
              {moduleTitle || "Módulo sem título"}
            </span>
          </div>
        )}
      </form.Subscribe>

      <form.AppField
        name={`sections[${sectionIndex}].modules[${moduleIndex}].title`}
      >
        {(field) => (
          <field.Field>
            <field.Label className="sr-only">Título do módulo</field.Label>
            <field.Input
              data-tour="module-title"
              placeholder="Título do módulo"
            />
            <field.Error />
          </field.Field>
        )}
      </form.AppField>

      <form.AppField
        name={`sections[${sectionIndex}].modules[${moduleIndex}].description`}
      >
        {(field) => (
          <field.Field>
            <field.Label className="text-xs font-medium text-muted-foreground">
              Descrição
            </field.Label>
            <field.Textarea
              className="min-h-16"
              placeholder="Descrição opcional do módulo"
            />
            <field.Error />
          </field.Field>
        )}
      </form.AppField>

      <form.AppField
        name={`sections[${sectionIndex}].modules[${moduleIndex}].lectures`}
        mode="array"
      >
        {(lecturesField) => {
          const lectures = lecturesField.state.value;
          return (
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <h3 className="text-sm font-semibold">
                  Aulas ({lectures.length})
                </h3>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  id="course-editor-add-lecture"
                  onClick={() => {
                    const newIndex = lectures.length;
                    lecturesField.pushValue({
                      title: "",
                      type: "video",
                      sort_order: newIndex,
                    });
                    onSelect({
                      kind: "lecture",
                      sectionIndex,
                      moduleIndex,
                      lectureIndex: newIndex,
                    });
                  }}
                >
                  <Plus className="mr-1 size-4" />
                  Adicionar aula
                </Button>
              </div>

              {lectures.length === 0 ? (
                <div className="rounded-lg border border-dashed py-8 text-center">
                  <Presentation className="mx-auto mb-2 size-8 text-muted-foreground" />
                  <p className="text-sm text-muted-foreground">
                    Adicione aulas de vídeo, PDF ou link a este módulo.
                  </p>
                </div>
              ) : (
                <div className="space-y-2">
                  {lectures.map((lecture, lectureIndex) => (
                    <button
                      key={lecture.id ?? lectureIndex}
                      type="button"
                      onClick={() =>
                        onSelect({
                          kind: "lecture",
                          sectionIndex,
                          moduleIndex,
                          lectureIndex,
                        })
                      }
                      className="flex w-full items-center gap-3 rounded-lg border bg-card p-3 text-left transition-colors hover:border-primary/30 hover:bg-muted/30"
                    >
                      <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-muted text-xs font-bold tabular-nums text-muted-foreground">
                        {padNumber(lectureIndex)}
                      </span>
                      <div className="min-w-0 flex-1 space-y-0.5">
                        <p className="line-clamp-1 text-sm font-medium">
                          {lecture.title || "Aula sem título"}
                        </p>
                        <p className="text-xs text-muted-foreground">
                          {lecture.type === "video"
                            ? "Vídeo"
                            : lecture.type === "pdf"
                              ? "PDF"
                              : "Link"}
                          {lecture.quiz ? " • com quiz" : ""}
                        </p>
                      </div>
                      <ChevronRight className="size-4 shrink-0 text-muted-foreground" />
                    </button>
                  ))}
                  <button
                    type="button"
                    onClick={() => {
                      const newIndex = lectures.length;
                      lecturesField.pushValue({
                        title: "",
                        type: "video",
                        sort_order: newIndex,
                      });
                      onSelect({
                        kind: "lecture",
                        sectionIndex,
                        moduleIndex,
                        lectureIndex: newIndex,
                      });
                    }}
                    className="flex w-full items-center justify-center gap-1 rounded-lg border border-dashed py-2 text-xs text-muted-foreground transition-colors hover:border-primary/40 hover:text-foreground"
                  >
                    <Plus className="size-3.5" />
                    Adicionar outra aula
                  </button>
                </div>
              )}
            </div>
          );
        }}
      </form.AppField>

      <Button
        variant="ghost"
        size="sm"
        onClick={() => onSelect({ kind: "section", sectionIndex })}
        className="text-xs"
      >
        <ChevronRight className="mr-1 size-3 rotate-180" />
        Voltar para a seção
      </Button>
    </div>
  );
}
