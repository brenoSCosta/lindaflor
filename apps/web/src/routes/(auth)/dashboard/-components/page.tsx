import type { Organization } from "@lindaflor/db/schema/auth";
import { toOrgRole } from "@lindaflor/shared/lib/roles";
import { useMutation, useQuery } from "@tanstack/react-query";
import { getRouteApi } from "@tanstack/react-router";
import {
  Building2,
  Camera,
  Check,
  Minus,
  Pencil,
  Plus,
  RotateCcw,
  Trash2,
  Users,
} from "lucide-react";
import { useMemo, useState } from "react";
import { toast } from "sonner";
import { z } from "zod";

import { useAppForm } from "@/components/form/hooks";
import { OrgLogo } from "@/components/org-switcher";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import {
  Empty,
  EmptyDescription,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import {
  FileUpload,
  FileUploadDropzone,
  FileUploadDropzoneIcon,
  FileUploadHelper,
  FileUploadTitle,
} from "@/components/ui/file-upload";
import {
  ImageCropperImage,
  ImageCropperRootProvider,
  ImageCropperSelection,
  useImageCropper,
} from "@/components/ui/image-cropper";
import { Slider } from "@/components/ui/slider";
import { authClient } from "@/lib/auth-client";
import { orpc } from "@/lib/orpc";

const route = getRouteApi("/(auth)/dashboard/");

const LOGO_MAX_SIZE_BYTES = 2 * 1024 * 1024;
const ACCEPTED_IMAGE_TYPES = ["image/jpeg", "image/png", "image/webp"];

function getCroppedImageType(fileType: string): string {
  if (ACCEPTED_IMAGE_TYPES.includes(fileType)) {
    return fileType;
  }
  return "image/png";
}

function getFileExtension(type: string): string {
  switch (type) {
    case "image/jpeg":
      return "jpg";
    case "image/webp":
      return "webp";
    case "image/png":
    default:
      return "png";
  }
}

function LogoUploadDialog({
  children,
  onConfirm,
}: {
  children: React.ReactElement;
  onConfirm: (file: File) => void;
}) {
  const [open, setOpen] = useState(false);
  const [selectedFile, setSelectedFile] = useState<File | null>(null);
  const [isImageLoaded, setIsImageLoaded] = useState(false);

  const imageUrl = useMemo(() => {
    if (!selectedFile) {
      return null;
    }
    return URL.createObjectURL(selectedFile);
  }, [selectedFile]);

  const cropper = useImageCropper({
    aspectRatio: 1,
    cropShape: "circle",
  });

  const handleOpenChange = (isOpen: boolean) => {
    setOpen(isOpen);
    if (!isOpen) {
      setSelectedFile(null);
      setIsImageLoaded(false);
      cropper.reset();
    }
  };

  const handleFileSelect = (file: File) => {
    setSelectedFile(file);
    setIsImageLoaded(false);
    cropper.reset();
  };

  const handleClear = () => {
    setSelectedFile(null);
    setIsImageLoaded(false);
    cropper.reset();
  };

  const handleApply = async () => {
    if (!selectedFile) {
      return;
    }

    const outputType = getCroppedImageType(selectedFile.type);
    const blob = await cropper.getCroppedImage({
      type: outputType,
      output: "blob",
    });

    if (!(blob instanceof Blob)) {
      toast.error("Não foi possível recortar a imagem");
      return;
    }

    const filename = `${selectedFile.name.replace(/\.[^.]+$/, "")}.${getFileExtension(outputType)}`;
    const file = new File([blob], filename, { type: outputType });
    onConfirm(file);
    handleOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogTrigger render={children} />
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Alterar logo</DialogTitle>
          <DialogDescription>
            Selecione uma imagem e ajuste o recorte.
          </DialogDescription>
        </DialogHeader>

        <div className="py-4">
          {!imageUrl ? (
            <FileUpload
              accept={ACCEPTED_IMAGE_TYPES}
              maxFiles={1}
              maxFileSize={LOGO_MAX_SIZE_BYTES}
              onFileAccept={(details) => {
                const file = details.files[0];
                if (file) {
                  handleFileSelect(file);
                }
              }}
            >
              <FileUploadDropzone className="min-h-48">
                <FileUploadDropzoneIcon />
                <div className="space-y-1 text-center">
                  <FileUploadTitle>
                    Clique ou arraste uma imagem
                  </FileUploadTitle>
                  <FileUploadHelper>
                    JPG, PNG ou WebP · máx. 2MB
                  </FileUploadHelper>
                </div>
              </FileUploadDropzone>
            </FileUpload>
          ) : (
            <div className="space-y-4">
              <ImageCropperRootProvider
                className="mx-auto aspect-square size-64 rounded-lg border border-border"
                value={cropper}
              >
                <ImageCropperImage
                  src={imageUrl}
                  alt="Pré-visualização do recorte"
                  onLoad={() => {
                    setIsImageLoaded(true);
                    cropper.reset();
                  }}
                />
                <ImageCropperSelection axis="both" />
              </ImageCropperRootProvider>

              <div className="flex items-center gap-3">
                <Button
                  type="button"
                  variant="outline"
                  size="icon-sm"
                  onClick={() => cropper.zoomBy(-0.1)}
                  aria-label="Diminuir zoom"
                >
                  <Minus className="size-4" />
                </Button>
                <Slider
                  value={[cropper.zoom]}
                  min={1}
                  max={5}
                  step={0.1}
                  onValueChange={(value) => {
                    const zoom = Array.isArray(value) ? value[0] : value;
                    if (zoom !== undefined) {
                      cropper.setZoom(zoom);
                    }
                  }}
                />
                <Button
                  type="button"
                  variant="outline"
                  size="icon-sm"
                  onClick={() => cropper.zoomBy(0.1)}
                  aria-label="Aumentar zoom"
                >
                  <Plus className="size-4" />
                </Button>
                <Button
                  type="button"
                  variant="outline"
                  size="icon-sm"
                  onClick={() => cropper.reset()}
                  aria-label="Redefinir recorte"
                >
                  <RotateCcw className="size-4" />
                </Button>
              </div>

              <Button
                type="button"
                variant="outline"
                size="sm"
                className="w-full"
                onClick={handleClear}
              >
                Escolher outra imagem
              </Button>
            </div>
          )}
        </div>

        <DialogFooter>
          <Button
            type="button"
            onClick={handleApply}
            disabled={!imageUrl || !isImageLoaded}
            className="w-full sm:w-auto"
          >
            Aplicar
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function WelcomeSection() {
  const { session } = route.useRouteContext();
  const { data, isLoading } = useQuery(orpc.privateData.queryOptions());

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-2xl">
          Bem-vindo de volta, {session.user.name}
        </CardTitle>
        <CardDescription>
          Gerencie suas organizações e colabore com sua equipe.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="flex items-center gap-4 text-sm">
          <div className="flex items-center gap-2">
            <span className="text-muted-foreground">Status da API:</span>
            {isLoading ? (
              <Badge variant="outline">Carregando…</Badge>
            ) : (
              <Badge variant="secondary">
                {data?.message ?? "Desconhecido"}
              </Badge>
            )}
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

function CreateOrganizationCard() {
  const createOrganization = async ({
    name,
    slug,
  }: Pick<Organization, "name" | "slug">) => {
    await authClient.organization.create(
      {
        name,
        slug,
      },
      {
        onSuccess: () => {
          toast.success(`Organização "${name}" criada com sucesso!`);
          form.reset();
        },
        onError: (e) => {
          toast.error(e.error.message);
        },
      },
    );
  };

  const defaultValues: Pick<Organization, "name" | "slug"> = {
    name: "",
    slug: "",
  };

  const schema = z.object({
    name: z.string().min(2, "O nome deve ter pelo menos 2 caracteres"),
    slug: z.string().min(2, "O slug deve ter pelo menos 2 caracteres"),
  });

  const form = useAppForm({
    defaultValues,
    onSubmit: async ({ value }) => {
      await createOrganization({ name: value.name, slug: value.slug });
    },
    validators: {
      onChange: schema,
    },
  });

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center gap-2">
          <Building2 className="size-5 text-muted-foreground" />
          <CardTitle>Criar nova organização</CardTitle>
        </div>
        <CardDescription>
          Adicione uma nova organização para começar a colaborar com sua equipe.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form
          action={async () => {
            await form.handleSubmit();
          }}
          className="flex flex-col gap-4 md:flex-row md:items-end"
        >
          <form.AppForm>
            <div className="grid flex-1 gap-4 md:grid-cols-2">
              <form.AppField name="name">
                {(field) => (
                  <field.Field>
                    <field.Label>Nome da organização</field.Label>
                    <field.Input placeholder="Acme Corp" />
                    <field.Error />
                  </field.Field>
                )}
              </form.AppField>
              <form.AppField name="slug">
                {(field) => (
                  <field.Field>
                    <field.Label>Slug</field.Label>
                    <field.Input placeholder="acme-corp" />
                    <field.Error />
                  </field.Field>
                )}
              </form.AppField>
            </div>
            <form.Button
              disabled={form.state.isSubmitting}
              loading={form.state.isSubmitting}
              loadingText="Criando..."
              className="md:mb-0.5"
            >
              <Plus className="size-4" />
              Criar
            </form.Button>
          </form.AppForm>
        </form>
      </CardContent>
    </Card>
  );
}

const activateOrganization = async ({ id }: { id: Organization["id"] }) => {
  await authClient.organization.setActive(
    { organizationId: id },
    {
      onSuccess: () => {
        toast.success("Organização ativada com sucesso!");
      },
      onError: (e) => {
        toast.error(e.error.message);
      },
    },
  );
};

function OrganizationListSection() {
  const { data: organizations, refetch: refetchList } =
    authClient.useListOrganizations();
  const { data: activeOrganization, refetch: refetchActive } =
    authClient.useActiveOrganization();
  const { data: activeMember, refetch: refetchMember } =
    authClient.useActiveMember();
  const [open, setOpen] = useState(false);
  const [editingOrgId, setEditingOrgId] = useState<string | null>(null);

  const editingOrg = editingOrgId
    ? (organizations?.find((o) => o.id === editingOrgId) ?? null)
    : null;

  const orgRole = toOrgRole(activeMember?.role);
  const canUpdateLogo = orgRole === "owner" || orgRole === "admin";
  const activeOrgId = activeOrganization?.id;

  const handleUpdate = async ({
    name,
    slug,
  }: Pick<Organization, "name" | "slug">) => {
    if (!editingOrg) return;
    await authClient.organization.update(
      {
        data: { name, slug },
        organizationId: editingOrg.id,
      },
      {
        onSuccess: () => {
          toast.success("Organização atualizada com sucesso!");
          setOpen(false);
          setEditingOrgId(null);
        },
        onError: (e) => {
          toast.error(e.error.message);
        },
      },
    );
  };

  const handleDelete = async (id: Organization["id"]) => {
    await authClient.organization.delete(
      { organizationId: id },
      {
        onSuccess: async () => {
          toast.success("Organização excluída com sucesso!");
          await Promise.all([refetchList(), refetchActive(), refetchMember()]);
        },
        onError: (e) => {
          toast.error(e.error.message);
        },
      },
    );
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2">
        <Users className="size-5 text-muted-foreground" />
        <h2 className="text-lg font-semibold">Suas organizações</h2>
        {organizations && organizations.length > 0 && (
          <Badge variant="secondary" className="ml-auto">
            {organizations.length}{" "}
            {organizations.length === 1 ? "organização" : "organizações"}
          </Badge>
        )}
      </div>

      {!organizations || organizations.length === 0 ? (
        <Empty>
          <EmptyMedia variant="icon">
            <Building2 className="size-8" />
          </EmptyMedia>
          <EmptyTitle>Nenhuma organização ainda</EmptyTitle>
          <EmptyDescription>
            Crie sua primeira organização acima para começar.
          </EmptyDescription>
        </Empty>
      ) : (
        <div className="grid gap-4 md:grid-cols-2">
          {organizations.map((org) => (
            <Card
              key={org.id}
              className={org.id === activeOrgId ? "border-primary/50" : ""}
            >
              <CardHeader className="pb-3">
                <div className="flex items-start justify-between">
                  <div className="flex items-center gap-3">
                    <OrgLogo
                      orgId={org.id}
                      logo={org.logo}
                      name={org.name}
                      size="lg"
                    />
                    <div className="space-y-1">
                      <CardTitle className="flex items-center gap-2">
                        {org.name}
                        {org.id === activeOrgId && (
                          <Badge variant="secondary" className="text-xs">
                            <Check className="size-3 mr-1" />
                            Ativa
                          </Badge>
                        )}
                      </CardTitle>
                      <CardDescription className="font-mono text-xs">
                        {org.slug}
                      </CardDescription>
                    </div>
                  </div>
                </div>
              </CardHeader>
              <CardContent className="pt-0">
                <div className="flex gap-2">
                  {org.id !== activeOrgId && (
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => activateOrganization({ id: org.id })}
                    >
                      <Check className="size-3.5" />
                      Ativar
                    </Button>
                  )}
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => {
                      setEditingOrgId(org.id);
                      setOpen(true);
                    }}
                  >
                    <Pencil className="size-3.5" />
                    Editar
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="text-destructive hover:text-destructive"
                    onClick={() => handleDelete(org.id)}
                  >
                    <Trash2 className="size-3.5" />
                    Excluir
                  </Button>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      )}

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Atualizar organização</DialogTitle>
            <DialogDescription>
              Modifique os detalhes da organização abaixo.
            </DialogDescription>
          </DialogHeader>
          {editingOrg && (
            <UpdateOrganizationForm
              organization={editingOrg}
              canUpdateLogo={canUpdateLogo}
              onSubmit={handleUpdate}
              refetchList={refetchList}
              onCancel={() => {
                setOpen(false);
                setEditingOrgId(null);
              }}
            />
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}

function UpdateOrganizationForm({
  organization,
  canUpdateLogo,
  onSubmit,
  onCancel,
  refetchList,
}: {
  organization: {
    id: string;
    name: string;
    slug: string;
    logo?: string | null;
  };
  canUpdateLogo: boolean;
  onSubmit: (data: Pick<Organization, "name" | "slug">) => Promise<void>;
  onCancel: () => void;
  refetchList: () => Promise<unknown>;
}) {
  const logoMutation = useMutation(
    orpc.organization.v1.logo.update.mutationOptions({
      onSuccess: async () => {
        toast.success("Logo atualizada com sucesso!");
        await refetchList();
      },
      onError: (error) => {
        toast.error(error.message);
      },
    }),
  );

  const handleLogoConfirm = (file: File) => {
    logoMutation.mutate({ id: organization.id, file });
  };

  const defaultValues: Pick<Organization, "name" | "slug"> = {
    name: organization.name,
    slug: organization.slug,
  };

  const schema = z.object({
    name: z.string().min(2, "O nome deve ter pelo menos 2 caracteres"),
    slug: z.string().min(2, "O slug deve ter pelo menos 2 caracteres"),
  });

  const form = useAppForm({
    defaultValues,
    onSubmit: async ({ value }) => {
      await onSubmit({ name: value.name, slug: value.slug });
    },
    validators: {
      onChange: schema,
    },
  });

  return (
    <form
      action={async () => {
        await form.handleSubmit();
      }}
    >
      <form.AppForm>
        <div className="grid gap-4">
          {canUpdateLogo && (
            <div className="flex items-center gap-4">
              <OrgLogo
                orgId={organization.id}
                logo={organization.logo}
                name={organization.name}
                size="lg"
              />
              <div className="flex-1">
                <LogoUploadDialog onConfirm={handleLogoConfirm}>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={logoMutation.isPending}
                  >
                    <Camera className="size-4" />
                    {logoMutation.isPending ? "Enviando..." : "Alterar logo"}
                  </Button>
                </LogoUploadDialog>
                <p className="mt-1 text-xs text-muted-foreground">
                  JPG, PNG ou WebP · máx. 2MB
                </p>
              </div>
            </div>
          )}
          <form.AppField name="name">
            {(field) => (
              <field.Field>
                <field.Label>Nome da organização</field.Label>
                <field.Input placeholder="Nome da organização" />
                <field.Error />
              </field.Field>
            )}
          </form.AppField>
          <form.AppField name="slug">
            {(field) => (
              <field.Field>
                <field.Label>Slug</field.Label>
                <field.Input placeholder="slug-da-org" />
                <field.Error />
              </field.Field>
            )}
          </form.AppField>
        </div>
        <DialogFooter className="mt-6">
          <Button type="button" variant="outline" onClick={onCancel}>
            Cancelar
          </Button>
          <form.Button
            disabled={form.state.isSubmitting}
            loading={form.state.isSubmitting}
            loadingText="Atualizando..."
          >
            Salvar alterações
          </form.Button>
        </DialogFooter>
      </form.AppForm>
    </form>
  );
}

export function DashboardPage() {
  return (
    <div className="container max-w-4xl mx-auto py-8 px-4 space-y-8">
      <WelcomeSection />

      <CreateOrganizationCard />

      <OrganizationListSection />
    </div>
  );
}
