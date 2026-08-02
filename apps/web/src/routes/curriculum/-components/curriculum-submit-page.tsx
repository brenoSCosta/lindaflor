import { schema } from "@lindaflor/shared/schemas/curriculum";
import { useMutation, useQuery } from "@tanstack/react-query";
import { getRouteApi, useNavigate } from "@tanstack/react-router";
import {
  Briefcase,
  Building2,
  FileText,
  HardHat,
  Mail,
  MapPin,
  Phone,
  Send,
  User,
} from "lucide-react";
import type { ReactNode } from "react";
import { toast } from "sonner";
import { z } from "zod";

import { useAppForm } from "@/components/form/hooks";
import { Badge } from "@/components/ui/badge";
import {
  SelectGroup,
  SelectLabel,
  SelectSeparator,
  SelectValue,
} from "@/components/ui/select";
import { orpc, queryClient } from "@/lib/orpc";
import { assets } from "@/routes/-components/landing/assets";

const routeApi = getRouteApi("/curriculum/submit");

const TALENT_POOL_TARGET = "talent-pool";
const TALENT_POOL_LABEL = "Banco de talentos — cadastro geral";

const formSchema = z
  .object({
    name: schema.v1.submit.input.shape.name,
    email: schema.v1.submit.input.shape.email,
    phone: schema.v1.submit.input.shape.phone,
    summary: schema.v1.submit.input.shape.summary,
    application_target: z.string().min(1, { error: "Selecione uma opção" }),
    headline: z.string(),
    skills: z
      .string({ error: "Informe pelo menos uma habilidade" })
      .min(1, { error: "Informe pelo menos uma habilidade" }),
    file: schema.v1.submit.input.shape.file,
  })
  .superRefine((data, ctx) => {
    if (
      data.application_target === TALENT_POOL_TARGET &&
      !data.headline.trim()
    ) {
      ctx.addIssue({
        code: "custom",
        message: "Informe a área ou cargo de interesse",
        path: ["headline"],
      });
    }
  });

type FormValues = Omit<z.infer<typeof formSchema>, "file"> & {
  file: File | null;
};

function FormSection({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <div className="space-y-3">
      <h2 className="font-display text-sm font-semibold text-foreground">
        {title}
      </h2>
      {children}
    </div>
  );
}

