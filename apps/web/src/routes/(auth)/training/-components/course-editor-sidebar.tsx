import {
  FileText,
  HelpCircle,
  Info,
  Layers,
  Link as LinkIcon,
  PlayCircle,
} from "lucide-react";
import type { CSSProperties, ReactNode } from "react";

import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";
import { Badge } from "@/components/ui/badge";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
} from "@/components/ui/sidebar";
import { cn } from "@/lib/utils";
import type { CourseFormInstance } from "@/routes/(auth)/training/-components/course-form-types";

export type Selection =
  | { kind: "course" }
  | { kind: "section"; sectionIndex: number }
  | { kind: "module"; sectionIndex: number; moduleIndex: number }
  | {
      kind: "lecture";
      sectionIndex: number;
      moduleIndex: number;
      lectureIndex: number;
    };

type CourseEditorSidebarProps = {
  form: CourseFormInstance;
  selection: Selection;
  onSelect: (selection: Selection) => void;
  footer?: ReactNode;
};

const padNumber = (value: number) => String(value + 1).padStart(2, "0");

function lectureTypeIcon(type: string) {
  if (type === "pdf") return FileText;
  if (type === "link") return LinkIcon;
  return PlayCircle;
}

export function CourseEditorSidebar({
  form,
  selection,
  onSelect,
  footer,
}: CourseEditorSidebarProps) {
  return (
    <form.Subscribe selector={(state) => state.values}>
      {(values) => {
        const sections = values.sections ?? [];
        const totalLectures = sections.reduce(
          (acc, s) =>
            acc +
            (s.modules ?? []).reduce(
              (ma, m) => ma + (m.lectures?.length ?? 0),
              0,
            ),
          0,
        );
        const totalQuizzes = sections.reduce(
          (acc, s) =>
            acc +
            (s.modules ?? []).reduce(
              (ma, m) => ma + (m.lectures?.filter((l) => l.quiz).length ?? 0),
              0,
            ),
          0,
        );

        const defaultExpanded: string[] = [];
        if (sections.length > 0) {
          const si = selection.kind !== "course" ? selection.sectionIndex : 0;
          defaultExpanded.push(`section-${si}`);
        }

        return (
          <Sidebar
            id="course-editor-sidebar"
            side="right"
            collapsible="none"
            className="border-l"
            style={
              { "--sidebar-width": "380px" } as CSSProperties &
                Record<string, string>
            }
          >
            <SidebarHeader className="space-y-0 p-0">
              <button
                type="button"
                onClick={() => onSelect({ kind: "course" })}
                className={cn(
                  "flex w-full items-center gap-2 border-b px-4 py-3 text-left transition-colors",
                  selection.kind === "course"
                    ? "bg-primary/5"
                    : "hover:bg-muted/60",
                )}
              >
                <Info className="size-4 shrink-0 text-muted-foreground" />
                <div className="min-w-0 flex-1 space-y-0.5">
                  <p className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                    Informações do curso
                  </p>
                  <p className="line-clamp-1 text-sm font-medium">
                    {values.title || "Curso sem título"}
                  </p>
                </div>
              </button>

              <div className="flex items-center gap-3 border-b px-4 py-2.5 text-xs text-muted-foreground">
                <span className="inline-flex items-center gap-1">
                  <Layers className="size-3" />
                  <span className="tabular-nums font-medium text-foreground">
                    {sections.length}
                  </span>
                  seções
                </span>
                <span aria-hidden className="text-border">
                  |
                </span>
                <span className="inline-flex items-center gap-1">
                  <PlayCircle className="size-3" />
                  <span className="tabular-nums font-medium text-foreground">
                    {totalLectures}
                  </span>
                  aulas
                </span>
                {totalQuizzes > 0 ? (
                  <>
                    <span aria-hidden className="text-border">
                      |
                    </span>
                    <span className="inline-flex items-center gap-1">
                      <HelpCircle className="size-3" />
                      <span className="tabular-nums font-medium text-foreground">
                        {totalQuizzes}
                      </span>
                      quizzes
                    </span>
                  </>
                ) : null}
              </div>
            </SidebarHeader>

            <SidebarContent className="p-2">
              {sections.length === 0 ? (
                <p className="px-4 py-8 text-center text-xs text-muted-foreground">
                  Nenhuma seção ainda. Use o editor para adicionar a primeira
                  seção.
                </p>
              ) : (
                <Accordion
                  multiple
                  defaultValue={defaultExpanded}
                  className="p-2"
                >
                  {sections.map((section, sectionIndex) => {
                    const sectionModules = section.modules ?? [];
                    const sectionLectures = sectionModules.reduce(
                      (acc, m) => acc + (m.lectures?.length ?? 0),
                      0,
                    );
                    const isSectionSelected =
                      selection.kind === "section" &&
                      selection.sectionIndex === sectionIndex;

                    return (
                      <AccordionItem
                        key={section.id ?? `section-${sectionIndex}`}
                        value={`section-${sectionIndex}`}
                        className="overflow-hidden rounded-lg border-b-0 px-1"
                      >
                        <AccordionTrigger
                          className={cn(
                            "items-center gap-3 rounded-lg px-2 py-2.5 hover:bg-muted/60 hover:no-underline",
                            isSectionSelected && "bg-primary/5",
                          )}
                        >
                          <button
                            type="button"
                            onClick={(e) => {
                              e.stopPropagation();
                              onSelect({ kind: "section", sectionIndex });
                            }}
                            onKeyDown={(e) => {
                              if (e.key === "Enter" || e.key === " ") {
                                e.preventDefault();
                                e.stopPropagation();
                                onSelect({
                                  kind: "section",
                                  sectionIndex,
                                });
                              }
                            }}
                            className="flex size-8 shrink-0 cursor-pointer items-center justify-center rounded-md bg-muted text-xs font-semibold tabular-nums text-muted-foreground"
                          >
                            {padNumber(sectionIndex)}
                          </button>
                          <span className="flex-1 space-y-0.5">
                            <span className="block line-clamp-1 text-sm font-semibold leading-tight">
                              {section.title || "Seção sem título"}
                            </span>
                            <span className="block text-xs text-muted-foreground">
                              {sectionLectures} aulas
                            </span>
                          </span>
                          {isSectionSelected ? (
                            <Badge variant="secondary" className="shrink-0">
                              Selecionado
                            </Badge>
                          ) : null}
                        </AccordionTrigger>
                        <AccordionContent className="space-y-3 px-1 pb-3 pt-0">
                          {sectionModules.map((module, moduleIndex) => {
                            const moduleLectures = module.lectures ?? [];
                            const isModuleSelected =
                              selection.kind === "module" &&
                              selection.sectionIndex === sectionIndex &&
                              selection.moduleIndex === moduleIndex;

                            return (
                              <div
                                key={
                                  module.id ??
                                  `module-${sectionIndex}-${moduleIndex}`
                                }
                                className="space-y-0.5"
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
                                  className={cn(
                                    "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors",
                                    "hover:bg-muted/60",
                                    isModuleSelected && "bg-primary/5",
                                  )}
                                >
                                  <Layers className="size-3.5 shrink-0 text-muted-foreground" />
                                  <span
                                    className={cn(
                                      "flex-1 line-clamp-1 text-xs font-medium",
                                      isModuleSelected
                                        ? "text-foreground"
                                        : "text-muted-foreground",
                                    )}
                                  >
                                    {module.title || "Módulo sem título"}
                                  </span>
                                  {isModuleSelected ? (
                                    <Badge
                                      variant="secondary"
                                      className="shrink-0"
                                    >
                                      Selecionado
                                    </Badge>
                                  ) : null}
                                </button>

                                <ul className="ml-3.5 space-y-0.5 border-l pl-1.5">
                                  {moduleLectures.map(
                                    (lecture, lectureIndex) => {
                                      const isLectureSelected =
                                        selection.kind === "lecture" &&
                                        selection.sectionIndex ===
                                          sectionIndex &&
                                        selection.moduleIndex === moduleIndex &&
                                        selection.lectureIndex === lectureIndex;
                                      const TypeIcon = lectureTypeIcon(
                                        lecture.type ?? "video",
                                      );

                                      return (
                                        <li
                                          key={
                                            lecture.id ??
                                            `lecture-${sectionIndex}-${moduleIndex}-${lectureIndex}`
                                          }
                                        >
                                          <button
                                            type="button"
                                            onClick={() =>
                                              onSelect({
                                                kind: "lecture",
                                                sectionIndex,
                                                moduleIndex,
                                                lectureIndex,
                                              })
                                            }
                                            aria-current={
                                              isLectureSelected
                                                ? "true"
                                                : undefined
                                            }
                                            className={cn(
                                              "flex w-full items-center gap-2 rounded-md border border-transparent p-1.5 text-left transition-colors",
                                              "hover:bg-muted/60",
                                              isLectureSelected &&
                                                "border-primary/20 bg-primary/5",
                                            )}
                                          >
                                            <TypeIcon className="size-3.5 shrink-0 text-muted-foreground" />
                                            <span className="w-4 shrink-0 text-xs tabular-nums text-muted-foreground">
                                              {padNumber(lectureIndex)}
                                            </span>
                                            <span
                                              className={cn(
                                                "flex-1 line-clamp-1 text-xs",
                                                isLectureSelected
                                                  ? "font-medium text-foreground"
                                                  : "text-muted-foreground",
                                              )}
                                            >
                                              {lecture.title ||
                                                "Aula sem título"}
                                            </span>
                                            {lecture.quiz ? (
                                              <HelpCircle className="size-3 shrink-0 text-primary/60" />
                                            ) : null}
                                            {isLectureSelected ? (
                                              <Badge
                                                variant="secondary"
                                                className="shrink-0"
                                              >
                                                Editando
                                              </Badge>
                                            ) : null}
                                          </button>
                                        </li>
                                      );
                                    },
                                  )}
                                </ul>
                              </div>
                            );
                          })}
                        </AccordionContent>
                      </AccordionItem>
                    );
                  })}
                </Accordion>
              )}
            </SidebarContent>
            {footer ? <SidebarFooter>{footer}</SidebarFooter> : null}
          </Sidebar>
        );
      }}
    </form.Subscribe>
  );
}
