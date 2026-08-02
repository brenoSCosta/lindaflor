import { Effect } from "effect";
import { CircleHelp } from "lucide-react";
import React from "react";

import { Button } from "@/components/ui/button";
import {
  Tour,
  TourContent,
  TourDescription,
  TourFooter,
  TourHeader,
  TourNextStep,
  TourPreviousStep,
  TourProgressText,
  type TourStepType,
  TourTitle,
  TourTrigger,
  useTourContext,
  waitForElement,
  waitForEvent,
} from "@/components/ui/tour";

const TOUR_STORAGE_KEY = "lindaflor:training:course-editor-tour";
const TOUR_AUTO_START_DELAY_MS = 400;
const ELEMENT_TIMEOUT_MS = 5000;
const INPUT_POLL_INTERVAL_MS = 150;
const TITLE_MIN_LENGTH = 2;

const sectionTitleResolver = () =>
  document.querySelector<HTMLInputElement>('[data-tour="section-title"]');
const moduleTitleResolver = () =>
  document.querySelector<HTMLInputElement>('[data-tour="module-title"]');
const lectureTitleResolver = () =>
  document.querySelector<HTMLInputElement>('[data-tour="lecture-title"]');
const addSectionResolver = () =>
  document.querySelector<HTMLElement>("#course-editor-add-section");
const addModuleResolver = () =>
  document.querySelector<HTMLElement>("#course-editor-add-module");
const addLectureResolver = () =>
  document.querySelector<HTMLElement>("#course-editor-add-lecture");

/**
 * Build an `effect` that waits for the input (resolved by `resolver`) to mount,
 * shows the tooltip, focuses it, and advances once the trimmed value reaches
 * `TITLE_MIN_LENGTH`. Returns a single cleanup that cancels both the
 * element-wait and the value poll. Storing the poll cleanup as a function (not
 * the raw interval handle) sidesteps the DOM `number` vs Node `Timeout`
 * mismatch on `ReturnType<typeof window.setInterval>`.
 */
function attachInputEffect(
  resolver: () => HTMLInputElement | null,
): NonNullable<Extract<TourStepType, { effect?: unknown }>["effect"]> {
  return function effect({ next, show }) {
    const [elPromise, elCancel] = waitForElement(resolver, {
      timeout: ELEMENT_TIMEOUT_MS,
    });
    let clear: (() => void) | undefined;
    let advanced = false;
    void Effect.runPromise(
      Effect.tryPromise({
        try: () => elPromise,
        catch: () => new Error("element-wait-timeout"),
      }).pipe(
        Effect.tap(() =>
          Effect.sync(() => {
            show();
            window.requestAnimationFrame(() => {
              const el = resolver();
              if (el instanceof HTMLInputElement) el.focus();
            });
            const id = window.setInterval(() => {
              if (advanced) return;
              const el = resolver();
              if (
                el instanceof HTMLInputElement &&
                el.value.trim().length >= TITLE_MIN_LENGTH
              ) {
                advanced = true;
                next();
              }
            }, INPUT_POLL_INTERVAL_MS);
            clear = () => window.clearInterval(id);
          }),
        ),
        Effect.catchAll(() => Effect.void),
      ),
    );
    return () => {
      elCancel();
      clear?.();
    };
  };
}

