import type { TrainingCourseDetailOutput } from "@lindaflor/shared/schemas/training";
import {
  CheckCircle,
  Circle,
  FileText,
  Link as LinkIcon,
  PlayCircle,
} from "lucide-react";

import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";

type CourseSidebarProps = {
  course: TrainingCourseDetailOutput;
  selectedLectureId: string | null;
  onSelectLecture: (lectureId: string) => void;
};

const padNumber = (value: number) => String(value + 1).padStart(2, "0");

function lectureTypeLabel(type: "video" | "pdf" | "link") {
  if (type === "video") return "Vídeo";
  if (type === "pdf") return "PDF";
  return "Link";
}

function lectureTypeIcon(type: "video" | "pdf" | "link") {
  if (type === "pdf") return FileText;
  if (type === "link") return LinkIcon;
  return PlayCircle;
}

export function CourseSidebarContent({
  course,
  selectedLectureId,
  onSelectLecture,
}: CourseSidebarProps) {
  const allLectures = course.sections.flatMap((section) =>
    section.modules.flatMap((module) => module.lectures),
  );
  const completedCount = allLectures.filter(
    (lecture) => lecture.progress?.status === "completed",
  ).length;
  const totalLectures = allLectures.length;
  const progressPercent =
    totalLectures > 0 ? Math.round((completedCount / totalLectures) * 100) : 0;

  const defaultExpanded: string[] = [];
  for (const section of course.sections) {
    const isSelected = section.modules.some((module) =>
      module.lectures.some((lecture) => lecture.id === selectedLectureId),
    );
    if (isSelected) {
      defaultExpanded.push(section.id);
    }
  }
  if (defaultExpanded.length === 0 && course.sections.length > 0) {
    defaultExpanded.push(course.sections[0].id);
  }

  return (
    <>
      <div className="space-y-2 border-b px-5 py-4">
        <div className="space-y-0.5">
          <h2 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">
            Conteúdo do curso
          </h2>
          <p className="line-clamp-1 text-sm font-medium">{course.title}</p>
        </div>
        <div className="space-y-1.5">
          <div className="flex items-center justify-between text-xs">
            <span className="font-medium text-muted-foreground">
              {progressPercent}% concluído
            </span>
            <span className="tabular-nums text-muted-foreground">
              {completedCount}/{totalLectures} aulas
            </span>
          </div>
          <div className="h-2 w-full overflow-hidden rounded-full bg-muted">
            <div
              className="h-full rounded-full bg-primary transition-all duration-500"
              style={{ width: `${progressPercent}%` }}
            />
          </div>
        </div>
      </div>

      <ScrollArea className="min-h-0 flex-1">
        <Accordion multiple defaultValue={defaultExpanded} className="p-2">
          {course.sections.map((section, sectionIndex) => {
            const sectionLectures = section.modules.flatMap(
              (module) => module.lectures,
            );
            const sectionCompleted = sectionLectures.filter(
              (lecture) => lecture.progress?.status === "completed",
            ).length;

            return (
              <AccordionItem
                key={section.id}
                value={section.id}
                className="overflow-hidden rounded-lg border-b-0 px-1"
              >
                <AccordionTrigger className="items-center gap-3 rounded-lg px-2 py-3 hover:bg-muted/60 hover:no-underline">
                  <span className="flex size-8 shrink-0 items-center justify-center rounded-md bg-muted text-xs font-semibold tabular-nums text-muted-foreground">
                    {padNumber(sectionIndex)}
                  </span>
                  <span className="flex-1 space-y-0.5">
                    <span className="block text-sm font-semibold leading-tight">
                      {section.title}
                    </span>
                    <span className="block text-xs text-muted-foreground">
                      {sectionCompleted}/{sectionLectures.length} aulas
                    </span>
                  </span>
                </AccordionTrigger>
                <AccordionContent className="space-y-4 px-1 pb-3 pt-0">
                  {section.modules.map((module, moduleIndex) => (
                    <div key={module.id} className="space-y-1">
                      <h4 className="px-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
                        {module.title}
                      </h4>
                      <ul className="space-y-0.5">
                        {module.lectures.map((lecture, lectureIndex) => {
                          const status =
                            lecture.progress?.status ?? "not_started";
                          const isSelected = lecture.id === selectedLectureId;
                          const TypeIcon = lectureTypeIcon(lecture.type);

                          return (
                            <li key={lecture.id}>
                              <button
                                type="button"
                                onClick={() => onSelectLecture(lecture.id)}
                                aria-current={isSelected ? "true" : undefined}
                                className={cn(
                                  "flex w-full items-center gap-3 rounded-md border border-transparent p-2 text-left transition-colors",
                                  "hover:bg-muted/60",
                                  isSelected &&
                                    "border-primary/20 bg-primary/5",
                                )}
                              >
                                <span className="flex w-5 shrink-0 justify-center">
                                  {status === "completed" ? (
                                    <CheckCircle className="size-4 shrink-0 text-success" />
                                  ) : isSelected ? (
                                    <PlayCircle className="size-4 shrink-0 text-primary" />
                                  ) : (
                                    <Circle className="size-4 shrink-0 text-muted-foreground/50" />
                                  )}
                                </span>
                                <span className="w-5 shrink-0 text-xs tabular-nums text-muted-foreground">
                                  {padNumber(lectureIndex)}
                                </span>
                                <span className="flex-1 space-y-1">
                                  <span
                                    className={cn(
                                      "block line-clamp-1 text-sm",
                                      isSelected
                                        ? "font-medium text-foreground"
                                        : "text-foreground/80",
                                      status === "completed" &&
                                        !isSelected &&
                                        "text-muted-foreground",
                                    )}
                                  >
                                    {lecture.title}
                                  </span>
                                  <span className="flex items-center gap-1.5">
                                    <TypeIcon className="size-3 text-muted-foreground" />
                                    <span className="text-xs text-muted-foreground">
                                      {lectureTypeLabel(lecture.type)}
                                    </span>
                                  </span>
                                </span>
                                {isSelected ? (
                                  <Badge
                                    variant="secondary"
                                    className="shrink-0"
                                  >
                                    Reproduzindo
                                  </Badge>
                                ) : null}
                              </button>
                            </li>
                          );
                        })}
                      </ul>
                      {moduleIndex < section.modules.length - 1 ? (
                        <div className="mx-2 my-2 h-px bg-border" />
                      ) : null}
                    </div>
                  ))}
                </AccordionContent>
              </AccordionItem>
            );
          })}
        </Accordion>
      </ScrollArea>
    </>
  );
}

export function CourseSidebar(props: CourseSidebarProps) {
  return (
    <aside
      className={cn("hidden border-l bg-card lg:flex lg:h-full lg:flex-col")}
    >
      <CourseSidebarContent {...props} />
    </aside>
  );
}
