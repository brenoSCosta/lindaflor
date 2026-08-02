import { useMutation } from "@tanstack/react-query";
import { Camera, Loader2, Minus, Plus, RotateCcw, X } from "lucide-react";
import { useMemo, useState } from "react";
import { toast } from "sonner";
import { z } from "zod";

import { useAppForm } from "@/components/form/hooks";
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
import { Skeleton } from "@/components/ui/skeleton";
import { Slider } from "@/components/ui/slider";
import { UserAvatar } from "@/components/user-avatar";
import { authClient } from "@/lib/auth-client";
import { orpc } from "@/lib/orpc";

const schema = z.object({
  name: z.string().min(2, "O nome deve ter pelo menos 2 caracteres"),
});

const AVATAR_MAX_SIZE_BYTES = 2 * 1024 * 1024;
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

function AvatarUploadDialog({
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
          <DialogTitle>Alterar avatar</DialogTitle>
          <DialogDescription>
            Selecione uma imagem e ajuste o recorte circular.
          </DialogDescription>
        </DialogHeader>

        <div className="py-4">
          {!imageUrl ? (
            <FileUpload
              accept={ACCEPTED_IMAGE_TYPES}
              maxFiles={1}
              maxFileSize={AVATAR_MAX_SIZE_BYTES}
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

export function ProfileCard() {
  const { data: session, isPending, refetch } = authClient.useSession();

  const uploadMutation = useMutation(
    orpc.user.v1.avatar.update.mutationOptions({
      onSuccess: async (data) => {
        await authClient.updateUser(
          { image: data.image },
          {
            onSuccess: async () => {
              await refetch();
              toast.success("Avatar atualizado");
            },
            onError: (e) => {
              toast.error(e.error.message ?? e.error.statusText);
            },
          },
        );
      },
      onError: (error) => {
        toast.error(error.message);
      },
    }),
  );

  const form = useAppForm({
    defaultValues: {
      name: session?.user.name ?? "",
    },
    validators: {
      onChange: schema,
    },
    onSubmit: async ({ value }) => {
      await authClient.updateUser(
        {
          name: value.name,
        },
        {
          onSuccess: async () => {
            await refetch();
            toast.success("Perfil atualizado");
          },
          onError: (e) => {
            toast.error(e.error.message ?? e.error.statusText);
          },
        },
      );
    },
  });

  if (isPending || !session) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Perfil</CardTitle>
          <CardDescription>Seu nome e avatar.</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <Skeleton className="h-10 w-full" />
          <Skeleton className="h-10 w-full" />
        </CardContent>
      </Card>
    );
  }

  const initial = (
    session.user.name?.[0] ?? session.user.email[0]
  ).toUpperCase();

  const handleRemoveImage = async () => {
    await authClient.updateUser(
      { image: null },
      {
        onSuccess: async () => {
          await refetch();
          toast.success("Avatar removido");
        },
        onError: (e) => {
          toast.error(e.error.message ?? e.error.statusText);
        },
      },
    );
  };

  return (
    <Card>
      <CardHeader>
        <CardTitle>Perfil</CardTitle>
        <CardDescription>Seu nome e avatar.</CardDescription>
      </CardHeader>
      <CardContent>
        <form
          action={async () => {
            await form.handleSubmit();
          }}
          className="space-y-6"
        >
          <div className="flex flex-col items-center gap-4 sm:flex-row">
            <UserAvatar
              userId={session.user.id}
              image={session.user.image}
              name={session.user.name}
              fallback={initial}
              size="lg"
            />

            <div className="flex flex-wrap items-center gap-2">
              <AvatarUploadDialog
                onConfirm={(file) => uploadMutation.mutate({ file })}
              >
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={uploadMutation.isPending}
                >
                  {uploadMutation.isPending ? (
                    <Loader2 className="mr-2 size-4 animate-spin" />
                  ) : (
                    <Camera className="mr-2 size-4" />
                  )}
                  Alterar foto
                </Button>
              </AvatarUploadDialog>

              {session.user.image && (
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={handleRemoveImage}
                >
                  <X className="mr-2 size-4" />
                  Remover
                </Button>
              )}
            </div>
          </div>

          <form.AppForm>
            <div className="grid gap-4">
              <form.AppField name="name">
                {(field) => (
                  <field.Field>
                    <field.Label>Nome de exibição</field.Label>
                    <field.Input placeholder="João da Silva" />
                    <field.Error />
                  </field.Field>
                )}
              </form.AppField>
            </div>

            <form.Button
              className="w-full sm:w-auto"
              disabled={form.state.isSubmitting}
              loading={form.state.isSubmitting}
              loadingText="Salvando..."
            >
              Salvar alterações
            </form.Button>
          </form.AppForm>
        </form>
      </CardContent>
    </Card>
  );
}