const steps: TourStepType[] = [
  {
    id: "welcome",
    type: "dialog",
    title: "Bem-vindo ao editor de cursos",
    description:
      "Vamos guiá-lo por toda a criação de um curso: seções, módulos e aulas.",
    actions: [{ label: "Iniciar", action: "next" }],
  },
  {
    id: "course-info",
    type: "tooltip",
    target: () => document.querySelector<HTMLElement>("#course-editor-info"),
    title: "Informações do curso",
    description:
      "Aqui você define o título, a descrição e o status de publicação do curso.",
    actions: [
      { label: "Voltar", action: "prev" },
      { label: "Próximo", action: "next" },
    ],
  },
  {
    id: "add-section",
    type: "tooltip",
    target: () =>
      document.querySelector<HTMLElement>("#course-editor-add-section"),
    title: "Primeira seção",
    description:
      "Clique no botão “Adicionar seção” para criar a primeira parte do curso.",
    effect({ next, show }) {
      show();
      const [promise, cancel] = waitForEvent(addSectionResolver, "click");
      void promise.then(() => next());
      return cancel;
    },
  },
  {
    id: "section-title",
    type: "tooltip",
    target: sectionTitleResolver,
    title: "Título da seção",
    description: `Digite um título com pelo menos ${TITLE_MIN_LENGTH} caracteres para continuar.`,
    effect: attachInputEffect(sectionTitleResolver),
  },
  {
    id: "add-module",
    type: "tooltip",
    target: () =>
      document.querySelector<HTMLElement>("#course-editor-add-module"),
    title: "Primeiro módulo",
    description:
      "Agora clique em “Adicionar módulo” para organizar as aulas dentro desta seção.",
    effect({ next, show }) {
      show();
      const [promise, cancel] = waitForEvent(addModuleResolver, "click");
      void promise.then(() => next());
      return cancel;
    },
  },
  {
    id: "module-title",
    type: "tooltip",
    target: moduleTitleResolver,
    title: "Título do módulo",
    description: `Digite um título para o módulo (mínimo ${TITLE_MIN_LENGTH} caracteres).`,
    effect: attachInputEffect(moduleTitleResolver),
  },
  {
    id: "add-lecture",
    type: "tooltip",
    target: () =>
      document.querySelector<HTMLElement>("#course-editor-add-lecture"),
    title: "Primeira aula",
    description: "Clique em “Adicionar aula” para criar o conteúdo do módulo.",
    effect({ next, show }) {
      show();
      const [promise, cancel] = waitForEvent(addLectureResolver, "click");
      void promise.then(() => next());
      return cancel;
    },
  },
  {
    id: "lecture-title",
    type: "tooltip",
    target: lectureTitleResolver,
    title: "Título da aula",
    description: `Digite um título para a aula (mínimo ${TITLE_MIN_LENGTH} caracteres).`,
    effect: attachInputEffect(lectureTitleResolver),
  },
  {
    id: "lecture-content",
    type: "tooltip",
    target: () =>
      document.querySelector<HTMLElement>('[data-tour="lecture-content"]'),
    title: "Conteúdo da aula",
    description:
      "Escolha o tipo (vídeo, PDF ou link), informe o conteúdo e, na aba Quiz, crie perguntas avaliativas.",
    actions: [
      { label: "Voltar", action: "prev" },
      { label: "Próximo", action: "next" },
    ],
  },
  {
    id: "sidebar",
    type: "floating",
    title: "Estrutura do curso",
    description:
      "Use a barra lateral para navegar entre seções, módulos e aulas que você criar.",
    actions: [
      { label: "Voltar", action: "prev" },
      { label: "Próximo", action: "next" },
    ],
  },
  {
    id: "finish",
    type: "dialog",
    title: "Tudo pronto!",
    description:
      "É só preencher as informações e salvar o curso. Use o botão Tour para rever este guia quando quiser.",
    actions: [{ label: "Concluir", action: "dismiss" }],
  },
];

function hasSeenTour(): boolean {
  return Effect.runSync(
    Effect.try({
      try: () => localStorage.getItem(TOUR_STORAGE_KEY) === "seen",
      catch: () => new Error("localStorage unavailable"),
    }).pipe(Effect.catchAll(() => Effect.succeed(false))),
  );
}

function markTourSeen(): void {
  Effect.runSync(
    Effect.try({
      try: () => {
        localStorage.setItem(TOUR_STORAGE_KEY, "seen");
      },
      catch: () => new Error("localStorage unavailable"),
    }).pipe(Effect.catchAll(() => Effect.void)),
  );
}

interface CourseEditorTourContentProps {
  autoStart: boolean;
}

function CourseEditorTourContent({ autoStart }: CourseEditorTourContentProps) {
  const { handleStart } = useTourContext();

  React.useEffect(() => {
    if (!autoStart || hasSeenTour()) {
      return undefined;
    }
    const timer = window.setTimeout(() => {
      handleStart();
    }, TOUR_AUTO_START_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [autoStart, handleStart]);

  return (
    <>
      <TourTrigger asChild>
        <Button size="sm" variant="ghost">
          <CircleHelp className="size-4" />
          Tour
        </Button>
      </TourTrigger>

      <TourContent className="overflow-hidden" progress>
        <TourHeader>
          <TourProgressText />
          <TourTitle />
          <TourDescription />
        </TourHeader>

        <TourFooter>
          <TourPreviousStep />
          <TourNextStep />
        </TourFooter>
      </TourContent>
    </>
  );
}

export function CourseEditorTour({
  autoStart = false,
}: {
  autoStart?: boolean;
}) {
  const handleStatusChange = React.useCallback(
    (details: { status: string }) => {
      if (
        details.status === "completed" ||
        details.status === "dismissed" ||
        details.status === "skipped"
      ) {
        markTourSeen();
      }
    },
    [],
  );

  return (
    <Tour steps={steps} onStatusChange={handleStatusChange}>
      <CourseEditorTourContent autoStart={autoStart} />
    </Tour>
  );
}