export function CurriculumSubmitPage() {
  const navigate = useNavigate();
  const { career: preselectedCareerId } = routeApi.useSearch();
  const { careers: careersOptions } = routeApi.useLoaderData();
  const { data } = useQuery(careersOptions);

  const defaultValues: FormValues = {
    name: "",
    email: "",
    phone: "",
    summary: "",
    application_target: preselectedCareerId ?? TALENT_POOL_TARGET,
    headline: "",
    skills: "",
    file: null,
  };

  const mutation = useMutation(
    orpc.curriculum.v1.submit.mutationOptions({
      onSuccess: async () => {
        toast.success("Cadastro enviado com sucesso");
        await queryClient.invalidateQueries({
          queryKey: orpc.curriculum.v1.list.key(),
        });
        void navigate({ to: "/", search: {} });
      },
      onError: (error) => {
        toast.error(error.message);
      },
    }),
  );

  const form = useAppForm({
    defaultValues,
    validators: {
      onSubmit: formSchema,
    },
    onSubmit: ({ value }) => {
      const { file } = value;
      if (!file) {
        return;
      }

      const isTalentPool = value.application_target === TALENT_POOL_TARGET;

      mutation.mutate({
        name: value.name,
        email: value.email,
        phone: value.phone,
        summary: value.summary,
        career_id: isTalentPool ? undefined : value.application_target,
        headline: isTalentPool ? value.headline : undefined,
        skills: value.skills,
        file,
      });
    },
  });

  const careerOptions = data?.data ?? [];
  const openCareersCount = careerOptions.length;

  const applicationOptions = [
    { value: TALENT_POOL_TARGET, label: TALENT_POOL_LABEL },
    ...careerOptions.map((career) => ({
      value: career.id,
      label: `${career.title} · ${career.location}`,
    })),
  ];

  return (
    <div className="relative min-h-svh overflow-hidden">
      <div className="pointer-events-none absolute inset-0 -z-10">
        <img
          src={assets.background}
          alt=""
          className="size-full object-cover opacity-30"
        />
        <div className="absolute inset-0 bg-linear-to-b from-background via-background/95 to-background" />
      </div>

      <section className="relative mx-auto flex min-h-svh w-full max-w-360 items-center px-6 py-4 md:py-6">
        <div className="grid w-full gap-8 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.1fr)] lg:gap-12">
          <div className="flex flex-col justify-center">
            <div className="flex items-center gap-3 font-mono text-xs tracking-[0.22em] uppercase text-muted-foreground">
              <span className="h-px w-8 bg-border" />
              <span>Banco de talentos</span>
            </div>

            <h1 className="mt-3 max-w-xl font-display text-3xl font-bold tracking-tight text-balance text-foreground md:text-4xl lg:text-5xl">
              Faça parte do nosso{" "}
              <span className="text-secondary">banco de talentos</span>
            </h1>

            <p className="mt-3 max-w-md text-sm leading-relaxed text-muted-foreground">
              Cadastre seu perfil no banco de talentos da OG Service. Nossa
              equipe analisa cada candidatura e conecta profissionais às vagas
              em operação, manutenção, inspeção, engenharia e tecnologia.
            </p>

            {openCareersCount > 0 && (
              <Badge
                className="mt-4 h-auto overflow-visible whitespace-normal py-1.5 text-center md:text-start"
                variant="secondary"
              >
                <Briefcase className="size-4 shrink-0" />
                {openCareersCount} vagas abertas, ou cadastre-se no banco de
                talentos
              </Badge>
            )}
          </div>

          <div className="relative rounded-2xl border border-border/60 bg-card/80 p-4 shadow-2xl shadow-black/5 backdrop-blur-md sm:p-5">
            <form
              action={async () => {
                await form.handleSubmit();
              }}
              className="space-y-4"
            >
              <form.AppForm>
                <FormSection title="Dados pessoais">
                  <div className="grid gap-3 sm:grid-cols-2">
                    <form.AppField
                      name="name"
                      validators={{
                        onChange: schema.v1.submit.input.shape.name,
                      }}
                    >
                      {(field) => (
                        <field.Field>
                          <field.Label className="flex items-center gap-2 font-mono text-[0.625rem] tracking-[0.15em] text-muted-foreground uppercase">
                            <User className="size-3.5" />
                            Nome completo
                          </field.Label>
                          <field.Input placeholder="Seu nome completo" />
                          <field.Error />
                        </field.Field>
                      )}
                    </form.AppField>

                    <form.AppField
                      name="email"
                      validators={{
                        onChange: schema.v1.submit.input.shape.email,
                      }}
                    >
                      {(field) => (
                        <field.Field>
                          <field.Label className="flex items-center gap-2 font-mono text-[0.625rem] tracking-[0.15em] text-muted-foreground uppercase">
                            <Mail className="size-3.5" />
                            Email
                          </field.Label>
                          <field.Input
                            type="email"
                            placeholder="seu.email@exemplo.com"
                          />
                          <field.Error />
                        </field.Field>
                      )}
                    </form.AppField>
                  </div>

                  <form.AppField
                    name="phone"
                    validators={{
                      onChange: schema.v1.submit.input.shape.phone,
                    }}
                  >
                    {(field) => (
                      <field.Field>
                        <field.Label className="flex items-center gap-2 font-mono text-[0.625rem] tracking-[0.15em] text-muted-foreground uppercase">
                          <Phone className="size-3.5" />
                          Telefone
                        </field.Label>
                        <field.Input placeholder="(00) 00000-0000" />
                        <field.Error />
                      </field.Field>
                    )}
                  </form.AppField>
                </FormSection>

                <div className="h-px bg-border/60" />

                <FormSection title="Interesse profissional">
                  <form.AppField name="application_target">
                    {(field) => {
                      const selectedCareer = careerOptions.find(
                        (career) => career.id === field.state.value,
                      );

                      return (
                        <field.Field>
                          <field.Label className="flex items-center gap-2 font-mono text-[0.625rem] tracking-[0.15em] text-muted-foreground uppercase">
                            <Briefcase className="size-3.5" />
                            Onde deseja se candidatar?
                          </field.Label>
                          <field.Select>
                            <field.SelectTrigger>
                              <SelectValue placeholder="Selecione uma opção">
                                {(value) =>
                                  value
                                    ? (applicationOptions.find(
                                        (option) => option.value === value,
                                      )?.label ?? "Selecione uma opção")
                                    : null
                                }
                              </SelectValue>
                            </field.SelectTrigger>
                            <field.SelectContent
                              alignItemWithTrigger={false}
                              className="max-h-72"
                            >
                              <field.SelectItem value={TALENT_POOL_TARGET}>
                                {TALENT_POOL_LABEL}
                              </field.SelectItem>
                              {careerOptions.length > 0 && (
                                <>
                                  <SelectSeparator />
                                  <SelectGroup>
                                    <SelectLabel>Vagas abertas</SelectLabel>
                                    {careerOptions.map((career) => (
                                      <field.SelectItem
                                        key={career.id}
                                        value={career.id}
                                      >
                                        {career.title} · {career.location}
                                      </field.SelectItem>
                                    ))}
                                  </SelectGroup>
                                </>
                              )}
                            </field.SelectContent>
                          </field.Select>
                          <field.Error />

                          {selectedCareer && (
                            <div className="rounded-lg border border-secondary/20 bg-secondary/5 p-3">
                              <div className="flex flex-wrap items-center gap-2">
                                <Badge variant="secondary" className="text-xs">
                                  {selectedCareer.type}
                                </Badge>
                                <span className="flex items-center gap-1 text-xs text-muted-foreground">
                                  <Building2 className="size-3.5" />
                                  {selectedCareer.department}
                                </span>
                                <span className="flex items-center gap-1 text-xs text-muted-foreground">
                                  <MapPin className="size-3.5" />
                                  {selectedCareer.location}
                                </span>
                              </div>
                              <h3 className="mt-2 font-display text-sm font-semibold text-foreground">
                                {selectedCareer.title}
                              </h3>
                              {selectedCareer.description && (
                                <p className="mt-1 text-xs leading-relaxed text-muted-foreground">
                                  {selectedCareer.description}
                                </p>
                              )}
                            </div>
                          )}
                        </field.Field>
                      );
                    }}
                  </form.AppField>

                  <form.Subscribe
                    selector={(state) => state.values.application_target}
                  >
                    {(applicationTarget) =>
                      applicationTarget === TALENT_POOL_TARGET ? (
                        <form.AppField
                          name="headline"
                          validators={{
                            onChange: z.string().min(1, {
                              error: "Informe a área ou cargo de interesse",
                            }),
                          }}
                        >
                          {(headlineField) => (
                            <headlineField.Field>
                              <headlineField.Label className="flex items-center gap-2 font-mono text-[0.625rem] tracking-[0.15em] text-muted-foreground uppercase">
                                <Briefcase className="size-3.5" />
                                Área ou cargo de interesse
                              </headlineField.Label>
                              <headlineField.Input placeholder="Ex: Técnico de Operação, Manutenção Mecânica" />
                              <headlineField.Error />
                            </headlineField.Field>
                          )}
                        </form.AppField>
                      ) : null
                    }
                  </form.Subscribe>
                </FormSection>

                <div className="h-px bg-border/60" />

                <FormSection title="Perfil profissional">
                  <form.AppField
                    name="summary"
                    validators={{
                      onChange: schema.v1.submit.input.shape.summary,
                    }}
                  >
                    {(field) => (
                      <field.Field>
                        <field.Label className="flex items-center gap-2 font-mono text-[0.625rem] tracking-[0.15em] text-muted-foreground uppercase">
                          <FileText className="size-3.5" />
                          Resumo profissional
                        </field.Label>
                        <field.Textarea
                          className="min-h-12"
                          placeholder="Breve resumo sobre sua experiência na área industrial"
                          rows={3}
                        />
                        <field.Error />
                      </field.Field>
                    )}
                  </form.AppField>

                  <form.AppField
                    name="skills"
                    validators={{
                      onChange: z
                        .string({
                          error: "Informe pelo menos uma habilidade",
                        })
                        .min(1, {
                          error: "Informe pelo menos uma habilidade",
                        }),
                    }}
                  >
                    {(field) => (
                      <field.Field>
                        <field.Label className="flex items-center gap-2 font-mono text-[0.625rem] tracking-[0.15em] text-muted-foreground uppercase">
                          <HardHat className="size-3.5" />
                          Habilidades
                        </field.Label>
                        <field.Input placeholder="Operação de campo, NR-33, manutenção mecânica" />
                        <p className="text-xs text-muted-foreground">
                          Separe as habilidades por vírgula.
                        </p>
                        <field.Error />
                      </field.Field>
                    )}
                  </form.AppField>
                </FormSection>

                <div className="h-px bg-border/60" />

                <FormSection title="Currículo em PDF">
                  <form.AppField
                    name="file"
                    validators={{
                      onChange: schema.v1.submit.input.shape.file,
                    }}
                  >
                    {(field) => (
                      <field.Field className="[&>label]:min-h-20">
                        <field.FileUpload
                          accept="application/pdf"
                          placeholder="Arraste um PDF ou clique para selecionar"
                          description="Apenas PDF"
                        />
                        <field.Error />
                      </field.Field>
                    )}
                  </form.AppField>
                </FormSection>

                <div className="pt-1">
                  <form.Button
                    type="submit"
                    className="h-10 w-full gap-2 rounded-lg bg-primary font-mono text-xs font-semibold tracking-wider uppercase text-primary-foreground transition-colors hover:bg-primary/85"
                    loading={mutation.isPending}
                    loadingText="Enviando…"
                  >
                    Enviar cadastro
                    <Send className="size-4" />
                  </form.Button>
                  <p className="mt-2 text-center text-[0.625rem] text-muted-foreground">
                    Ao enviar, você concorda em ser contatado pela equipe de
                    Talentos da OG Service.
                  </p>
                </div>
              </form.AppForm>
            </form>
          </div>
        </div>
      </section>
    </div>
  );
}
